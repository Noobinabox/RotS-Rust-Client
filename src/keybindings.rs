//! Custom shortcut overlays. Application dispatch retains legacy defaults.
use std::collections::HashMap;

use crossterm::event::{KeyEvent, KeyEventKind};
use serde::{Deserialize, Serialize};

use crate::macros::{KeyBinding, MacroRule};

const MAX_BINDINGS: usize = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeybindingConfig {
    pub bindings: Vec<KeybindingRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindingRule {
    pub key: String,
    pub action: ShortcutAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    None,
    Submit,
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    LineStart,
    LineEnd,
    WordLeft,
    WordRight,
    DeleteWordLeft,
    DeleteWordRight,
    HistoryPrevious,
    HistoryNext,
    CompleteNext,
    CompletePrevious,
    ScrollPageUp,
    ScrollPageDown,
    ScrollLineUp,
    ScrollLineDown,
    FollowOutput,
    ClearOutput,
    SearchOutput,
    SearchNext,
    SearchPrevious,
    ToggleOutputDisplayMode,
    Reconnect,
    Reload,
    Quit,
}

impl ShortcutAction {
    pub fn is_editor(self) -> bool {
        matches!(
            self,
            Self::Submit
                | Self::Backspace
                | Self::Delete
                | Self::CursorLeft
                | Self::CursorRight
                | Self::LineStart
                | Self::LineEnd
                | Self::WordLeft
                | Self::WordRight
                | Self::DeleteWordLeft
                | Self::DeleteWordRight
                | Self::HistoryPrevious
                | Self::HistoryNext
                | Self::CompleteNext
                | Self::CompletePrevious
        )
    }

    /// Sending commands and changing application modes require a fresh press.
    pub fn allow_repeat(self) -> bool {
        (self.is_editor() && self != Self::Submit)
            || matches!(
                self,
                Self::ScrollPageUp
                    | Self::ScrollPageDown
                    | Self::ScrollLineUp
                    | Self::ScrollLineDown
                    | Self::SearchNext
                    | Self::SearchPrevious
            )
    }
}

#[derive(Debug, Default)]
pub struct KeybindingEngine {
    bindings: HashMap<KeyBinding, ShortcutAction>,
}

impl KeybindingEngine {
    pub fn new(config: &KeybindingConfig) -> Result<Self, String> {
        if config.bindings.len() > MAX_BINDINGS {
            return Err(format!(
                "keybindings.bindings supports at most {MAX_BINDINGS} entries"
            ));
        }
        let mut engine = Self::default();
        for rule in &config.bindings {
            let key = KeyBinding::parse(&rule.key)
                .map_err(|error| format!("keybindings.bindings: {error}"))?;
            if key.protected() {
                return Err(format!(
                    "Shortcut key `{}` is reserved for Ctrl+C or Escape recovery",
                    rule.key
                ));
            }
            if key.printable() {
                return Err(format!(
                    "Shortcut key `{}` would replace ordinary text input; use Ctrl, Alt, a function key, or Numpad",
                    rule.key
                ));
            }
            if engine.bindings.insert(key, rule.action).is_some() {
                return Err(format!("Duplicate shortcut key `{}`", rule.key));
            }
        }
        Ok(engine)
    }

    /// Returns only configured overrides, including explicit `none` bindings.
    /// Release events never dispatch. Callers apply the action's repeat policy.
    pub fn lookup(&self, event: KeyEvent) -> Option<ShortcutAction> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        self.bindings
            .get(&KeyBinding::from_event(event))
            .or_else(|| self.bindings.get(&KeyBinding::generic_event(event)))
            .copied()
    }

    pub fn is_bound(&self, event: KeyEvent) -> bool {
        self.lookup(event).is_some()
    }

    /// Explicitly unbound shortcuts still require macro override permission.
    pub fn validate_macro(&self, rule: &MacroRule) -> Result<(), String> {
        if !rule.enabled || rule.override_builtin {
            return Ok(());
        }
        let key = KeyBinding::parse(&rule.key)?;
        if self.bindings.contains_key(&key) {
            return Err(format!(
                "Macro key `{}` replaces a configured shortcut; use override_builtin = true or --override",
                rule.key
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crossterm::event::{KeyCode, KeyEventState, KeyModifiers};

    fn config(keys: &[&str]) -> KeybindingConfig {
        KeybindingConfig {
            bindings: keys
                .iter()
                .map(|key| KeybindingRule {
                    key: (*key).into(),
                    action: ShortcutAction::None,
                })
                .collect(),
        }
    }

    #[test]
    fn configured_none_and_empty_defaults_are_distinct() {
        let key = KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE);
        assert_eq!(KeybindingEngine::default().lookup(key), None);
        let engine = KeybindingEngine::new(&config(&["F6"])).unwrap();
        assert_eq!(engine.lookup(key), Some(ShortcutAction::None));
        assert!(engine.is_bound(key));
        assert_eq!(
            engine.lookup(KeyEvent {
                kind: KeyEventKind::Release,
                ..key
            }),
            None
        );
    }

    #[test]
    fn canonical_duplicates_and_invalid_keys_are_rejected() {
        for keys in [
            vec!["Ctrl+A", "control+a"],
            vec!["BackTab", "Shift+Tab"],
            vec!["NumpadUp", "Numpad8"],
            vec!["F25"],
            vec!["Ctrl+Ctrl+A"],
            vec![""],
        ] {
            assert!(KeybindingEngine::new(&config(&keys)).is_err(), "{keys:?}");
        }
        assert!(
            KeybindingEngine::new(&config(&vec!["F6"; MAX_BINDINGS + 1]))
                .unwrap_err()
                .contains("at most")
        );
    }

    #[test]
    fn recovery_and_printable_keys_are_protected() {
        for key in [
            "Ctrl+C",
            "Ctrl+Shift+C",
            "Alt+Ctrl+C",
            "Esc",
            "Alt+Esc",
            "Shift+Esc",
            "Ctrl+Esc",
            "a",
            "A",
            "Shift+a",
            "Space",
            "界",
        ] {
            assert!(KeybindingEngine::new(&config(&[key])).is_err(), "{key}");
        }
        for key in ["Ctrl+A", "Alt+A", "F6", "Numpad8", "Shift+Left"] {
            assert!(KeybindingEngine::new(&config(&[key])).is_ok(), "{key}");
        }
    }

    #[test]
    fn keypad_identity_precedes_generic_fallback() {
        let engine = KeybindingEngine::new(&KeybindingConfig {
            bindings: vec![
                KeybindingRule {
                    key: "Numpad8".into(),
                    action: ShortcutAction::ScrollLineUp,
                },
                KeybindingRule {
                    key: "Up".into(),
                    action: ShortcutAction::HistoryPrevious,
                },
            ],
        })
        .unwrap();
        let normal = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let keypad = KeyEvent {
            state: KeyEventState::KEYPAD,
            ..normal
        };
        assert_eq!(engine.lookup(normal), Some(ShortcutAction::HistoryPrevious));
        assert_eq!(engine.lookup(keypad), Some(ShortcutAction::ScrollLineUp));
        assert_eq!(
            engine.lookup(KeyEvent {
                code: KeyCode::Char('8'),
                ..keypad
            }),
            Some(ShortcutAction::ScrollLineUp)
        );
        let fallback = KeybindingEngine::new(&config(&["Up"])).unwrap();
        assert_eq!(fallback.lookup(keypad), Some(ShortcutAction::None));
    }

    #[test]
    fn config_roundtrip_and_macro_conflicts() {
        let mut app: AppConfig =
            toml::from_str("[[keybindings.bindings]]\nkey = 'F6'\naction = 'reload'").unwrap();
        app.validate().unwrap();
        let roundtrip: AppConfig = toml::from_str(&toml::to_string(&app).unwrap()).unwrap();
        assert_eq!(app.keybindings, roundtrip.keybindings);
        app.macros.rules.push(MacroRule {
            key: "F6".into(),
            command: "look".into(),
            ..MacroRule::default()
        });
        assert!(
            app.validate()
                .unwrap_err()
                .to_string()
                .contains("configured shortcut")
        );
        app.macros.rules[0].enabled = false;
        app.validate().unwrap();
        app.macros.rules[0].enabled = true;
        app.macros.rules[0].override_builtin = true;
        app.validate().unwrap();
    }

    #[test]
    fn unknown_configuration_is_rejected() {
        for raw in [
            "[keybindings]\nunknown = true",
            "[[keybindings.bindings]]\nkey = 'F6'\naction = 'typo'",
            "[[keybindings.bindings]]\nkey = 'F6'\naction = 'none'\nunknown = true",
            "[[keybindings.bindings]]\nkey = 'F6'",
        ] {
            assert!(toml::from_str::<AppConfig>(raw).is_err(), "{raw}");
        }
    }
}
