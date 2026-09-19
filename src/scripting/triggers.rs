use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use regex::Regex;

use crate::{
    color::{AnsiColor, AnsiColors, parse_ansi_color},
    config::{MatchType, TriggerConfig, TriggerRuleConfig},
    error::{MudClientError, Result},
    scripting::highlights::category_allowed,
    scripting::templates::{
        has_capture_reference, substitute_captures, validate_capture_references,
    },
    scripting::variables::VariableStore,
    state::{OutputCategory, ScriptEventRecord},
};

#[derive(Debug, Clone)]
pub struct TriggerEngine {
    enabled: bool,
    max_commands_per_line: usize,
    rules: Vec<CompiledTrigger>,
    fired_once: HashSet<String>,
    variables: VariableStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerActions {
    pub commands: Vec<String>,
    pub events: Vec<ScriptEventRecord>,
    pub lua: Vec<LuaTriggerHook>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTriggerHook {
    pub function: String,
    pub line: String,
    pub category: OutputCategory,
    pub captures: Vec<String>,
    pub colors: AnsiColors,
}

#[derive(Debug, Clone)]
struct CompiledTrigger {
    config: TriggerRuleConfig,
    pattern: String,
    commands: Vec<String>,
    event: Option<String>,
    regex: Option<Regex>,
    foreground: Option<AnsiColor>,
    background: Option<AnsiColor>,
    last_match: Option<Instant>,
    runtime: bool,
}

impl TriggerEngine {
    pub fn new(config: &TriggerConfig) -> Result<Self> {
        Self::new_with_variables(config, &VariableStore::empty())
    }

    pub fn new_with_variables(config: &TriggerConfig, variables: &VariableStore) -> Result<Self> {
        let rules = config
            .rules
            .iter()
            .cloned()
            .map(|rule| CompiledTrigger::new(rule, variables))
            .collect::<Result<Vec<_>>>()?;
        let mut engine = Self {
            enabled: config.enabled,
            max_commands_per_line: config.max_commands_per_line,
            rules,
            fired_once: HashSet::new(),
            variables: variables.clone(),
        };
        engine.sort_rules();
        Ok(engine)
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_commands_per_line: TriggerConfig::default().max_commands_per_line,
            rules: Vec::new(),
            fired_once: HashSet::new(),
            variables: VariableStore::empty(),
        }
    }

    pub fn set_variables(&mut self, variables: &VariableStore) -> Result<()> {
        let rules = self
            .rules
            .iter()
            .map(|rule| {
                let mut compiled =
                    CompiledTrigger::compile(rule.config.clone(), rule.runtime, variables)?;
                compiled.last_match = rule.last_match;
                Ok(compiled)
            })
            .collect::<Result<Vec<_>>>()?;
        self.rules = rules;
        self.variables = variables.clone();
        self.sort_rules();
        Ok(())
    }

    pub fn evaluate(&mut self, line: &str, category: OutputCategory) -> TriggerActions {
        self.evaluate_with_colors(line, category, &AnsiColors::default())
    }

    pub fn evaluate_with_colors(
        &mut self,
        line: &str,
        category: OutputCategory,
        colors: &AnsiColors,
    ) -> TriggerActions {
        let now = Instant::now();
        let mut commands = Vec::new();
        let mut events = Vec::new();
        let mut lua = Vec::new();
        for rule in &mut self.rules {
            let action_count = commands
                .len()
                .saturating_add(events.len())
                .saturating_add(lua.len());
            if action_count >= self.max_commands_per_line {
                break;
            }
            if !self.enabled && !rule.runtime {
                continue;
            }
            let rule_key = rule.key();
            if self.fired_once.contains(&rule_key) {
                continue;
            }
            let Some(captures) = rule.captures(line, category.clone(), colors, now) else {
                continue;
            };
            rule.last_match = Some(now);
            if rule.config.one_shot {
                self.fired_once.insert(rule_key);
            }
            let remaining = self.max_commands_per_line.saturating_sub(
                commands
                    .len()
                    .saturating_add(events.len())
                    .saturating_add(lua.len()),
            );
            commands.extend(
                rule.commands
                    .iter()
                    .take(remaining)
                    .map(|template| substitute_captures(template, &captures)),
            );
            if commands.len().saturating_add(events.len()) < self.max_commands_per_line
                && let Some(event) = &rule.event
            {
                events.push(ScriptEventRecord {
                    name: substitute_captures(event, &captures),
                    source: line.to_string(),
                });
            }
            if commands
                .len()
                .saturating_add(events.len())
                .saturating_add(lua.len())
                < self.max_commands_per_line
                && let Some(function) = &rule.config.lua
            {
                lua.push(LuaTriggerHook {
                    function: function.clone(),
                    line: line.to_string(),
                    category: category.clone(),
                    captures,
                    colors: colors.clone(),
                });
            }
        }
        TriggerActions {
            commands,
            events,
            lua,
        }
    }

    pub fn add_runtime_trigger(&mut self, pattern: &str, commands: Vec<String>) -> Result<()> {
        self.add_runtime_trigger_with_type(pattern, commands, None)
    }

    pub fn add_runtime_trigger_with_type(
        &mut self,
        pattern: &str,
        commands: Vec<String>,
        match_type: Option<MatchType>,
    ) -> Result<()> {
        self.add_runtime_trigger_with_options(pattern, commands, match_type, None, None)
    }

    pub fn add_runtime_trigger_with_options(
        &mut self,
        pattern: &str,
        commands: Vec<String>,
        match_type: Option<MatchType>,
        foreground: Option<String>,
        background: Option<String>,
    ) -> Result<()> {
        let pattern = pattern.trim();
        if pattern.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(
                "trigger pattern must not be empty".to_string(),
            ));
        }
        if commands.is_empty() || commands.iter().any(|command| command.trim().is_empty()) {
            return Err(MudClientError::ConfigValidation(
                "trigger must include at least one command".to_string(),
            ));
        }
        if commands.len() > self.max_commands_per_line {
            return Err(MudClientError::ConfigValidation(format!(
                "trigger has {} commands but triggers.max_commands_per_line is {}",
                commands.len(),
                self.max_commands_per_line
            )));
        }
        let name = format!("runtime:{pattern}");
        let expanded_commands = commands
            .iter()
            .map(|command| self.variables.expand(command))
            .collect::<Result<Vec<_>>>()?;
        let uses_captures = expanded_commands
            .iter()
            .map(|command| has_capture_reference(command))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|has_capture| has_capture);
        let match_type = match_type.unwrap_or(if uses_captures {
            MatchType::Regex
        } else {
            MatchType::Plain
        });
        let config = TriggerRuleConfig {
            name: name.clone(),
            enabled: true,
            priority: 10_000,
            match_type,
            pattern: pattern.to_string(),
            foreground,
            background,
            commands,
            lua: None,
            ..TriggerRuleConfig::default()
        };
        let compiled = CompiledTrigger::new_runtime(config, &self.variables)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != pattern);
        self.fired_once.remove(&format!("runtime:{name}"));
        self.rules.push(compiled);
        self.sort_rules();
        Ok(())
    }

    pub fn trigger_configs(&self) -> Vec<&TriggerRuleConfig> {
        self.rules.iter().map(|rule| &rule.config).collect()
    }

    pub fn remove_runtime_trigger(&mut self, pattern: &str) -> bool {
        let pattern = pattern.trim();
        let removed = self
            .rules
            .iter()
            .filter(|rule| rule.runtime && rule.config.pattern == pattern)
            .map(|rule| rule.config.name.clone())
            .collect::<Vec<_>>();
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != pattern);
        for name in &removed {
            self.fired_once.remove(name);
        }
        !removed.is_empty()
    }

    pub fn clear_runtime_triggers(&mut self) -> usize {
        let removed = self
            .rules
            .iter()
            .filter(|rule| rule.runtime)
            .map(|rule| rule.config.name.clone())
            .collect::<Vec<_>>();
        self.rules.retain(|rule| !rule.runtime);
        for name in &removed {
            self.fired_once.remove(name);
        }
        removed.len()
    }

    pub fn trigger_entries(&self) -> Vec<(&TriggerRuleConfig, bool)> {
        self.rules
            .iter()
            .map(|rule| (&rule.config, rule.runtime))
            .collect()
    }

    pub fn runtime_configs(&self) -> Vec<TriggerRuleConfig> {
        self.rules
            .iter()
            .filter(|rule| rule.runtime)
            .map(|rule| rule.config.clone())
            .collect()
    }

    pub fn add_runtime_config(&mut self, config: TriggerRuleConfig) -> Result<()> {
        let compiled = CompiledTrigger::new_runtime(config, &self.variables)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != compiled.config.pattern);
        self.rules.push(compiled);
        self.sort_rules();
        Ok(())
    }

    pub fn max_commands_per_line(&self) -> usize {
        self.max_commands_per_line
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

impl CompiledTrigger {
    fn new(config: TriggerRuleConfig, variables: &VariableStore) -> Result<Self> {
        Self::compile(config, false, variables)
    }

    fn new_runtime(config: TriggerRuleConfig, variables: &VariableStore) -> Result<Self> {
        Self::compile(config, true, variables)
    }

    fn compile(
        config: TriggerRuleConfig,
        runtime: bool,
        variables: &VariableStore,
    ) -> Result<Self> {
        let pattern = variables.expand(&config.pattern)?;
        if pattern.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(format!(
                "trigger `{}` pattern expands to an empty value",
                config.name
            )));
        }
        let commands = config
            .commands
            .iter()
            .map(|command| variables.expand(command))
            .collect::<Result<Vec<_>>>()?;
        let event = config
            .event
            .as_deref()
            .map(|event| variables.expand(event))
            .transpose()?;
        if commands.iter().any(|command| command.trim().is_empty())
            || event.as_ref().is_some_and(|event| event.trim().is_empty())
        {
            return Err(MudClientError::ConfigValidation(format!(
                "trigger `{}` action expands to an empty value",
                config.name
            )));
        }
        let regex = if config.match_type == MatchType::Regex {
            Some(Regex::new(&pattern).map_err(|error| {
                MudClientError::ConfigValidation(format!(
                    "trigger `{}` has invalid regex pattern: {error}",
                    config.name
                ))
            })?)
        } else {
            None
        };
        let foreground = config
            .foreground
            .as_deref()
            .map(Self::parse_trigger_color)
            .transpose()?;
        let background = config
            .background
            .as_deref()
            .map(Self::parse_trigger_color)
            .transpose()?;
        let available_captures = regex
            .as_ref()
            .map_or(0, |regex| regex.captures_len().saturating_sub(1));
        for template in commands.iter().chain(event.iter()) {
            validate_capture_references(
                template,
                available_captures,
                &format!("trigger `{}`", config.name),
            )?;
        }
        Ok(Self {
            config,
            pattern,
            commands,
            event,
            regex,
            foreground,
            background,
            last_match: None,
            runtime,
        })
    }

    fn key(&self) -> String {
        format!(
            "{}:{}",
            if self.runtime { "runtime" } else { "config" },
            self.config.name
        )
    }

    fn captures(
        &self,
        line: &str,
        category: OutputCategory,
        colors: &AnsiColors,
        now: Instant,
    ) -> Option<Vec<String>> {
        if !self.config.enabled
            || !category_allowed(&self.config.categories, category)
            || self
                .foreground
                .is_some_and(|color| !colors.foregrounds.contains(&color))
            || self
                .background
                .is_some_and(|color| !colors.backgrounds.contains(&color))
            || self.on_cooldown(now)
        {
            return None;
        }
        match self.config.match_type {
            MatchType::Plain if line.contains(&self.pattern) => Some(vec![line.to_string()]),
            MatchType::Plain => None,
            MatchType::Regex => self.regex.as_ref().and_then(|regex| {
                regex.captures(line).map(|captures| {
                    (0..captures.len())
                        .map(|index| {
                            captures
                                .get(index)
                                .map(|capture| capture.as_str().to_string())
                                .unwrap_or_default()
                        })
                        .collect()
                })
            }),
        }
    }

    fn parse_trigger_color(value: &str) -> Result<AnsiColor> {
        parse_ansi_color(value).ok_or_else(|| {
            MudClientError::ConfigValidation(format!(
                "invalid trigger color `{value}`; use a named color, index:N, or #RRGGBB"
            ))
        })
    }

    fn on_cooldown(&self, now: Instant) -> bool {
        self.config.cooldown_ms > 0
            && self.last_match.is_some_and(|last_match| {
                now.duration_since(last_match) < Duration::from_millis(self.config.cooldown_ms)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VariableConfig;
    use std::collections::BTreeMap;

    fn engine(rules: Vec<TriggerRuleConfig>) -> TriggerEngine {
        TriggerEngine::new(&TriggerConfig {
            rules,
            ..TriggerConfig::default()
        })
        .expect("triggers should compile")
    }

    #[test]
    fn expands_regex_trigger_commands_and_events() {
        let mut engine = engine(vec![TriggerRuleConfig {
            name: "enemy".to_string(),
            match_type: MatchType::Regex,
            pattern: "^(.+) arrives\\.$".to_string(),
            commands: vec!["target {1}".to_string()],
            event: Some("Enemy:{1}".to_string()),
            ..TriggerRuleConfig::default()
        }]);

        let actions = engine.evaluate("Orc arrives.", OutputCategory::Normal);

        assert_eq!(actions.commands, vec!["target Orc"]);
        assert_eq!(actions.events[0].name, "Enemy:Orc");
    }

    #[test]
    fn honors_one_shot_triggers() {
        let mut engine = engine(vec![TriggerRuleConfig {
            name: "once".to_string(),
            pattern: "hungry".to_string(),
            commands: vec!["eat bread".to_string()],
            one_shot: true,
            ..TriggerRuleConfig::default()
        }]);

        assert_eq!(
            engine
                .evaluate("You are hungry.", OutputCategory::Normal)
                .commands,
            vec!["eat bread"]
        );
        assert!(
            engine
                .evaluate("You are hungry.", OutputCategory::Normal)
                .commands
                .is_empty()
        );
    }

    #[test]
    fn runtime_plain_trigger_fires_commands() {
        let mut engine = engine(Vec::new());

        engine
            .add_runtime_trigger("You are hungry", vec!["eat bread".to_string()])
            .expect("runtime trigger should compile");

        assert_eq!(
            engine
                .evaluate("You are hungry.", OutputCategory::Normal)
                .commands,
            vec!["eat bread"]
        );
        assert_eq!(engine.trigger_configs().len(), 1);
        assert_eq!(engine.trigger_configs()[0].match_type, MatchType::Plain);
    }

    #[test]
    fn runtime_trigger_uses_regex_when_commands_reference_captures() {
        let mut engine = engine(Vec::new());

        engine
            .add_runtime_trigger(
                "^(.+) arrives\\.$",
                vec!["target {1}".to_string(), "consider {1}".to_string()],
            )
            .expect("runtime regex trigger should compile");

        let actions = engine.evaluate("Orc arrives.", OutputCategory::Normal);
        assert_eq!(actions.commands, vec!["target Orc", "consider Orc"]);
        assert_eq!(engine.trigger_configs()[0].match_type, MatchType::Regex);
    }

    #[test]
    fn runtime_trigger_replaces_same_pattern() {
        let mut engine = engine(Vec::new());
        engine
            .add_runtime_trigger("hungry", vec!["eat bread".to_string()])
            .expect("first runtime trigger should compile");
        engine
            .add_runtime_trigger("hungry", vec!["eat meat".to_string()])
            .expect("replacement runtime trigger should compile");

        assert_eq!(engine.trigger_configs().len(), 1);
        assert_eq!(
            engine.evaluate("hungry", OutputCategory::Normal).commands,
            vec!["eat meat"]
        );
    }

    #[test]
    fn runtime_trigger_does_not_enable_configured_triggers() {
        let mut engine = TriggerEngine::new(&TriggerConfig {
            enabled: false,
            rules: vec![TriggerRuleConfig {
                name: "configured".to_string(),
                pattern: "hungry".to_string(),
                commands: vec!["configured command".to_string()],
                ..TriggerRuleConfig::default()
            }],
            ..TriggerConfig::default()
        })
        .expect("configured trigger should compile");
        engine
            .add_runtime_trigger("hungry", vec!["runtime command".to_string()])
            .expect("runtime trigger should compile");

        assert_eq!(
            engine.evaluate("hungry", OutputCategory::Normal).commands,
            vec!["runtime command"]
        );
    }

    #[test]
    fn invalid_runtime_replacement_preserves_existing_trigger() {
        let mut engine = engine(Vec::new());
        engine
            .add_runtime_trigger("hungry", vec!["eat bread".to_string()])
            .expect("runtime trigger should compile");

        assert!(
            engine
                .add_runtime_trigger("hungry", vec!["eat {1}".to_string()])
                .is_err()
        );
        assert_eq!(
            engine.evaluate("hungry", OutputCategory::Normal).commands,
            vec!["eat bread"]
        );
    }

    #[test]
    fn expands_variables_in_trigger_patterns_commands_and_events() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([
                ("arrival".to_string(), "^(.+) arrives\\.$".to_string()),
                ("verb".to_string(), "target".to_string()),
                ("event".to_string(), "EnemyEntered".to_string()),
            ]),
            ..VariableConfig::default()
        })
        .unwrap();
        let mut engine = TriggerEngine::new_with_variables(
            &TriggerConfig {
                rules: vec![TriggerRuleConfig {
                    name: "variable-trigger".to_string(),
                    match_type: MatchType::Regex,
                    pattern: "${arrival}".to_string(),
                    commands: vec!["${verb} {1}".to_string()],
                    event: Some("${event}:{1}".to_string()),
                    ..TriggerRuleConfig::default()
                }],
                ..TriggerConfig::default()
            },
            &variables,
        )
        .unwrap();

        let actions = engine.evaluate("Orc arrives.", OutputCategory::Normal);

        assert_eq!(actions.commands, ["target Orc"]);
        assert_eq!(actions.events[0].name, "EnemyEntered:Orc");
    }

    #[test]
    fn runtime_trigger_infers_regex_after_variable_expansion() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([("capture".to_string(), "{1}".to_string())]),
            ..VariableConfig::default()
        })
        .unwrap();
        let mut engine =
            TriggerEngine::new_with_variables(&TriggerConfig::default(), &variables).unwrap();

        engine
            .add_runtime_trigger("^(.+) arrives\\.$", vec!["target ${capture}".to_string()])
            .unwrap();

        assert_eq!(
            engine
                .evaluate("Orc arrives.", OutputCategory::Normal)
                .commands,
            ["target Orc"]
        );
        assert_eq!(engine.trigger_configs()[0].match_type, MatchType::Regex);
    }

    #[test]
    fn color_filtered_trigger_requires_matching_ansi_colors() {
        let mut engine = engine(vec![TriggerRuleConfig {
            name: "colored".to_string(),
            pattern: "Danger".to_string(),
            foreground: Some("lightred".to_string()),
            background: Some("index:17".to_string()),
            commands: vec!["flee".to_string()],
            ..TriggerRuleConfig::default()
        }]);
        let matching = AnsiColors {
            foregrounds: vec![AnsiColor::BrightRed],
            backgrounds: vec![AnsiColor::Indexed(17)],
        };

        assert!(
            engine
                .evaluate("Danger", OutputCategory::Normal)
                .commands
                .is_empty()
        );
        assert_eq!(
            engine
                .evaluate_with_colors("Danger", OutputCategory::Normal, &matching)
                .commands,
            ["flee"]
        );
    }

    #[test]
    fn event_only_triggers_respect_per_line_action_limit() {
        let mut engine = TriggerEngine::new(&TriggerConfig {
            max_commands_per_line: 1,
            rules: vec![
                TriggerRuleConfig {
                    name: "first".to_string(),
                    pattern: "Danger".to_string(),
                    event: Some("First".to_string()),
                    ..TriggerRuleConfig::default()
                },
                TriggerRuleConfig {
                    name: "second".to_string(),
                    pattern: "Danger".to_string(),
                    event: Some("Second".to_string()),
                    ..TriggerRuleConfig::default()
                },
            ],
            ..TriggerConfig::default()
        })
        .unwrap();

        let actions = engine.evaluate("Danger", OutputCategory::Normal);

        assert!(actions.commands.is_empty());
        assert_eq!(actions.events.len(), 1);
    }

    #[test]
    fn runtime_trigger_rejects_too_many_commands() {
        let mut engine = TriggerEngine::new(&TriggerConfig {
            max_commands_per_line: 1,
            ..TriggerConfig::default()
        })
        .expect("trigger engine should compile");

        assert!(
            engine
                .add_runtime_trigger("hungry", vec!["eat".to_string(), "drink".to_string()])
                .is_err()
        );
    }
}
