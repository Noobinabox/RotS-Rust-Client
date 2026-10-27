//! Bounded command history and private, versioned per-character history files.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

pub const MAX_HISTORY_BYTES: usize = 1_048_576;
const MAX_FILE_BYTES: u64 = 8 * 1_048_576;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HistoryConfig {
    pub persist: bool,
    pub max_entries: usize,
    pub ignore_prefixes: Vec<String>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            persist: false,
            max_entries: 1000,
            ignore_prefixes: vec!["password ".into(), "passwd ".into()],
        }
    }
}

impl HistoryConfig {
    pub fn accepts(&self, command: &str) -> bool {
        !command.trim().is_empty()
            && command.len() <= crate::input::MAX_PASTE_BYTES
            && !command
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            && !command.lines().any(|line| {
                line.starts_with(' ')
                    || self
                        .ignore_prefixes
                        .iter()
                        .any(|prefix| line.starts_with(prefix))
            })
    }

    /// Keep newest entries; shared by input, disk loading, and configuration reload.
    pub fn trim(&self, entries: &mut Vec<String>) {
        entries.retain(|command| self.accepts(command));
        entries.dedup();
        let mut bytes = 0usize;
        let keep = entries
            .iter()
            .rev()
            .take(self.max_entries)
            .take_while(|entry| {
                bytes = bytes.saturating_add(entry.len());
                bytes <= MAX_HISTORY_BYTES
            })
            .count();
        entries.drain(..entries.len().saturating_sub(keep));
    }

    pub fn record(&self, entries: &mut Vec<String>, command: &str) {
        if self.accepts(command) && entries.last().is_none_or(|last| last != command) {
            entries.push(command.to_owned());
            self.trim(entries);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryFile {
    version: u32,
    commands: Vec<String>,
}

/// The last observed file contents also serve as a conflict token.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

impl HistoryStore {
    pub fn load(
        base: &Path,
        character: &str,
        config: &HistoryConfig,
    ) -> io::Result<(Self, Vec<String>)> {
        // Reuse the profile identifier validation, even for characters without profiles.
        let profile = crate::profiles::character_path(base, character)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        let name = profile
            .file_name()
            .ok_or_else(|| io::Error::other("missing history filename"))?;
        let path = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("history")
            .join(name);
        if let Some(parent) = path.parent() {
            match fs::symlink_metadata(parent) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                    return Err(io::Error::other(
                        "history directory must be a regular directory",
                    ));
                }
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        let previous = read_file(&path)?;
        let mut commands = match &previous {
            Some(bytes) => {
                let text = std::str::from_utf8(bytes).map_err(io::Error::other)?;
                let file: HistoryFile = toml::from_str(text).map_err(io::Error::other)?;
                if file.version != 1 {
                    return Err(io::Error::other("unsupported command history version"));
                }
                file.commands
            }
            None => Vec::new(),
        };
        config.trim(&mut commands);
        Ok((Self { path, previous }, commands))
    }

    pub fn save(&mut self, commands: Vec<String>) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("missing history directory"))?;
        fs::create_dir_all(parent)?;
        if fs::symlink_metadata(parent)?.file_type().is_symlink() {
            return Err(io::Error::other("history directory must not be a symlink"));
        }
        let lock_path = self.path.with_extension("toml.lock");
        let _lock_file = private_file(&lock_path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot acquire history lock: {error}"),
            )
        })?;
        drop(_lock_file);
        let _lock = RemoveFile(lock_path);
        if read_file(&self.path)? != self.previous {
            return Err(io::Error::other(
                "history changed in another client; restart to load it before saving",
            ));
        }
        let bytes = toml::to_string(&HistoryFile {
            version: 1,
            commands,
        })
        .map_err(io::Error::other)?
        .into_bytes();
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("history exceeds the 8 MiB file limit"));
        }
        let temp = self.path.with_extension(format!(
            "toml.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = private_file(&temp)?;
        let _cleanup = RemoveFile(temp.clone());
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &self.path)?;
        self.previous = Some(bytes);
        Ok(())
    }
}

fn private_file(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err(io::Error::other("history path must be a regular file"));
        }
        Ok(_) => {}
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::other("history exceeds the 8 MiB file limit"));
    }
    Ok(Some(bytes))
}

struct RemoveFile(PathBuf);
impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct Fixture(pub PathBuf);
    impl Fixture {
        pub(crate) fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mud-history-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        pub(crate) fn base(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn history_retains_newest_deduplicates_and_filters_entire_multiline_entries() {
        let config = HistoryConfig {
            max_entries: 2,
            ..HistoryConfig::default()
        };
        let mut entries = Vec::new();
        for command in [
            "look",
            "look",
            "say café",
            "score\nlook",
            " password",
            "password secret",
            "look\n secret",
            "bad\u{1b}",
            " ",
        ] {
            config.record(&mut entries, command);
        }
        assert_eq!(entries, ["say café", "score\nlook"]);
        HistoryConfig {
            max_entries: 0,
            ..config
        }
        .trim(&mut entries);
        assert!(entries.is_empty());
    }

    #[test]
    fn history_text_and_entry_bytes_are_bounded() {
        let config = HistoryConfig {
            max_entries: 10000,
            ..HistoryConfig::default()
        };
        let mut entries = Vec::new();
        config.record(&mut entries, &"x".repeat(crate::input::MAX_PASTE_BYTES + 1));
        assert!(entries.is_empty());
        for index in 0..40 {
            config.record(&mut entries, &format!("{index}{}", "x".repeat(60_000)));
        }
        assert!(entries.iter().map(String::len).sum::<usize>() <= MAX_HISTORY_BYTES);
        assert!(entries.last().unwrap().starts_with("39"));
        assert!(entries.len() < 40);
    }

    #[test]
    fn history_store_roundtrips_and_detects_conflicts() {
        let fixture = Fixture::new();
        let config = HistoryConfig::default();
        let (mut first, entries) = HistoryStore::load(&fixture.base(), "Hero", &config).unwrap();
        assert!(entries.is_empty());
        let (mut stale, _) = HistoryStore::load(&fixture.base(), "hero", &config).unwrap();
        let commands = vec!["say café".into(), "look\nscore".into()];
        first.save(commands.clone()).unwrap();
        assert_eq!(
            HistoryStore::load(&fixture.base(), "hero", &config)
                .unwrap()
                .1,
            commands
        );
        assert!(
            stale
                .save(vec!["stale".into()])
                .unwrap_err()
                .to_string()
                .contains("another client")
        );
        first.save(vec!["updated".into()]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(!first.path.with_extension("toml.lock").exists());
    }

    #[test]
    fn invalid_history_paths_files_and_locks_leave_previous_contents_untouched() {
        let fixture = Fixture::new();
        let config = HistoryConfig::default();
        assert!(HistoryStore::load(&fixture.base(), "../escape", &config).is_err());
        let (mut store, _) = HistoryStore::load(&fixture.base(), "hero", &config).unwrap();
        store.save(vec!["look".into()]).unwrap();
        let previous = fs::read(&store.path).unwrap();
        fs::write(store.path.with_extension("toml.lock"), "").unwrap();
        assert!(store.save(vec!["score".into()]).is_err());
        assert_eq!(fs::read(&store.path).unwrap(), previous);
        fs::remove_file(store.path.with_extension("toml.lock")).unwrap();
        for contents in [
            "invalid",
            "version=2\ncommands=[]",
            "version=1\ncommands=[]\nunknown=true",
        ] {
            fs::write(&store.path, contents).unwrap();
            assert!(HistoryStore::load(&fixture.base(), "hero", &config).is_err());
            assert_eq!(fs::read_to_string(&store.path).unwrap(), contents);
        }
        fs::File::create(&store.path)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(HistoryStore::load(&fixture.base(), "hero", &config).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn history_rejects_symlink_files() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("history")).unwrap();
        let target = fixture.0.join("target");
        fs::write(&target, "private").unwrap();
        std::os::unix::fs::symlink(&target, fixture.0.join("history/hero.toml")).unwrap();
        assert!(HistoryStore::load(&fixture.base(), "hero", &HistoryConfig::default()).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "private");
    }

    #[test]
    fn history_config_validates_limits() {
        for raw in [
            "[history]\nmax_entries=10001",
            "[history]\nignore_prefixes=['']",
        ] {
            assert!(
                toml::from_str::<crate::config::AppConfig>(raw)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        assert!(!HistoryConfig::default().persist);
    }
}
