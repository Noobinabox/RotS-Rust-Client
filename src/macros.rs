//! Compiled key bindings; execution stays in the application's command pipeline.
use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MacroConfig {
    pub rules: Vec<MacroRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MacroRule {
    pub key: String,
    pub command: String,
    pub enabled: bool,
    pub override_builtin: bool,
    pub allow_repeat: bool,
}

impl Default for MacroRule {
    fn default() -> Self {
        Self {
            key: String::new(),
            command: String::new(),
            enabled: true,
            override_builtin: false,
            allow_repeat: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct KeyBinding {
    code: KeyCode,
    modifiers: KeyModifiers,
    keypad: bool,
}

impl KeyBinding {
    fn normalized(code: KeyCode, mut modifiers: KeyModifiers) -> Self {
        let code = match code {
            KeyCode::BackTab => {
                modifiers.insert(KeyModifiers::SHIFT);
                KeyCode::Tab
            }
            KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                let c = if modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                modifiers.remove(KeyModifiers::SHIFT);
                KeyCode::Char(c)
            }
            KeyCode::Char(c) if modifiers.contains(KeyModifiers::CONTROL) => {
                KeyCode::Char(c.to_ascii_lowercase())
            }
            code => code,
        };
        Self {
            code,
            modifiers,
            keypad: false,
        }
    }

    fn keypad(code: KeyCode, modifiers: KeyModifiers) -> Self {
        // Num Lock may change the reported symbol, not the physical key.
        let code = match code {
            KeyCode::Insert => KeyCode::Char('0'),
            KeyCode::End => KeyCode::Char('1'),
            KeyCode::Down => KeyCode::Char('2'),
            KeyCode::PageDown => KeyCode::Char('3'),
            KeyCode::Left => KeyCode::Char('4'),
            KeyCode::KeypadBegin => KeyCode::Char('5'),
            KeyCode::Right => KeyCode::Char('6'),
            KeyCode::Home => KeyCode::Char('7'),
            KeyCode::Up => KeyCode::Char('8'),
            KeyCode::PageUp => KeyCode::Char('9'),
            KeyCode::Delete => KeyCode::Char('.'),
            code => code,
        };
        Self {
            code,
            modifiers,
            keypad: true,
        }
    }

    fn parse(input: &str) -> Result<Self, String> {
        let mut rest = input.trim();
        let mut modifiers = KeyModifiers::NONE;
        while let Some((prefix, suffix)) = rest.split_once('+') {
            let modifier = match prefix.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => KeyModifiers::CONTROL,
                "alt" => KeyModifiers::ALT,
                "shift" => KeyModifiers::SHIFT,
                _ => break,
            };
            if modifiers.contains(modifier) {
                return Err(format!("Duplicate modifier in macro key `{input}`"));
            }
            modifiers.insert(modifier);
            rest = suffix;
        }
        let lower = rest.to_ascii_lowercase();
        if let Some(key) = lower.strip_prefix("numpad") {
            let code = match key {
                "0" | "insert" => KeyCode::Char('0'),
                "1" | "end" => KeyCode::Char('1'),
                "2" | "down" => KeyCode::Char('2'),
                "3" | "pagedown" => KeyCode::Char('3'),
                "4" | "left" => KeyCode::Char('4'),
                "5" | "begin" => KeyCode::Char('5'),
                "6" | "right" => KeyCode::Char('6'),
                "7" | "home" => KeyCode::Char('7'),
                "8" | "up" => KeyCode::Char('8'),
                "9" | "pageup" => KeyCode::Char('9'),
                "decimal" | "delete" => KeyCode::Char('.'),
                "add" => KeyCode::Char('+'),
                "subtract" => KeyCode::Char('-'),
                "multiply" => KeyCode::Char('*'),
                "divide" => KeyCode::Char('/'),
                "enter" => KeyCode::Enter,
                "equal" => KeyCode::Char('='),
                "separator" => KeyCode::Char(','),
                _ => return Err(format!("Invalid numpad macro key `{input}`")),
            };
            return Ok(Self::keypad(code, modifiers));
        }
        let code = match lower.as_str() {
            "enter" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "space" => KeyCode::Char(' '),
            "plus" => KeyCode::Char('+'),
            _ if rest.chars().count() == 1 => {
                KeyCode::Char(rest.chars().next().unwrap_or_default())
            }
            _ => {
                let number = lower
                    .strip_prefix('f')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=24).contains(n));
                match number {
                    Some(n) => KeyCode::F(n),
                    None => return Err(format!("Invalid macro key `{input}`")),
                }
            }
        };
        if matches!(code, KeyCode::Char(c) if c.is_control()) {
            return Err(format!("Invalid control character in macro key `{input}`"));
        }
        Ok(Self::normalized(code, modifiers))
    }

    fn printable(self) -> bool {
        !self.keypad
            && matches!(self.code, KeyCode::Char(_))
            && !self
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn protected(self) -> bool {
        self.code == KeyCode::Char('c') && self.modifiers.contains(KeyModifiers::CONTROL)
    }

    fn builtin(self) -> bool {
        if self.keypad {
            return false;
        }
        // These keys are consumed by input::handle_key regardless of modifiers.
        matches!(
            self.code,
            KeyCode::Enter
                | KeyCode::Esc
                | KeyCode::Tab
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::F(2)
        ) || (self.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(self.code, KeyCode::Char('e' | 'l' | 'f' | 'n' | 'p')))
    }
}

#[derive(Default)]
pub struct MacroEngine {
    configured: HashMap<KeyBinding, MacroRule>,
    runtime: HashMap<KeyBinding, MacroRule>,
    pub printable_mode: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MacroAction<'a> {
    Execute(&'a str),
    SuppressRepeat,
}

impl MacroEngine {
    pub fn new(config: &MacroConfig) -> Result<Self, String> {
        let mut engine = Self::default();
        for rule in &config.rules {
            let key = Self::validate(rule)?;
            if engine.configured.insert(key, rule.clone()).is_some() {
                return Err(format!("Duplicate macro key `{}`", rule.key));
            }
        }
        Ok(engine)
    }

    fn validate(rule: &MacroRule) -> Result<KeyBinding, String> {
        let key = KeyBinding::parse(&rule.key)?;
        if key.protected() {
            return Err("Ctrl+C is reserved for clearing input and leaving macro mode".into());
        }
        if key.builtin() && !rule.override_builtin {
            return Err(format!(
                "Macro key `{}` replaces a built-in shortcut; use override_builtin = true or --override",
                rule.key
            ));
        }
        if rule.command.trim().is_empty() {
            return Err("Macro command must not be empty".into());
        }
        Ok(key)
    }

    pub fn with_config(&self, config: &MacroConfig) -> Result<Self, String> {
        let mut engine = Self::new(config)?;
        engine.runtime = self.runtime.clone();
        engine.printable_mode = self.printable_mode;
        Ok(engine)
    }

    pub fn add(&mut self, rule: MacroRule) -> Result<(), String> {
        let key = Self::validate(&rule)?;
        self.runtime.insert(key, rule);
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> Result<bool, String> {
        Ok(self.runtime.remove(&KeyBinding::parse(key)?).is_some())
    }

    pub fn clear(&mut self) -> usize {
        let count = self.runtime.len();
        self.runtime.clear();
        count
    }

    pub fn action(&self, event: KeyEvent, searching: bool) -> Option<MacroAction<'_>> {
        if searching || event.kind == KeyEventKind::Release {
            return None;
        }
        let generic = KeyBinding::normalized(event.code, event.modifiers);
        let dedicated = event
            .state
            .contains(KeyEventState::KEYPAD)
            .then(|| KeyBinding::keypad(event.code, event.modifiers));
        let (key, rule) = dedicated
            .and_then(|key| self.rule(&key).map(|rule| (key, rule)))
            .or_else(|| self.rule(&generic).map(|rule| (generic, rule)))?;
        if key.protected() || (key.printable() && !self.printable_mode) {
            return None;
        }
        if !rule.enabled {
            return None;
        }
        if event.kind == KeyEventKind::Repeat && !rule.allow_repeat {
            return Some(MacroAction::SuppressRepeat);
        }
        Some(MacroAction::Execute(&rule.command))
    }

    fn rule(&self, key: &KeyBinding) -> Option<&MacroRule> {
        self.runtime.get(key).or_else(|| self.configured.get(key))
    }

    pub fn listing(&self) -> String {
        let mut rows = Vec::new();
        for (source, rules) in [("configured", &self.configured), ("runtime", &self.runtime)] {
            for (key, rule) in rules {
                let shadowed = source == "configured" && self.runtime.contains_key(key);
                rows.push(format!(
                    "- `{}` [{source}{}{}] -> `{}` (override={}, repeat={})",
                    rule.key,
                    if shadowed { ", shadowed" } else { "" },
                    if rule.enabled { "" } else { ", disabled" },
                    rule.command,
                    rule.override_builtin,
                    rule.allow_repeat
                ));
            }
        }
        rows.sort();
        format!(
            "# Macros\n\nPrintable-key mode: {} (Ctrl+C leaves this mode).\n{}",
            if self.printable_mode { "on" } else { "off" },
            if rows.is_empty() {
                "No macros loaded.".into()
            } else {
                rows.join("\n")
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(key: &str, command: &str) -> MacroRule {
        MacroRule {
            key: key.into(),
            command: command.into(),
            ..MacroRule::default()
        }
    }

    #[test]
    fn numpad_bindings_are_distinct_and_do_not_require_printable_mode() {
        let mut engine = MacroEngine::default();
        engine.add(rule("Numpad8", "north")).unwrap();
        let ordinary = KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE);
        let mut keypad = ordinary;
        keypad.state = KeyEventState::KEYPAD | KeyEventState::NUM_LOCK;
        assert_eq!(engine.action(ordinary, false), None);
        assert_eq!(
            engine.action(keypad, false),
            Some(MacroAction::Execute("north"))
        );
        keypad.code = KeyCode::Up;
        keypad.state = KeyEventState::KEYPAD;
        assert_eq!(
            engine.action(keypad, false),
            Some(MacroAction::Execute("north"))
        );
        assert_eq!(
            engine.action(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), false),
            None
        );
        assert_eq!(engine.action(keypad, true), None);
        keypad.kind = KeyEventKind::Repeat;
        assert_eq!(
            engine.action(keypad, false),
            Some(MacroAction::SuppressRepeat)
        );
        keypad.kind = KeyEventKind::Release;
        assert_eq!(engine.action(keypad, false), None);
    }

    #[test]
    fn numpad_aliases_normalize_and_validate_without_builtin_overrides() {
        for (alias, canonical) in [
            ("Insert", "0"),
            ("End", "1"),
            ("Down", "2"),
            ("PageDown", "3"),
            ("Left", "4"),
            ("Begin", "5"),
            ("Right", "6"),
            ("Home", "7"),
            ("Up", "8"),
            ("PageUp", "9"),
            ("Delete", "Decimal"),
        ] {
            assert_eq!(
                KeyBinding::parse(&format!("Numpad{alias}")),
                KeyBinding::parse(&format!("Numpad{canonical}"))
            );
        }
        for name in [
            "NumpadEnter",
            "NumpadDecimal",
            "NumpadAdd",
            "NumpadSubtract",
            "NumpadMultiply",
            "NumpadDivide",
            "NumpadEqual",
            "NumpadSeparator",
        ] {
            let mut engine = MacroEngine::default();
            engine.add(rule(name, "look")).unwrap();
            let binding = KeyBinding::parse(name).unwrap();
            let mut key = KeyEvent::new(binding.code, KeyModifiers::NONE);
            assert_eq!(engine.action(key, false), None);
            key.state = KeyEventState::KEYPAD;
            assert_eq!(
                engine.action(key, false),
                Some(MacroAction::Execute("look"))
            );
        }
        for name in ["Numpad", "Numpad10", "NumpadF5", "NumpadC"] {
            assert!(KeyBinding::parse(name).is_err());
        }
        assert!(
            MacroEngine::new(&MacroConfig {
                rules: vec![rule("Numpad8", "north"), rule("numpadup", "look")]
            })
            .is_err()
        );
    }

    #[test]
    fn numpad_modifiers_are_exact_and_dedicated_bindings_precede_generic_bindings() {
        let mut engine = MacroEngine::default();
        engine.add(rule("8", "generic")).unwrap();
        engine.printable_mode = true;
        let mut key = KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE);
        key.state = KeyEventState::KEYPAD;
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("generic"))
        );
        engine.add(rule("Numpad8", "dedicated")).unwrap();
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("dedicated"))
        );
        engine.add(rule("Shift+Numpad8", "shifted")).unwrap();
        engine.add(rule("Ctrl+Numpad8", "controlled")).unwrap();
        key.modifiers = KeyModifiers::SHIFT;
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("shifted"))
        );
        key.modifiers = KeyModifiers::CONTROL;
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("controlled"))
        );
        engine.printable_mode = false;
        key.modifiers = KeyModifiers::ALT;
        assert_eq!(engine.action(key, false), None);
    }

    #[test]
    fn numpad_config_and_runtime_bindings_survive_reload_and_removal() {
        let config: MacroConfig =
            toml::from_str("[[rules]]\nkey = 'Numpad8'\ncommand = 'north'\n").unwrap();
        let mut engine = MacroEngine::new(&config).unwrap();
        engine.add(rule("NumpadUp", "look")).unwrap();
        engine = engine.with_config(&config).unwrap();
        assert!(engine.listing().contains("configured, shadowed"));
        let mut key = KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE);
        key.state = KeyEventState::KEYPAD;
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("look"))
        );
        engine.remove("numpad8").unwrap();
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("north"))
        );
    }

    #[test]
    fn macro_keys_normalize_aliases_and_shifted_characters() {
        for (left, right) in [
            ("control+g", "Ctrl+G"),
            ("f05", "F5"),
            ("BackTab", "Shift+Tab"),
            ("Escape", "Esc"),
            ("Shift+a", "A"),
            ("Alt+Shift+a", "Alt+A"),
            ("Alt+Shift+a", "Alt+Shift+A"),
            ("Plus", "+"),
            ("Ctrl+Plus", "Ctrl++"),
        ] {
            assert_eq!(KeyBinding::parse(left), KeyBinding::parse(right));
        }
        assert_ne!(KeyBinding::parse("a"), KeyBinding::parse("A"));
        for key in [
            "",
            "F0",
            "F25",
            "Ctrl+",
            "Ctrl+Ctrl+G",
            "Super+G",
            "hello",
            "\u{7f}",
        ] {
            assert!(KeyBinding::parse(key).is_err(), "{key}");
        }
        assert!(KeyBinding::parse("é").is_ok());
    }

    #[test]
    fn macro_config_rejects_duplicates_conflicts_and_empty_commands() {
        for rules in [
            vec![rule("F5", "")],
            vec![rule("Ctrl+C", "look")],
            vec![rule("Ctrl+Alt+C", "look")],
            vec![rule("F2", "look")],
            vec![rule("F5", "look"), rule("f5", "score")],
            vec![rule("Alt+Shift+a", "look"), rule("Alt+Shift+A", "score")],
        ] {
            assert!(MacroEngine::new(&MacroConfig { rules }).is_err());
        }
        let mut override_rule = rule("F2", "look");
        override_rule.override_builtin = true;
        assert!(
            MacroEngine::new(&MacroConfig {
                rules: vec![override_rule]
            })
            .is_ok()
        );
    }

    #[test]
    fn macro_dispatch_respects_search_mode_repeat_release_and_modifiers() {
        let mut engine = MacroEngine::default();
        engine.add(rule("n", "north")).unwrap();
        let n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(engine.action(n, false), None);
        engine.printable_mode = true;
        assert_eq!(engine.action(n, false), Some(MacroAction::Execute("north")));
        assert_eq!(engine.action(n, true), None);
        assert_eq!(
            engine.action(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT), false),
            None
        );
        let mut repeat = n;
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(
            engine.action(repeat, false),
            Some(MacroAction::SuppressRepeat)
        );
        let mut repeated_rule = rule("n", "north");
        repeated_rule.allow_repeat = true;
        engine.add(repeated_rule).unwrap();
        assert_eq!(
            engine.action(repeat, false),
            Some(MacroAction::Execute("north"))
        );
        repeat.kind = KeyEventKind::Release;
        assert_eq!(engine.action(repeat, false), None);
    }

    #[test]
    fn macro_shifted_alt_chord_matches_terminal_uppercase_with_or_without_shift() {
        let mut engine = MacroEngine::default();
        engine.add(rule("Alt+Shift+a", "look")).unwrap();
        for modifiers in [KeyModifiers::ALT, KeyModifiers::ALT | KeyModifiers::SHIFT] {
            assert_eq!(
                engine.action(KeyEvent::new(KeyCode::Char('A'), modifiers), false),
                Some(MacroAction::Execute("look"))
            );
        }
        assert_eq!(
            engine.action(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT), false),
            None
        );
    }

    #[test]
    fn macro_runtime_override_remove_and_reload_preserve_configuration() {
        let config = MacroConfig {
            rules: vec![rule("F5", "look")],
        };
        let mut engine = MacroEngine::new(&config).unwrap();
        let key = KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE);
        engine.add(rule("f5", "score")).unwrap();
        engine.printable_mode = true;
        engine = engine.with_config(&config).unwrap();
        assert!(engine.printable_mode);
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("score"))
        );
        assert!(engine.listing().contains("configured, shadowed"));
        assert!(engine.remove("F5").unwrap());
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("look"))
        );
        assert!(!engine.remove("F5").unwrap());
        engine.add(rule("F6", "rest")).unwrap();
        assert_eq!(engine.clear(), 1);
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("look"))
        );
        assert!(engine.add(rule("F5", " ")).is_err());
        assert_eq!(
            engine.action(key, false),
            Some(MacroAction::Execute("look"))
        );
    }

    #[test]
    fn macro_config_toml_roundtrip_and_disabled_binding() {
        let config: crate::config::AppConfig =
            toml::from_str("[[macros.rules]]\nkey = 'F5'\ncommand = 'look'\nenabled = false\n")
                .unwrap();
        config.validate().unwrap();
        let engine = MacroEngine::new(&config.macros).unwrap();
        assert_eq!(
            engine.action(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE), false),
            None
        );
        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(config, toml::from_str(&serialized).unwrap());
        assert!(
            toml::from_str::<crate::config::AppConfig>(
                "[[macros.rules]]\nkey = 'F5'\ncommand = 'look'\nallow_repat = true\n"
            )
            .is_err()
        );
    }
}
