//! Explicit, versioned snapshots of runtime automation; never rewrites config.toml.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    config::{
        AliasRuleConfig, AppConfig, HighlightRuleConfig, SubstitutionRuleConfig, TriggerRuleConfig,
    },
    macros::{MacroConfig, MacroEngine, MacroRule},
    scripting::{
        aliases::AliasEngine, highlights::HighlightEngine, substitutions::SubstitutionEngine,
        triggers::TriggerEngine, variables::VariableStore,
    },
};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 1_048_576;
const MAX_RULES: usize = 4096;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("runtime settings {path}: {message}")]
    File { path: PathBuf, message: String },
    #[error("runtime settings are invalid: {0}")]
    Invalid(String),
    #[error("runtime settings cannot be saved without an active configuration path")]
    NoPath,
}

fn file_error(path: &Path, error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::File {
        path: path.to_owned(),
        message: error.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSettings {
    pub version: u32,
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub macros: Vec<MacroRule>,
    #[serde(default, deserialize_with = "deserialize_strict_rules")]
    pub aliases: Vec<AliasRuleConfig>,
    #[serde(default, deserialize_with = "deserialize_strict_rules")]
    pub triggers: Vec<TriggerRuleConfig>,
    #[serde(default, deserialize_with = "deserialize_strict_rules")]
    pub highlights: Vec<HighlightRuleConfig>,
    #[serde(default, deserialize_with = "deserialize_strict_rules")]
    pub substitutions: Vec<SubstitutionRuleConfig>,
}

// Main configuration remains permissive. Snapshots must reject unknown rule
// fields so a subsequent save cannot silently discard data from a newer client.
trait SavedRuleFields {
    const FIELDS: &'static [&'static str];
}

impl SavedRuleFields for AliasRuleConfig {
    const FIELDS: &'static [&'static str] = &[
        "name",
        "enabled",
        "priority",
        "match_type",
        "pattern",
        "commands",
        "lua",
    ];
}

impl SavedRuleFields for TriggerRuleConfig {
    const FIELDS: &'static [&'static str] = &[
        "name",
        "enabled",
        "priority",
        "match_type",
        "pattern",
        "foreground",
        "background",
        "commands",
        "event",
        "lua",
        "cooldown_ms",
        "one_shot",
        "categories",
    ];
}

impl SavedRuleFields for HighlightRuleConfig {
    const FIELDS: &'static [&'static str] = &[
        "name",
        "enabled",
        "priority",
        "match_type",
        "pattern",
        "foreground",
        "background",
        "bold",
        "dim",
        "italic",
        "underline",
        "reverse",
        "categories",
    ];
}

impl SavedRuleFields for SubstitutionRuleConfig {
    const FIELDS: &'static [&'static str] = &[
        "name",
        "enabled",
        "priority",
        "match_type",
        "pattern",
        "replacement",
        "foreground",
        "background",
        "categories",
    ];
}

fn deserialize_strict_rules<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + SavedRuleFields,
{
    use serde::de::Error;
    Vec::<toml::Value>::deserialize(deserializer)?
        .into_iter()
        .map(|value| {
            let table = value
                .as_table()
                .ok_or_else(|| D::Error::custom("expected rule table"))?;
            if table.keys().any(|key| !T::FIELDS.contains(&key.as_str())) {
                return Err(D::Error::custom("unknown saved rule field"));
            }
            value.try_into().map_err(D::Error::custom)
        })
        .collect()
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            variables: Default::default(),
            macros: Vec::new(),
            aliases: Vec::new(),
            triggers: Vec::new(),
            highlights: Vec::new(),
            substitutions: Vec::new(),
        }
    }
}

pub struct RuntimeEngines {
    pub variables: VariableStore,
    pub macros: MacroEngine,
    pub aliases: AliasEngine,
    pub triggers: TriggerEngine,
    pub highlights: HighlightEngine,
    pub substitutions: SubstitutionEngine,
}

impl RuntimeSettings {
    pub fn count(&self) -> usize {
        self.variables.len()
            + self.macros.len()
            + self.aliases.len()
            + self.triggers.len()
            + self.highlights.len()
            + self.substitutions.len()
    }

    /// Restore variables before compiling dependent rules; replace nothing live
    /// until the complete snapshot has passed validation.
    pub fn compile(&self, config: &AppConfig) -> Result<RuntimeEngines, PersistenceError> {
        if self.version != FORMAT_VERSION {
            return Err(PersistenceError::Invalid(format!(
                "unsupported version {}; expected {FORMAT_VERSION}",
                self.version
            )));
        }
        if self.count() > MAX_RULES {
            return Err(PersistenceError::Invalid(format!(
                "at most {MAX_RULES} rules and variables may be saved"
            )));
        }
        let mut validation = config.clone();
        validation.variables.values.extend(self.variables.clone());
        validation.macros.rules = self.macros.clone();
        validation.aliases.rules = self.aliases.clone();
        validation.triggers.rules = self.triggers.clone();
        validation.highlights.rules = self.highlights.clone();
        validation.substitutions.rules = self.substitutions.clone();
        validation
            .validate()
            .map_err(|error| PersistenceError::Invalid(error.to_string()))?;
        // Validate normalized key uniqueness, including aliases such as NumpadUp.
        MacroEngine::new(&MacroConfig {
            rules: self.macros.clone(),
        })
        .map_err(PersistenceError::Invalid)?;
        unique(
            self.aliases.iter().map(|rule| rule.name.as_str()),
            "alias name",
        )?;
        unique(
            self.triggers.iter().map(|rule| rule.pattern.as_str()),
            "trigger pattern",
        )?;
        unique(
            self.highlights.iter().map(|rule| rule.pattern.as_str()),
            "highlight pattern",
        )?;
        unique(
            self.substitutions.iter().map(|rule| rule.pattern.as_str()),
            "substitution pattern",
        )?;
        let variables =
            VariableStore::with_runtime_values(&config.variables, self.variables.clone())
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?;
        let mut engines = RuntimeEngines {
            macros: MacroEngine::new(&config.macros).map_err(PersistenceError::Invalid)?,
            aliases: AliasEngine::new_with_variables(&config.aliases, &variables)
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?,
            triggers: TriggerEngine::new_with_variables(&config.triggers, &variables)
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?,
            highlights: HighlightEngine::new(&config.highlights)
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?,
            substitutions: SubstitutionEngine::new(&config.substitutions)
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?,
            variables,
        };
        for rule in &self.macros {
            engines
                .macros
                .add(rule.clone())
                .map_err(PersistenceError::Invalid)?;
        }
        for rule in &self.aliases {
            engines
                .aliases
                .add_runtime_config(rule.clone())
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?;
        }
        for rule in &self.triggers {
            engines
                .triggers
                .add_runtime_config(rule.clone())
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?;
        }
        for rule in &self.highlights {
            engines
                .highlights
                .add_runtime_highlight(rule.clone())
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?;
        }
        for rule in &self.substitutions {
            engines
                .substitutions
                .add_runtime_substitution(rule.clone())
                .map_err(|e| PersistenceError::Invalid(e.to_string()))?;
        }
        Ok(engines)
    }
}

fn unique<'a>(values: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), PersistenceError> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(PersistenceError::Invalid(format!("duplicate {kind}")));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RuntimeStore {
    path: Option<PathBuf>,
    expected: Option<Vec<u8>>,
    blocked: bool,
}

impl RuntimeStore {
    pub fn new(config_path: Option<&Path>) -> Self {
        Self {
            path: config_path.map(|path| suffixed(path, ".runtime.toml")),
            expected: None,
            blocked: false,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn load(&mut self, config: &AppConfig) -> Result<Option<RuntimeEngines>, PersistenceError> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        // A failed load must never permit overwriting an unrecognized/corrupt file.
        self.blocked = true;
        let bytes = read_snapshot(path)?;
        let engines = match &bytes {
            Some(bytes) => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| file_error(path, "expected UTF-8 TOML"))?;
                let settings: RuntimeSettings = toml::from_str(text).map_err(|_| {
                    file_error(
                        path,
                        "invalid runtime TOML; repair or move this file and restart",
                    )
                })?;
                Some(
                    settings
                        .compile(config)
                        .map_err(|error| file_error(path, error))?,
                )
            }
            None => None,
        };
        self.expected = bytes;
        self.blocked = false;
        Ok(engines)
    }

    /// Blocking filesystem operation: callers must run this off the async executor.
    pub fn save(&mut self, settings: &RuntimeSettings) -> Result<(), PersistenceError> {
        self.save_with_writer(settings, |file, bytes| {
            file.write_all(bytes)?;
            file.sync_all()
        })
    }

    fn save_with_writer(
        &mut self,
        settings: &RuntimeSettings,
        write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
    ) -> Result<(), PersistenceError> {
        let path = self.path.as_deref().ok_or(PersistenceError::NoPath)?;
        if self.blocked {
            return Err(file_error(
                path,
                "startup load failed; repair or move the saved file and restart before saving",
            ));
        }
        let text = toml::to_string_pretty(settings)
            .map_err(|_| file_error(path, "could not serialize runtime settings"))?;
        if text.len() > MAX_FILE_BYTES {
            return Err(file_error(path, "snapshot exceeds the 1 MiB size limit"));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| file_error(parent, e))?;
        let lock_path = suffixed(path, ".lock");
        let lock = create_private(&lock_path).map_err(|e| file_error(&lock_path, format!("cannot acquire save lock ({e}); another save may be active; remove a stale lock only when no client is saving")))?;
        let _lock = TemporaryFile {
            path: lock_path,
            file: Some(lock),
        };
        if read_snapshot(path)? != self.expected {
            return Err(file_error(
                path,
                "file changed outside this session; save refused; back up your edits and restart to load the current file",
            ));
        }
        let temp_path = suffixed(
            path,
            &format!(
                ".{}.{}.tmp",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ),
        );
        let file = create_private(&temp_path).map_err(|e| file_error(&temp_path, e))?;
        let mut temp = TemporaryFile {
            path: temp_path,
            file: Some(file),
        };
        if let Some(file) = temp.file.as_mut() {
            write(file, text.as_bytes()).map_err(|e| file_error(path, e))?;
        }
        // Close before rename for Windows; rename never exposes a partial snapshot.
        drop(temp.file.take());
        fs::rename(&temp.path, path).map_err(|e| file_error(path, e))?;
        self.expected = Some(text.into_bytes());
        Ok(())
    }
}

fn read_snapshot(path: &Path) -> Result<Option<Vec<u8>>, PersistenceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(file_error(path, error)),
    };
    if !metadata.is_file() {
        return Err(file_error(
            path,
            "expected a regular file, not a directory or symbolic link",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| file_error(path, e))?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| file_error(path, e))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(file_error(path, "snapshot exceeds the 1 MiB size limit"));
    }
    Ok(Some(bytes))
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

struct TemporaryFile {
    path: PathBuf,
    file: Option<File>,
}
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AliasMatchType, MatchType};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mud-runtime-store-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample() -> RuntimeSettings {
        RuntimeSettings {
            macros: vec![MacroRule {
                key: "Numpad8".into(),
                command: "north".into(),
                ..MacroRule::default()
            }],
            aliases: vec![AliasRuleConfig {
                name: "runtime:rr".into(),
                pattern: "rr".into(),
                match_type: AliasMatchType::Exact,
                commands: vec!["look".into()],
                ..AliasRuleConfig::default()
            }],
            highlights: vec![HighlightRuleConfig {
                name: "runtime:danger".into(),
                pattern: "danger".into(),
                foreground: Some("red".into()),
                ..HighlightRuleConfig::default()
            }],
            substitutions: vec![SubstitutionRuleConfig {
                name: "runtime:orc".into(),
                pattern: "orc".into(),
                replacement: "goblin".into(),
                ..SubstitutionRuleConfig::default()
            }],
            ..RuntimeSettings::default()
        }
    }

    #[test]
    fn round_trip_and_empty_save_preserve_configuration_bytes() {
        let dir = TestDir::new();
        let config_path = dir.config();
        fs::write(&config_path, "# preserve this comment\n").unwrap();
        let config = AppConfig::default();
        let mut store = RuntimeStore::new(Some(&config_path));
        assert!(store.load(&config).unwrap().is_none());
        let settings = sample();
        settings.compile(&config).unwrap();
        store.save(&settings).unwrap();
        let bytes = fs::read(store.path().unwrap()).unwrap();
        let restored: RuntimeSettings =
            toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_eq!(restored, settings);
        let mut restarted = RuntimeStore::new(Some(&config_path));
        let engines = restarted.load(&config).unwrap().unwrap();
        assert_eq!(engines.aliases.expand("rr").unwrap(), ["look"]);
        assert_eq!(
            engines
                .substitutions
                .apply(
                    "orc",
                    crate::state::OutputCategory::Normal,
                    &crate::color::AnsiColors::default(),
                    &engines.variables
                )
                .unwrap(),
            "goblin"
        );
        store.save(&RuntimeSettings::default()).unwrap();
        assert!(
            RuntimeStore::new(Some(&config_path))
                .load(&config)
                .unwrap()
                .unwrap()
                .aliases
                .runtime_configs()
                .is_empty()
        );
        assert_eq!(
            fs::read_to_string(config_path).unwrap(),
            "# preserve this comment\n"
        );
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn concurrent_session_or_external_changes_are_not_overwritten() {
        let dir = TestDir::new();
        let mut first = RuntimeStore::new(Some(&dir.config()));
        let mut second = first.clone();
        first.load(&AppConfig::default()).unwrap();
        second.load(&AppConfig::default()).unwrap();
        first.save(&sample()).unwrap();
        let original = fs::read(first.path().unwrap()).unwrap();
        assert!(
            second
                .save(&RuntimeSettings::default())
                .unwrap_err()
                .to_string()
                .contains("changed outside")
        );
        assert_eq!(fs::read(first.path().unwrap()).unwrap(), original);
        fs::write(first.path().unwrap(), "version = 1\n# edited externally\n").unwrap();
        assert!(first.save(&sample()).is_err());
        assert!(
            fs::read_to_string(first.path().unwrap())
                .unwrap()
                .contains("externally")
        );
    }

    #[test]
    fn partial_write_and_busy_lock_keep_previous_snapshot_and_clean_up() {
        let dir = TestDir::new();
        let mut store = RuntimeStore::new(Some(&dir.config()));
        store.load(&AppConfig::default()).unwrap();
        store.save(&sample()).unwrap();
        let path = store.path().unwrap().to_owned();
        let original = fs::read(&path).unwrap();
        assert!(
            store
                .save_with_writer(&RuntimeSettings::default(), |file, bytes| {
                    file.write_all(&bytes[..4])?;
                    Err(std::io::Error::other("injected disk failure"))
                })
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        let lock = suffixed(&path, ".lock");
        fs::write(&lock, "occupied").unwrap();
        assert!(
            store
                .save(&RuntimeSettings::default())
                .unwrap_err()
                .to_string()
                .contains("lock")
        );
        assert_eq!(fs::read_to_string(lock).unwrap(), "occupied");
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn invalid_or_unsupported_files_block_overwrites() {
        let dir = TestDir::new();
        let mut store = RuntimeStore::new(Some(&dir.config()));
        let path = store.path().unwrap().to_owned();
        for text in [
            "bad TOML [",
            "version = 2",
            "version = 1\nunknown = true",
            "version = 1\n[variables]\nfood = 42",
            "version = 1\n[variables]\nfood = '${food}'",
            "version = 1\n[variables]\nfood = '${missing}'",
            "version = 1\n[variables]\n'bad name' = 'bread'",
            "version = 1\n[[aliases]]\nname = 'look'\npattern = '^look$'\ncommands = ['look']\nunknown = true",
            "version = 1\n[[triggers]]\nname = 'hungry'\npattern = 'hungry'\ncommands = ['eat bread']\nunknown = true",
            "version = 1\n[[highlights]]\nname = 'danger'\npattern = 'danger'\nforeground = 'red'\nunknown = true",
            "version = 1\n[[substitutions]]\nname = 'orc'\npattern = 'orc'\nreplacement = 'goblin'\nunknown = true",
            "version = 1\n[[substitutions]]\nname = 'orc'\npattern = 'orc'\nreplacement = '{1}'",
            "version = 1\n[[macros]]\nkey = 'Ctrl+C'\ncommand = 'look'",
            "version = 1\n[[aliases]]\nname = 'bad'\npattern = '['\ncommands = ['look']",
        ] {
            fs::write(&path, text).unwrap();
            assert!(store.load(&AppConfig::default()).is_err(), "{text}");
            assert!(store.save(&sample()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        fs::write(&path, [0xff]).unwrap();
        assert!(store.load(&AppConfig::default()).is_err());
        fs::write(&path, vec![b' '; MAX_FILE_BYTES + 1]).unwrap();
        assert!(
            store
                .load(&AppConfig::default())
                .err()
                .unwrap()
                .to_string()
                .contains("1 MiB")
        );
    }

    #[test]
    fn limits_duplicate_keys_and_restart_variable_dependencies_are_validated() {
        let mut substitutions = sample();
        substitutions
            .substitutions
            .push(substitutions.substitutions[0].clone());
        assert!(substitutions.compile(&AppConfig::default()).is_err());
        let mut settings = sample();
        settings.macros.push(MacroRule {
            key: "numpadup".into(),
            command: "look".into(),
            ..MacroRule::default()
        });
        assert!(settings.compile(&AppConfig::default()).is_err());
        let mut settings = sample();
        settings.aliases[0].commands = vec!["kill ${target}".into()];
        assert!(settings.compile(&AppConfig::default()).is_err());
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".into(), "orc".into());
        assert!(settings.compile(&config).is_ok());
        settings.aliases[0].commands =
            vec!["look".into(); config.aliases.max_expanded_commands + 1];
        assert!(settings.compile(&config).is_err());
        let mut settings = sample();
        settings.highlights[0].match_type = MatchType::Regex;
        settings.highlights[0].pattern = "[".into();
        assert!(settings.compile(&config).is_err());
        settings.macros = vec![MacroRule::default(); MAX_RULES + 1];
        assert!(settings.compile(&config).is_err());
    }

    #[test]
    fn missing_path_or_directory_target_fail_safely() {
        assert!(matches!(
            RuntimeStore::new(None).save(&sample()),
            Err(PersistenceError::NoPath)
        ));
        let dir = TestDir::new();
        let mut store = RuntimeStore::new(Some(&dir.config()));
        fs::create_dir(store.path().unwrap()).unwrap();
        assert!(store.load(&AppConfig::default()).is_err());
        assert!(store.save(&sample()).is_err());
    }

    #[test]
    fn old_snapshots_default_to_no_variables_and_variable_limits_are_enforced() {
        let old: RuntimeSettings = toml::from_str("version = 1").unwrap();
        assert!(old.variables.is_empty());
        assert!(old.substitutions.is_empty());
        let mut settings = RuntimeSettings::default();
        settings.variables.insert("food".into(), "bread".into());
        let mut config = AppConfig::default();
        config.variables.max_expanded_bytes = 4;
        assert!(settings.compile(&config).is_err());
        settings.variables = (0..=MAX_RULES)
            .map(|n| (format!("v{n}"), "x".into()))
            .collect();
        assert!(settings.compile(&AppConfig::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn saves_are_private_and_symlink_targets_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = TestDir::new();
        let mut store = RuntimeStore::new(Some(&dir.config()));
        store.load(&AppConfig::default()).unwrap();
        store.save(&sample()).unwrap();
        let path = store.path().unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link_config = dir.0.join("linked.toml");
        let mut linked = RuntimeStore::new(Some(&link_config));
        symlink(path, linked.path().unwrap()).unwrap();
        assert!(linked.load(&AppConfig::default()).is_err());
        assert!(linked.save(&sample()).is_err());
    }
}
