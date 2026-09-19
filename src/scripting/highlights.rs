use regex::Regex;

use crate::{
    color::parse_color,
    config::{HighlightConfig, HighlightRuleConfig, MatchType, OutputCategoryConfig},
    error::{MudClientError, Result},
    state::{OutputCategory, OutputStyle},
};

#[derive(Debug, Clone)]
pub struct HighlightEngine {
    enabled: bool,
    rules: Vec<CompiledHighlight>,
}

#[derive(Debug, Clone)]
struct CompiledHighlight {
    config: HighlightRuleConfig,
    regex: Option<Regex>,
    runtime: bool,
}

impl HighlightEngine {
    pub fn new(config: &HighlightConfig) -> Result<Self> {
        let mut rules = config
            .rules
            .iter()
            .cloned()
            .map(|rule| CompiledHighlight::new(rule, false))
            .collect::<Result<Vec<_>>>()?;
        rules.sort_by(|left, right| {
            right
                .config
                .priority
                .cmp(&left.config.priority)
                .then_with(|| left.config.name.cmp(&right.config.name))
        });
        Ok(Self {
            enabled: config.enabled,
            rules,
        })
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
        }
    }

    pub fn style_for(&self, line: &str, category: OutputCategory) -> Option<OutputStyle> {
        self.rules
            .iter()
            .filter(|rule| self.enabled || rule.runtime)
            .find(|rule| rule.matches(line, category.clone()))
            .map(|rule| OutputStyle {
                foreground: rule.config.foreground.clone(),
                background: rule.config.background.clone(),
                bold: rule.config.bold,
                dim: rule.config.dim,
                italic: rule.config.italic,
                underline: rule.config.underline,
                reverse: rule.config.reverse,
            })
    }

    pub fn add_runtime_highlight(&mut self, mut config: HighlightRuleConfig) -> Result<()> {
        config.name = format!("runtime:{}", config.pattern.trim());
        config.enabled = true;
        config.priority = 10_000;
        let compiled = CompiledHighlight::new(config, true)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != compiled.config.pattern);
        self.rules.push(compiled);
        self.sort_rules();
        Ok(())
    }

    pub fn remove_runtime_highlight(&mut self, pattern: &str) -> bool {
        let pattern = pattern.trim();
        let before = self.rules.len();
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != pattern);
        before != self.rules.len()
    }

    pub fn clear_runtime_highlights(&mut self) -> usize {
        let before = self.rules.len();
        self.rules.retain(|rule| !rule.runtime);
        before - self.rules.len()
    }

    pub fn highlight_entries(&self) -> Vec<(&HighlightRuleConfig, bool)> {
        self.rules
            .iter()
            .map(|rule| (&rule.config, rule.runtime))
            .collect()
    }

    pub fn runtime_configs(&self) -> Vec<HighlightRuleConfig> {
        self.rules
            .iter()
            .filter(|rule| rule.runtime)
            .map(|rule| rule.config.clone())
            .collect()
    }

    fn sort_rules(&mut self) {
        self.rules.sort_by(|left, right| {
            right
                .runtime
                .cmp(&left.runtime)
                .then_with(|| right.config.priority.cmp(&left.config.priority))
                .then_with(|| left.config.name.cmp(&right.config.name))
        });
    }
}

impl CompiledHighlight {
    fn new(config: HighlightRuleConfig, runtime: bool) -> Result<Self> {
        if config.pattern.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(
                "highlight pattern must not be empty".to_string(),
            ));
        }
        for (field, value) in [
            ("foreground", config.foreground.as_deref()),
            ("background", config.background.as_deref()),
        ] {
            if let Some(value) = value
                && parse_color(value).is_none()
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "highlight `{}` has invalid {field} color `{value}`",
                    config.name
                )));
            }
        }
        if config.foreground.is_none()
            && config.background.is_none()
            && !config.bold
            && !config.dim
            && !config.italic
            && !config.underline
            && !config.reverse
        {
            return Err(MudClientError::ConfigValidation(format!(
                "highlight `{}` must define at least one color or style",
                config.name
            )));
        }
        let regex = if config.match_type == MatchType::Regex {
            Some(Regex::new(&config.pattern).map_err(|error| {
                MudClientError::ConfigValidation(format!(
                    "highlight `{}` has invalid regex pattern: {error}",
                    config.name
                ))
            })?)
        } else {
            None
        };
        Ok(Self {
            config,
            regex,
            runtime,
        })
    }

    fn matches(&self, line: &str, category: OutputCategory) -> bool {
        self.config.enabled
            && category_allowed(&self.config.categories, category)
            && match self.config.match_type {
                MatchType::Plain => line.contains(&self.config.pattern),
                MatchType::Regex => self
                    .regex
                    .as_ref()
                    .is_some_and(|regex| regex.is_match(line)),
            }
    }
}

pub(crate) fn category_allowed(allowed: &[OutputCategoryConfig], category: OutputCategory) -> bool {
    allowed.is_empty()
        || allowed.iter().any(|allowed| {
            matches!(
                (allowed, &category),
                (OutputCategoryConfig::Normal, OutputCategory::Normal)
                    | (OutputCategoryConfig::Combat, OutputCategory::Combat)
                    | (
                        OutputCategoryConfig::Communication,
                        OutputCategory::Communication
                    )
                    | (OutputCategoryConfig::System, OutputCategory::System)
                    | (OutputCategoryConfig::Error, OutputCategory::Error)
                    | (OutputCategoryConfig::Prompt, OutputCategory::Prompt)
                    | (OutputCategoryConfig::Triggered, OutputCategory::Triggered)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_highest_priority_matching_style() {
        let engine = HighlightEngine::new(&HighlightConfig {
            rules: vec![
                HighlightRuleConfig {
                    name: "low".to_string(),
                    pattern: "orc".to_string(),
                    foreground: Some("red".to_string()),
                    ..HighlightRuleConfig::default()
                },
                HighlightRuleConfig {
                    name: "high".to_string(),
                    priority: 10,
                    pattern: "orc".to_string(),
                    foreground: Some("#7aa2f7".to_string()),
                    bold: true,
                    ..HighlightRuleConfig::default()
                },
            ],
            ..HighlightConfig::default()
        })
        .expect("highlight config should compile");

        let style = engine
            .style_for("An orc arrives.", OutputCategory::Normal)
            .expect("style should match");

        assert_eq!(style.foreground.as_deref(), Some("#7aa2f7"));
        assert!(style.bold);
    }

    #[test]
    fn runtime_highlight_overrides_configured_rule_and_is_listed() {
        let mut engine = HighlightEngine::new(&HighlightConfig {
            enabled: true,
            rules: vec![HighlightRuleConfig {
                name: "configured".to_string(),
                priority: i32::MAX,
                pattern: "orc".to_string(),
                foreground: Some("red".to_string()),
                ..HighlightRuleConfig::default()
            }],
        })
        .unwrap();

        engine
            .add_runtime_highlight(HighlightRuleConfig {
                match_type: MatchType::Regex,
                pattern: "^orc$".to_string(),
                foreground: Some("yellow".to_string()),
                underline: true,
                ..HighlightRuleConfig::default()
            })
            .unwrap();

        let style = engine
            .style_for("orc", OutputCategory::Normal)
            .expect("runtime highlight should remain active");
        assert_eq!(style.foreground.as_deref(), Some("yellow"));
        assert!(style.underline);
        assert_eq!(
            engine
                .highlight_entries()
                .iter()
                .filter(|(_, runtime)| *runtime)
                .count(),
            1
        );
    }

    #[test]
    fn runtime_highlight_replaces_same_pattern_and_match_type() {
        let mut engine = HighlightEngine::new(&HighlightConfig::default()).unwrap();
        engine
            .add_runtime_highlight(HighlightRuleConfig {
                pattern: "orc".to_string(),
                foreground: Some("red".to_string()),
                ..HighlightRuleConfig::default()
            })
            .unwrap();
        engine
            .add_runtime_highlight(HighlightRuleConfig {
                match_type: MatchType::Regex,
                pattern: "orc".to_string(),
                foreground: Some("green".to_string()),
                ..HighlightRuleConfig::default()
            })
            .unwrap();

        assert_eq!(engine.runtime_configs().len(), 1);
        assert_eq!(
            engine
                .style_for("orc", OutputCategory::Normal)
                .and_then(|style| style.foreground),
            Some("green".to_string())
        );
    }

    #[test]
    fn rejects_empty_and_invalid_runtime_patterns() {
        let mut engine = HighlightEngine::new(&HighlightConfig::default()).unwrap();

        assert!(
            engine
                .add_runtime_highlight(HighlightRuleConfig {
                    pattern: " ".to_string(),
                    foreground: Some("red".to_string()),
                    ..HighlightRuleConfig::default()
                })
                .is_err()
        );
        assert!(
            engine
                .add_runtime_highlight(HighlightRuleConfig {
                    match_type: MatchType::Regex,
                    pattern: "[".to_string(),
                    foreground: Some("red".to_string()),
                    ..HighlightRuleConfig::default()
                })
                .is_err()
        );
        assert!(
            engine
                .add_runtime_highlight(HighlightRuleConfig {
                    pattern: "orc".to_string(),
                    foreground: Some("ultraviolet".to_string()),
                    ..HighlightRuleConfig::default()
                })
                .is_err()
        );
        assert!(
            engine
                .add_runtime_highlight(HighlightRuleConfig {
                    pattern: "orc".to_string(),
                    ..HighlightRuleConfig::default()
                })
                .is_err()
        );
    }
}
