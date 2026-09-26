//! Display-only substitutions. Original server text must remain available to automation.

use std::ops::Range;

use regex::Regex;

use crate::{
    color::{AnsiColor, AnsiColors, parse_ansi_color, parse_sgr_parameters},
    config::{MatchType, SubstitutionConfig, SubstitutionRuleConfig},
    error::{MudClientError, Result},
    state::OutputCategory,
};

use super::{
    ansi::AnsiStyleState, highlights::category_allowed, templates::validate_capture_references,
    variables::VariableStore,
};

pub const MAX_SUBSTITUTION_BYTES: usize = 65_536;

#[derive(Debug, Clone)]
pub struct SubstitutionEngine {
    enabled: bool,
    rules: Vec<CompiledSubstitution>,
}

#[derive(Debug, Clone)]
struct CompiledSubstitution {
    config: SubstitutionRuleConfig,
    regex: Regex,
    foreground: Option<AnsiColor>,
    background: Option<AnsiColor>,
    runtime: bool,
}

impl SubstitutionEngine {
    pub fn new(config: &SubstitutionConfig) -> Result<Self> {
        let mut engine = Self {
            enabled: config.enabled,
            rules: config
                .rules
                .iter()
                .cloned()
                .map(|rule| CompiledSubstitution::new(rule, false))
                .collect::<Result<_>>()?,
        };
        engine.sort_rules();
        Ok(engine)
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
        }
    }

    pub fn add_runtime_substitution(&mut self, mut config: SubstitutionRuleConfig) -> Result<()> {
        config.name = format!("runtime:{}", config.pattern);
        config.enabled = true;
        config.priority = 10_000;
        let compiled = CompiledSubstitution::new(config, true)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != compiled.config.pattern);
        self.rules.push(compiled);
        self.sort_rules();
        Ok(())
    }

    pub fn remove_runtime_substitution(&mut self, pattern: &str) -> bool {
        let before = self.rules.len();
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != pattern);
        before != self.rules.len()
    }

    pub fn clear_runtime_substitutions(&mut self) -> usize {
        let before = self.rules.len();
        self.rules.retain(|rule| !rule.runtime);
        before - self.rules.len()
    }

    pub fn substitution_entries(&self) -> Vec<(&SubstitutionRuleConfig, bool)> {
        self.rules
            .iter()
            .map(|rule| (&rule.config, rule.runtime))
            .collect()
    }

    pub fn runtime_configs(&self) -> Vec<SubstitutionRuleConfig> {
        self.rules
            .iter()
            .filter(|rule| rule.runtime)
            .map(|rule| rule.config.clone())
            .collect()
    }

    /// Apply each rule once, replacing every non-overlapping match. Later rules
    /// see earlier replacements, but category/color filters always use the
    /// original server metadata. Patterns are literal configuration, while
    /// replacement variables are expanded before capture interpolation.
    ///
    /// Prepend the original starting SGR state when a line inherits ANSI from
    /// earlier server output. This lets explicit replacement colors be scoped
    /// without changing the suffix. On error, the caller must display `raw`.
    pub fn apply(
        &self,
        raw: &str,
        category: OutputCategory,
        colors: &AnsiColors,
        variables: &VariableStore,
    ) -> Result<String> {
        if !self.rules.iter().any(|rule| self.enabled || rule.runtime) {
            return Ok(raw.to_owned());
        }
        check_size(raw.len())?;
        let mut result = raw.to_owned();
        for rule in &self.rules {
            if (!self.enabled && !rule.runtime)
                || !rule.config.enabled
                || !category_allowed(&rule.config.categories, category.clone())
                || rule
                    .foreground
                    .is_some_and(|value| !colors.foregrounds.contains(&value))
                || rule
                    .background
                    .is_some_and(|value| !colors.backgrounds.contains(&value))
            {
                continue;
            }
            result = rule.apply(&result, variables)?;
        }
        Ok(result)
    }

    fn sort_rules(&mut self) {
        self.rules.sort_by(|left, right| {
            right
                .config
                .priority
                .cmp(&left.config.priority)
                .then_with(|| left.config.name.cmp(&right.config.name))
        });
    }
}

impl CompiledSubstitution {
    fn new(config: SubstitutionRuleConfig, runtime: bool) -> Result<Self> {
        if config.name.trim().is_empty() || config.pattern.is_empty() {
            return Err(invalid("substitution name and pattern must not be empty"));
        }
        check_size(config.pattern.len())?;
        check_size(config.replacement.len())?;
        validate_replacement(&config.replacement)?;
        let pattern = match config.match_type {
            MatchType::Plain => regex::escape(&config.pattern),
            MatchType::Regex => config.pattern.clone(),
        };
        let regex = Regex::new(&pattern).map_err(|error| {
            invalid(format!(
                "substitution `{}` has invalid regex: {error}",
                config.name
            ))
        })?;
        validate_capture_references(
            &config.replacement,
            regex.captures_len() - 1,
            &format!("substitution `{}`", config.name),
        )?;
        let color = |value: &Option<String>| {
            value
                .as_deref()
                .map(|value| {
                    parse_ansi_color(value)
                        .ok_or_else(|| invalid(format!("invalid substitution color `{value}`")))
                })
                .transpose()
        };
        Ok(Self {
            foreground: color(&config.foreground)?,
            background: color(&config.background)?,
            config,
            regex,
            runtime,
        })
    }

    fn apply(&self, raw: &str, variables: &VariableStore) -> Result<String> {
        let projected = ProjectedText::new(raw);
        if !self.regex.is_match(&projected.plain) {
            return Ok(raw.to_owned());
        }
        let template = variables.expand(&self.config.replacement)?;
        validate_replacement(&template)?;
        validate_capture_references(
            &template,
            self.regex.captures_len() - 1,
            &format!("substitution `{}`", self.config.name),
        )?;
        let mut output = String::new();
        let mut copied_until = 0;
        let mut controls = projected.controls.iter().peekable();
        let mut style = AnsiStyleState::default();
        for captures in self.regex.captures_iter(&projected.plain) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            let range = projected.raw_range(matched.range(), raw.len());
            while controls
                .peek()
                .is_some_and(|control| control.end <= range.start)
            {
                if let Some(control) = controls.next() {
                    style.consume(&raw[control.clone()]);
                }
            }
            append(&mut output, &raw[copied_until..range.start])?;
            let replacement = expand_captures(&template, &captures)?;
            validate_replacement(&replacement)?;
            append(&mut output, &replacement)?;
            if replacement.contains('\x1b') {
                // Restore original style, not the replacement's final style.
                append(&mut output, &style.prefix())?;
            }
            // Retain transitions inside removed text so the original suffix
            // and the next server line inherit the same ANSI state.
            while controls
                .peek()
                .is_some_and(|control| control.end <= range.end)
            {
                if let Some(control) = controls.next() {
                    append(&mut output, &raw[control.clone()])?;
                    style.consume(&raw[control.clone()]);
                }
            }
            copied_until = range.end;
        }
        append(&mut output, &raw[copied_until..])?;
        Ok(output)
    }
}

/// Map UTF-8 text bytes back to raw glyph boundaries, excluding CSI controls.
/// This follows the client's existing ANSI-stripped text matching semantics.
struct ProjectedText {
    plain: String,
    starts: Vec<usize>,
    ends: Vec<usize>,
    controls: Vec<Range<usize>>,
}

impl ProjectedText {
    fn new(raw: &str) -> Self {
        let mut result = Self {
            plain: String::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            controls: Vec::new(),
        };
        let mut chars = raw.char_indices().peekable();
        while let Some((start, character)) = chars.next() {
            if character == '\x1b' && chars.peek().is_some_and(|(_, character)| *character == '[') {
                chars.next();
                let mut end = raw.len();
                for (index, character) in chars.by_ref() {
                    if ('@'..='~').contains(&character) {
                        end = index + character.len_utf8();
                        break;
                    }
                }
                result.controls.push(start..end);
            } else {
                result.plain.push(character);
                for _ in 0..character.len_utf8() {
                    result.starts.push(start);
                    result.ends.push(start + character.len_utf8());
                }
            }
        }
        result
    }

    fn raw_range(&self, matched: Range<usize>, raw_len: usize) -> Range<usize> {
        let start = self.starts.get(matched.start).copied().unwrap_or(raw_len);
        let end = if matched.is_empty() {
            start
        } else {
            self.ends[matched.end - 1]
        };
        start..end
    }
}

fn expand_captures(template: &str, captures: &regex::Captures<'_>) -> Result<String> {
    let mut result = String::new();
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        append(&mut result, &remaining[..start])?;
        remaining = &remaining[start..];
        if remaining.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            && let Some(end) = remaining.find('}')
            && remaining[1..end].bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(index) = remaining[1..end].parse::<usize>()
        {
            append(
                &mut result,
                captures.get(index).map_or("", |capture| capture.as_str()),
            )?;
            remaining = &remaining[end + 1..];
        } else {
            append(&mut result, "{")?;
            remaining = &remaining[1..];
        }
    }
    append(&mut result, remaining)?;
    Ok(result)
}

fn validate_replacement(value: &str) -> Result<()> {
    check_size(value.len())?;
    let mut chars = value.char_indices();
    while let Some((start, character)) = chars.next() {
        if character == '\x1b' {
            if chars.next().map(|(_, character)| character) != Some('[') {
                return Err(invalid(
                    "substitution replacement permits only supported SGR escapes",
                ));
            }
            let mut end = None;
            for (index, character) in chars.by_ref() {
                if ('@'..='~').contains(&character) {
                    end = Some(index + character.len_utf8());
                    break;
                }
            }
            if !end.is_some_and(|end| supported_sgr(&value[start..end])) {
                return Err(invalid(
                    "substitution replacement permits only supported SGR escapes",
                ));
            }
        } else if character.is_control() && character != '\t' {
            return Err(invalid(
                "substitution replacement must be a single line without control characters",
            ));
        }
    }
    Ok(())
}

fn is_sgr(value: &str) -> bool {
    value.starts_with("\x1b[") && value.ends_with('m')
}

fn supported_sgr(value: &str) -> bool {
    if !is_sgr(value) {
        return false;
    }
    let Some(values) = parse_sgr_parameters(&value[2..value.len() - 1]) else {
        return false;
    };
    let mut index = 0;
    while index < values.len() {
        match values[index] {
            0..=4 | 22..=24 | 30..=37 | 39..=47 | 49 | 90..=97 | 100..=107 => index += 1,
            38 | 48 => {
                let components = match values.get(index + 1) {
                    Some(5) => 1,
                    Some(2) => 3,
                    _ => return false,
                };
                if values
                    .get(index + 2..index + 2 + components)
                    .is_none_or(|values| values.iter().any(|value| *value > 255))
                {
                    return false;
                }
                index += 2 + components;
            }
            _ => return false,
        }
    }
    true
}

fn append(output: &mut String, value: &str) -> Result<()> {
    check_size(output.len().saturating_add(value.len()))?;
    output.push_str(value);
    Ok(())
}

fn check_size(size: usize) -> Result<()> {
    if size > MAX_SUBSTITUTION_BYTES {
        Err(invalid("substitution exceeds the 64 KiB display limit"))
    } else {
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> MudClientError {
    MudClientError::ConfigValidation(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{OutputCategoryConfig, VariableConfig},
        state::plain_text,
    };

    fn rule(pattern: &str, replacement: &str) -> SubstitutionRuleConfig {
        SubstitutionRuleConfig {
            name: pattern.into(),
            pattern: pattern.into(),
            replacement: replacement.into(),
            ..SubstitutionRuleConfig::default()
        }
    }

    fn engine(rules: Vec<SubstitutionRuleConfig>) -> SubstitutionEngine {
        SubstitutionEngine::new(&SubstitutionConfig {
            rules,
            ..SubstitutionConfig::default()
        })
        .unwrap()
    }

    fn apply(engine: &SubstitutionEngine, raw: &str) -> Result<String> {
        engine.apply(
            raw,
            OutputCategory::Normal,
            &AnsiColors::default(),
            &VariableStore::empty(),
        )
    }

    #[test]
    fn replaces_all_non_overlapping_plain_matches_including_unicode() {
        let substitutions = engine(vec![rule("猫", "{0}!")]);
        assert_eq!(apply(&substitutions, "猫 猫").unwrap(), "猫! 猫!");
        assert_eq!(apply(&substitutions, "unchanged").unwrap(), "unchanged");
        assert_eq!(apply(&substitutions, "").unwrap(), "");
        assert_eq!(apply(&engine(vec![rule("aa", "b")]), "aaa").unwrap(), "ba");
    }

    #[test]
    fn replaces_ansi_split_matches_without_losing_original_transitions() {
        let engine = engine(vec![rule("orc", "goblin")]);
        assert_eq!(
            apply(&engine, "\x1b[31mor\x1b[32mc tail\x1b[0m").unwrap(),
            "\x1b[31mgoblin\x1b[32m tail\x1b[0m"
        );
    }

    #[test]
    fn explicit_replacement_colors_restore_inherited_style_and_suffix() {
        let engine = engine(vec![rule("orc", "\x1b[34melf")]);
        let output = apply(&engine, "\x1b[0;1;31;48;5;17morc tail").unwrap();
        assert_eq!(
            output,
            "\x1b[0;1;31;48;5;17m\x1b[34melf\x1b[0;1;31;48;5;17m tail"
        );
        let mut original = AnsiStyleState::default();
        original.consume("\x1b[0;1;31;48;5;17morc tail");
        let mut rendered = AnsiStyleState::default();
        rendered.consume(&output);
        assert_eq!(original.prefix(), rendered.prefix());
    }

    #[test]
    fn regex_captures_and_variables_are_expanded_without_interpreting_capture_data() {
        let mut capture_rule = rule("(orc)(?: (captain))?", "${label}: {0} [{1}/{2}]");
        capture_rule.match_type = MatchType::Regex;
        let substitutions = engine(vec![capture_rule]);
        let variables = VariableStore::new(&VariableConfig {
            values: [("label".into(), "Target".into())].into(),
            ..VariableConfig::default()
        })
        .unwrap();
        assert_eq!(
            substitutions
                .apply(
                    "orc",
                    OutputCategory::Normal,
                    &AnsiColors::default(),
                    &variables
                )
                .unwrap(),
            "Target: orc [orc/]"
        );
        assert!(apply(&substitutions, "orc").is_err());
        assert_eq!(
            apply(&engine(vec![rule("${missing}", "{0}")]), "${missing}").unwrap(),
            "${missing}"
        );
    }

    #[test]
    fn rules_run_once_in_priority_then_name_order() {
        let mut first = rule("a", "b");
        first.priority = 10;
        let mut second = rule("b", "c");
        second.name = "second".into();
        let mut third = rule("c", "a");
        third.name = "third".into();
        assert_eq!(
            apply(&engine(vec![third, second, first]), "a").unwrap(),
            "a"
        );
    }

    #[test]
    fn category_and_original_color_filters_limit_matching() {
        let mut filtered = rule("orc", "elf");
        filtered.categories = vec![OutputCategoryConfig::Prompt];
        filtered.foreground = Some("red".into());
        filtered.background = Some("index:17".into());
        let engine = engine(vec![filtered]);
        let colors = AnsiColors {
            foregrounds: vec![AnsiColor::Red],
            backgrounds: vec![AnsiColor::Indexed(17)],
        };
        assert_eq!(
            engine
                .apply(
                    "orc",
                    OutputCategory::Prompt,
                    &colors,
                    &VariableStore::empty()
                )
                .unwrap(),
            "elf"
        );
        assert_eq!(
            engine
                .apply(
                    "orc",
                    OutputCategory::Normal,
                    &colors,
                    &VariableStore::empty()
                )
                .unwrap(),
            "orc"
        );
        assert_eq!(
            engine
                .apply(
                    "orc",
                    OutputCategory::Prompt,
                    &AnsiColors::default(),
                    &VariableStore::empty()
                )
                .unwrap(),
            "orc"
        );
    }

    #[test]
    fn runtime_replacement_removal_and_disabled_configuration_are_consistent() {
        let mut engine = SubstitutionEngine::new(&SubstitutionConfig {
            enabled: false,
            rules: vec![rule("orc", "configured")],
        })
        .unwrap();
        assert_eq!(apply(&engine, "orc").unwrap(), "orc");
        engine
            .add_runtime_substitution(rule("orc", "first"))
            .unwrap();
        engine
            .add_runtime_substitution(rule("orc", "second"))
            .unwrap();
        assert_eq!(engine.runtime_configs().len(), 1);
        assert_eq!(engine.substitution_entries().len(), 2);
        assert_eq!(apply(&engine, "orc").unwrap(), "second");
        assert!(engine.remove_runtime_substitution("orc"));
        assert!(!engine.remove_runtime_substitution("orc"));
        engine
            .add_runtime_substitution(rule("orc", "third"))
            .unwrap();
        assert_eq!(engine.clear_runtime_substitutions(), 1);
        assert_eq!(apply(&engine, "orc").unwrap(), "orc");
    }

    #[test]
    fn zero_width_matches_and_empty_replacements_terminate() {
        let mut zero = rule("^|$", "!");
        zero.match_type = MatchType::Regex;
        assert_eq!(apply(&engine(vec![zero]), "猫").unwrap(), "!猫!");
        assert_eq!(
            apply(&engine(vec![rule("orc", "")]), "orc orc").unwrap(),
            " "
        );
    }

    #[test]
    fn invalid_regex_captures_colors_and_control_sequences_are_rejected() {
        let mut invalid_regex = rule("[", "x");
        invalid_regex.match_type = MatchType::Regex;
        let mut invalid_color = rule("orc", "x");
        invalid_color.foreground = Some("unobtanium".into());
        for invalid_rule in [
            invalid_regex,
            invalid_color,
            rule("", "x"),
            rule("orc", "{1}"),
            rule("orc", "{0"),
            rule("orc", "\x1b[2J"),
            rule("orc", "\x1b]52;secret\x07"),
            rule("orc", "line\nline"),
            rule("orc", "\x1b[38;5;999m"),
        ] {
            assert!(
                SubstitutionEngine::new(&SubstitutionConfig {
                    rules: vec![invalid_rule],
                    ..SubstitutionConfig::default()
                })
                .is_err()
            );
        }
    }

    #[test]
    fn output_growth_is_bounded_before_large_capture_expansion() {
        let raw = "x".repeat(MAX_SUBSTITUTION_BYTES);
        assert_eq!(
            apply(&engine(vec![rule("x", "x")]), &raw).unwrap().len(),
            MAX_SUBSTITUTION_BYTES
        );
        assert!(apply(&engine(vec![rule("x", "xx")]), &raw).is_err());
        let mut large = rule("(.*)", "{1}{1}");
        large.match_type = MatchType::Regex;
        assert!(apply(&engine(vec![large]), &raw).is_err());
        assert!(apply(&engine(vec![rule("x", "y")]), &(raw + "x")).is_err());
    }

    #[test]
    fn supported_sgr_variants_are_kept_and_do_not_change_plain_text() {
        let engine = engine(vec![rule(
            "orc",
            "\x1b[1;3;38;2;12;34;56;48;5;17melf\x1b[m",
        )]);
        assert_eq!(plain_text(&apply(&engine, "orc!").unwrap()), "elf!");
    }

    #[test]
    fn replacement_variable_changes_apply_without_recompiling_and_reject_controls() {
        let substitutions = engine(vec![rule("orc", "${label}")]);
        let variables = VariableStore::empty()
            .with_runtime("label", "goblin")
            .unwrap();
        assert_eq!(
            substitutions
                .apply(
                    "orc",
                    OutputCategory::Normal,
                    &AnsiColors::default(),
                    &variables
                )
                .unwrap(),
            "goblin"
        );
        let variables = variables.with_runtime("label", "elf").unwrap();
        assert_eq!(
            substitutions
                .apply(
                    "orc",
                    OutputCategory::Normal,
                    &AnsiColors::default(),
                    &variables
                )
                .unwrap(),
            "elf"
        );
        let variables = variables.with_runtime("label", "\x1b[2J").unwrap();
        assert!(
            substitutions
                .apply(
                    "orc",
                    OutputCategory::Normal,
                    &AnsiColors::default(),
                    &variables
                )
                .is_err()
        );
    }

    #[test]
    fn configuration_toml_loads_and_validates_rules() {
        let valid: crate::config::AppConfig = toml::from_str(
            r#"
            [substitutions]
            enabled = true
            [[substitutions.rules]]
            name = "rename"
            match_type = "regex"
            pattern = '(orc)'
            replacement = '{1}!'
            foreground = 'lightcyan'
            categories = ['normal', 'prompt']
        "#,
        )
        .unwrap();
        valid.validate().unwrap();
        let mut invalid = valid;
        invalid.substitutions.rules[0].pattern = "[".into();
        assert!(invalid.validate().is_err());
    }
}
