use mud_client::config::{AppConfig, InputMode};

#[test]
fn installer_defaults_are_valid_and_do_not_enable_automation() {
    let config: AppConfig = toml::from_str(include_str!("../install/default-config.toml")).unwrap();
    config.validate().unwrap();
    assert_eq!(config.connection.host, "rotsmud.org");
    assert_eq!(config.connection.port, 3791);
    assert_eq!(config.terminal.input_mode, InputMode::Standard);
    assert!(!config.lua.enabled);
    assert!(config.aliases.rules.is_empty());
    assert!(config.triggers.rules.is_empty());
    assert!(config.events.handlers.is_empty());
    assert!(config.connection.username.is_empty());
    assert!(config.connection.password.is_empty());
}

#[cfg(unix)]
mod shell_installer {
    use std::{
        collections::BTreeMap,
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::{Command, Output, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "mud-client-installer-{}-{nonce}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let fixture = Self { root };
            fs::create_dir(fixture.root.join("tools")).unwrap();
            for (name, body) in [
                ("uname", "printf '%s\\n' Linux"),
                ("id", "printf '%s\\n' 1000"),
                ("cc", "exit 0"),
                ("make", "exit 0"),
                ("curl", "echo 'Unexpected download attempt' >&2; exit 90"),
                (
                    "sudo",
                    "echo 'Unexpected package installation' >&2; exit 91",
                ),
                (
                    "rustup",
                    r#"
set -eu
if [ "$1" != run ] || [ "$2" != stable ] || [ "$3" != cargo ]; then
    echo 'Unexpected rustup invocation' >&2
    exit 92
fi
case "$4" in
    --version) printf '%s\n' 'cargo mock' ;;
    install)
        if [ "${MOCK_FAIL_CARGO:-0}" = 1 ]; then exit 93; fi
        mkdir -p "$CARGO_HOME/bin"
        printf '%s\n' '#!/bin/sh' '[ "$1" = --help ] || exit 94' 'echo mud-client' > "$CARGO_HOME/bin/mud-client"
        chmod +x "$CARGO_HOME/bin/mud-client"
        ;;
    *) echo 'Unexpected Cargo invocation' >&2; exit 95 ;;
esac
"#,
                ),
            ] {
                let path = fixture.root.join("tools").join(name);
                fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            fixture
        }

        fn config_dir(&self) -> PathBuf {
            self.root.join("configuration/mud-client")
        }

        fn run(&self, args: &[&str], answer: &str, fail_cargo: bool) -> Output {
            let mut child = Command::new("/bin/bash")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
                .args(args)
                .env(
                    "PATH",
                    format!("{}:/usr/bin:/bin", self.root.join("tools").display()),
                )
                .env("CARGO_HOME", self.root.join("cargo"))
                .env("XDG_CONFIG_HOME", self.root.join("configuration"))
                .env("MOCK_FAIL_CARGO", if fail_cargo { "1" } else { "0" })
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            if !answer.is_empty() {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(answer.as_bytes())
                    .unwrap();
            }
            child.wait_with_output().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // This exact, uniquely-created fixture path is the only cleanup target.
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, current: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
            if !current.exists() {
                return;
            }
            for entry in fs::read_dir(current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, result);
                } else {
                    result.insert(
                        path.strip_prefix(root).unwrap().to_owned(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut result = BTreeMap::new();
        visit(root, root, &mut result);
        result
    }

    fn assert_success(output: Output) {
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn fresh_install_copies_safe_defaults_and_bundled_scripts() {
        let fixture = Fixture::new();
        assert_success(fixture.run(&["--skip-prerequisites"], "y\n", false));
        assert_eq!(
            fs::read(fixture.config_dir().join("config.toml")).unwrap(),
            include_bytes!("../install/default-config.toml")
        );
        assert!(fixture.root.join("cargo/bin/mud-client").is_file());
        assert_eq!(
            fs::read(fixture.config_dir().join("LICENSE")).unwrap(),
            include_bytes!("../LICENSE")
        );
        for entry in fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts")).unwrap() {
            let source = entry.unwrap().path();
            if source
                .extension()
                .is_some_and(|extension| extension == "lua")
            {
                assert_eq!(
                    fs::read(&source).unwrap(),
                    fs::read(
                        fixture
                            .config_dir()
                            .join("scripts")
                            .join(source.file_name().unwrap())
                    )
                    .unwrap()
                );
            }
        }
    }

    #[test]
    fn reinstall_preserves_config_scripts_runtime_profiles_and_maps() {
        let fixture = Fixture::new();
        assert_success(fixture.run(&["--skip-prerequisites"], "y\n", false));
        for relative in [
            "LICENSE",
            "config.toml",
            "scripts/init.lua",
            "runtime.toml",
            "characters/ranger.toml",
            "maps/world.toml",
        ] {
            let path = fixture.config_dir().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("user-owned {relative}\n")).unwrap();
        }
        let before = files(&fixture.config_dir());
        assert_success(fixture.run(&["--skip-prerequisites"], "y\n", false));
        assert_eq!(before, files(&fixture.config_dir()));
    }

    #[test]
    fn interrupted_copy_never_publishes_partial_config_and_retry_recovers() {
        let fixture = Fixture::new();
        let broken_copy = fixture.root.join("tools/cp");
        fs::write(
            &broken_copy,
            "#!/bin/sh\nfor last do :; done\nprintf partial > \"$last\"\nexit 96\n",
        )
        .unwrap();
        fs::set_permissions(&broken_copy, fs::Permissions::from_mode(0o755)).unwrap();
        let output = fixture.run(&["--skip-prerequisites"], "y\n", false);
        assert!(!output.status.success());
        assert!(!fixture.config_dir().join("config.toml").exists());
        assert!(files(&fixture.config_dir()).is_empty());
        fs::remove_file(broken_copy).unwrap();
        assert_success(fixture.run(&["--skip-prerequisites"], "y\n", false));
        assert_eq!(
            fs::read(fixture.config_dir().join("config.toml")).unwrap(),
            include_bytes!("../install/default-config.toml")
        );
    }

    #[test]
    fn failed_build_does_not_create_or_change_configuration() {
        let fixture = Fixture::new();
        assert!(
            !fixture
                .run(&["--skip-prerequisites"], "y\n", true)
                .status
                .success()
        );
        assert!(!fixture.config_dir().exists());
        fs::create_dir_all(fixture.config_dir()).unwrap();
        fs::write(fixture.config_dir().join("config.toml"), "user config").unwrap();
        let before = files(&fixture.config_dir());
        assert!(
            !fixture
                .run(&["--skip-prerequisites"], "y\n", true)
                .status
                .success()
        );
        assert_eq!(before, files(&fixture.config_dir()));
    }

    #[test]
    fn check_and_decline_do_not_write_installation_files() {
        let fixture = Fixture::new();
        let before = files(&fixture.root);
        assert_success(fixture.run(&["--check"], "", false));
        assert_eq!(before, files(&fixture.root));
        assert!(!fixture.root.join("cargo").exists());
        assert!(!fixture.root.join("configuration").exists());
        assert!(
            !fixture
                .run(&["--skip-prerequisites"], "n\n", false)
                .status
                .success()
        );
        assert_eq!(before, files(&fixture.root));
        assert!(!fixture.root.join("cargo").exists());
        assert!(!fixture.root.join("configuration").exists());
    }
}
