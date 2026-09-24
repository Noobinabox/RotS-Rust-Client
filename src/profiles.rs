//! Character-only configuration overlays. Connection settings remain shared.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{MudClientError, Result};

/// An absent profile is an intentional opt-out, not an error.
pub fn existing_character_path(path: PathBuf) -> Result<Option<PathBuf>> {
    match fs::metadata(&path) {
        Ok(_) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(MudClientError::ConfigValidation(format!(
            "cannot inspect character profile {}: {error}",
            path.display()
        ))),
    }
}

/// Resolve a portable, case-normalized character identifier beneath the base
/// configuration directory. This is a local settings label, not an auto-login.
pub fn character_path(base: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(MudClientError::Cli(
            "character name must be 1-64 ASCII letters, digits, '-' or '_', starting with a letter or digit".into(),
        ));
    }
    let normalized = name.to_ascii_lowercase();
    let reserved = matches!(normalized.as_str(), "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|prefix| {
            normalized.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    if reserved {
        return Err(MudClientError::Cli(
            "character name is reserved by Windows; choose another profile label".into(),
        ));
    }
    Ok(base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("characters")
        .join(format!("{normalized}.toml")))
}

pub(crate) fn read_overrides(path: &Path) -> Result<toml::Value> {
    let raw = fs::read_to_string(path).map_err(|error| {
        MudClientError::ConfigValidation(format!(
            "cannot read character profile {}: {error}. Create this TOML file before selecting it",
            path.display(),
        ))
    })?;
    let value: toml::Value =
        toml::from_str(&raw).map_err(|source| MudClientError::ConfigParse {
            path: path.to_owned(),
            source,
        })?;
    if value.get("connection").is_some() {
        return Err(MudClientError::ConfigValidation(format!(
            "character profile {} must not contain [connection]; connection settings belong in shared config.toml",
            path.display(),
        )));
    }
    Ok(value)
}

/// Tables merge recursively; arrays and scalar values replace the shared value.
/// In particular, rule arrays never append implicitly or duplicate automation.
pub(crate) fn merge(base: &mut toml::Value, overrides: toml::Value) {
    match (base, overrides) {
        (toml::Value::Table(base), toml::Value::Table(overrides)) => {
            for (key, value) in overrides {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overrides) => *base = overrides,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ConfigLoadOptions};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mud-character-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("characters")).unwrap();
            Self(path)
        }
        fn base(&self) -> PathBuf {
            self.0.join("config.toml")
        }
        fn profile(&self) -> PathBuf {
            character_path(&self.base(), "Aragorn").unwrap()
        }
        fn load(&self) -> Result<AppConfig> {
            AppConfig::load_with_character(
                Some(self.base()),
                Some(&self.profile()),
                ConfigLoadOptions::default(),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn character_names_are_normalized_and_cannot_escape_the_directory() {
        let base = Path::new("root/config.toml");
        assert_eq!(
            character_path(base, "Aragorn_2-a").unwrap(),
            Path::new("root/characters/aragorn_2-a.toml")
        );
        for name in [
            "",
            "..",
            "../other",
            "/tmp/other",
            "a/b",
            "a\\b",
            "a.toml",
            "-x",
            " a",
            "é",
            "a\n",
        ] {
            assert!(character_path(base, name).is_err(), "{name:?}");
        }
        assert!(character_path(base, &"a".repeat(64)).is_ok());
        assert!(character_path(base, &"a".repeat(65)).is_err());
        for name in ["CON", "aux", "Nul", "prn", "COM1", "com9", "LPT1", "lpt9"] {
            assert!(character_path(base, name).is_err(), "{name}");
        }
        assert!(character_path(base, "com10").is_ok());
    }

    #[test]
    fn overrides_merge_tables_replace_arrays_and_keep_shared_connection() {
        let fixture = Fixture::new();
        fs::write(fixture.base(), "[connection]\nhost='shared.example'\nport=4321\n[variables.values]\nshared='yes'\ntarget='orc'\n[[macros.rules]]\nkey='F5'\ncommand='look'\n[panels.output]\ntitle='Shared'\nrefresh_ms=250\n").unwrap();
        fs::write(fixture.profile(), "[variables.values]\ntarget='troll'\n[panels.output]\ntitle='Aragorn'\n[[macros.rules]]\nkey='F6'\ncommand='score'\n").unwrap();
        let config = fixture.load().unwrap();
        assert_eq!(config.connection.host, "shared.example");
        assert_eq!(config.connection.port, 4321);
        assert_eq!(config.variables.values["shared"], "yes");
        assert_eq!(config.variables.values["target"], "troll");
        assert_eq!(config.panels.output.title, "Aragorn");
        assert_eq!(config.panels.output.refresh_ms, 250);
        assert_eq!(config.macros.rules.len(), 1);
        assert_eq!(config.macros.rules[0].key, "F6");
        fs::write(fixture.profile(), "[macros]\nrules=[]\n").unwrap();
        assert!(fixture.load().unwrap().macros.rules.is_empty());
        let local = AppConfig::load_with_character(
            Some(fixture.base()),
            Some(&fixture.profile()),
            ConfigLoadOptions {
                local_test_endpoint: true,
            },
        )
        .unwrap();
        assert_eq!(local.connection.host, "localhost");
        assert_eq!(local.connection.port, 3791);
    }

    #[test]
    fn missing_base_uses_defaults_but_selected_profile_is_required() {
        let fixture = Fixture::new();
        assert!(
            existing_character_path(fixture.profile())
                .unwrap()
                .is_none()
        );
        assert!(
            fixture
                .load()
                .unwrap_err()
                .to_string()
                .contains("aragorn.toml")
        );
        fs::write(fixture.profile(), "").unwrap();
        assert_eq!(
            fixture.load().unwrap().connection,
            AppConfig::default().connection
        );
        fs::remove_file(fixture.profile()).unwrap();
        fs::create_dir(fixture.profile()).unwrap();
        assert!(fixture.load().is_err());
    }

    #[test]
    fn malformed_invalid_and_connection_overrides_are_rejected() {
        let fixture = Fixture::new();
        for raw in [
            "[bad",
            "[terminal]\ntick_rate_ms=0",
            "[connection]\nhost='other'",
            "[connection]\nusername='Aragorn'",
            "[connection]",
        ] {
            fs::write(fixture.profile(), raw).unwrap();
            assert!(fixture.load().is_err(), "{raw}");
        }
        fs::write(fixture.profile(), "").unwrap();
        fs::write(fixture.base(), "[broken").unwrap();
        assert!(fixture.load().is_err());
        fs::write(fixture.base(), "[connection]\nport='bad'").unwrap();
        let error = fixture.load().unwrap_err().to_string();
        assert!(error.contains("config.toml"));
        assert!(error.contains("aragorn.toml"));
    }
}
