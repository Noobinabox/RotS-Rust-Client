use regex::Regex;

use crate::{
    config::{AliasConfig, AliasMatchType, AliasRuleConfig},
    error::{MudClientError, Result},
    scripting::templates::{
        has_capture_reference, substitute_captures, validate_capture_references,
    },
    scripting::variables::VariableStore,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AliasActions {
    pub commands: Vec<String>,
    pub lua: Option<LuaAliasHook>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaAliasHook {
    pub function: String,
    pub input: String,
    pub captures: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AliasEngine {
    enabled: bool,
    max_expansion_depth: usize,
    max_expanded_commands: usize,
    rules: Vec<CompiledAlias>,
    variables: VariableStore,
}

#[derive(Debug, Clone)]
struct CompiledAlias {
    config: AliasRuleConfig,
    pattern: String,
    commands: Vec<String>,
    regex: Option<Regex>,
    runtime: bool,
}

impl AliasEngine {
    pub fn new(config: &AliasConfig) -> Result<Self> {
        Self::new_with_variables(config, &VariableStore::empty())
    }

    pub fn new_with_variables(config: &AliasConfig, variables: &VariableStore) -> Result<Self> {
        let rules = config
            .rules
            .iter()
            .cloned()
            .map(|rule| CompiledAlias::new(rule, variables))
            .collect::<Result<Vec<_>>>()?;
        let mut engine = Self {
            enabled: config.enabled,
            max_expansion_depth: config.max_expansion_depth,
            max_expanded_commands: config.max_expanded_commands,
            rules,
            variables: variables.clone(),
        };
        engine.sort_rules();
        Ok(engine)
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_expansion_depth: 1,
            max_expanded_commands: 1,
            rules: Vec::new(),
            variables: VariableStore::empty(),
        }
    }

    pub fn set_variables(&mut self, variables: &VariableStore) -> Result<()> {
        let rules = self
            .rules
            .iter()
            .map(|rule| CompiledAlias::compile(rule.config.clone(), variables, rule.runtime))
            .collect::<Result<Vec<_>>>()?;
        self.rules = rules;
        self.variables = variables.clone();
        self.sort_rules();
        Ok(())
    }

    pub fn expand(&self, command: &str) -> Result<Vec<String>> {
        Ok(self.expand_actions(command)?.commands)
    }

    pub fn expand_actions(&self, command: &str) -> Result<AliasActions> {
        let command = command.trim_end();
        if command.is_empty() {
            return Ok(AliasActions::default());
        }
        let mut expanded_commands = 0;
        let commands = self.expand_inner(command, 0, &mut expanded_commands)?;
        let lua = self
            .expand_lua_once(command)
            .map(|(function, captures)| LuaAliasHook {
                function,
                input: command.to_string(),
                captures,
            });
        Ok(AliasActions { commands, lua })
    }

    pub fn add_runtime_alias(&mut self, pattern: &str, commands: Vec<String>) -> Result<()> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Err(MudClientError::ConfigValidation(
                "alias pattern must not be empty".to_string(),
            ));
        }
        if commands.is_empty() || commands.iter().any(|command| command.trim().is_empty()) {
            return Err(MudClientError::ConfigValidation(
                "alias must include at least one command".to_string(),
            ));
        }
        if commands.len() > self.max_expanded_commands {
            return Err(MudClientError::ConfigValidation(format!(
                "alias has {} commands but aliases.max_expanded_commands is {}",
                commands.len(),
                self.max_expanded_commands
            )));
        }
        let uses_arguments = commands
            .iter()
            .map(|command| self.variables.expand(command))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .map(|command| has_capture_reference(command))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|has_capture| has_capture);
        let config = AliasRuleConfig {
            name: pattern.to_string(),
            enabled: true,
            priority: 10_000,
            match_type: if uses_arguments {
                AliasMatchType::Prefix
            } else {
                AliasMatchType::Exact
            },
            pattern: pattern.to_string(),
            commands,
            lua: None,
        };
        let compiled = CompiledAlias::new_runtime(config, &self.variables)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.name != compiled.config.name);
        self.rules.push(compiled);
        self.enabled = true;
        self.sort_rules();
        Ok(())
    }

    pub fn remove_runtime_alias(&mut self, pattern: &str) -> bool {
        let pattern = pattern.trim();
        let before = self.rules.len();
        self.rules
            .retain(|rule| !rule.runtime || rule.config.pattern != pattern);
        before != self.rules.len()
    }

    pub fn clear_runtime_aliases(&mut self) -> usize {
        let before = self.rules.len();
        self.rules.retain(|rule| !rule.runtime);
        before - self.rules.len()
    }

    pub fn alias_configs(&self) -> Vec<&AliasRuleConfig> {
        self.rules.iter().map(|rule| &rule.config).collect()
    }

    pub fn runtime_configs(&self) -> Vec<AliasRuleConfig> {
        self.rules
            .iter()
            .filter(|rule| rule.runtime)
            .map(|rule| rule.config.clone())
            .collect()
    }

    pub fn add_runtime_config(&mut self, config: AliasRuleConfig) -> Result<()> {
        let compiled = CompiledAlias::new_runtime(config, &self.variables)?;
        self.rules
            .retain(|rule| !rule.runtime || rule.config.name != compiled.config.name);
        self.rules.push(compiled);
        self.enabled = true;
        self.sort_rules();
        Ok(())
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

    fn expand_inner(
        &self,
        command: &str,
        depth: usize,
        expanded_commands: &mut usize,
    ) -> Result<Vec<String>> {
        if !self.enabled {
            return Ok(vec![command.to_string()]);
        }
        let Some(expanded) = self.expand_once(command) else {
            *expanded_commands = expanded_commands.saturating_add(expanded_command_count(command));
            if *expanded_commands > self.max_expanded_commands {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias expansion exceeded maximum command count of {}",
                    self.max_expanded_commands
                )));
            }
            return Ok(vec![command.to_string()]);
        };
        if depth >= self.max_expansion_depth {
            return Err(MudClientError::ConfigValidation(format!(
                "alias expansion exceeded maximum depth of {}",
                self.max_expansion_depth
            )));
        }

        let mut result = Vec::new();
        for command in expanded {
            result.extend(self.expand_inner(&command, depth + 1, expanded_commands)?);
        }
        Ok(result)
    }

    fn expand_once(&self, command: &str) -> Option<Vec<String>> {
        for rule in &self.rules {
            if !rule.config.enabled {
                continue;
            }
            if let Some(captures) = rule.captures(command) {
                return Some(
                    rule.commands
                        .iter()
                        .map(|template| substitute_captures(template, &captures))
                        .collect(),
                );
            }
        }
        None
    }

    fn expand_lua_once(&self, command: &str) -> Option<(String, Vec<String>)> {
        for rule in &self.rules {
            if !rule.config.enabled {
                continue;
            }
            let Some(lua) = &rule.config.lua else {
                continue;
            };
            if let Some(captures) = rule.captures(command) {
                return Some((lua.clone(), captures));
            }
        }
        None
    }
}

fn expanded_command_count(command: &str) -> usize {
    if command.trim_start().starts_with('/') {
        1
    } else {
        command
            .split(';')
            .filter(|part| !part.trim().is_empty())
            .count()
    }
}

impl CompiledAlias {
    fn new(config: AliasRuleConfig, variables: &VariableStore) -> Result<Self> {
        Self::compile(config, variables, false)
    }

    fn new_runtime(config: AliasRuleConfig, variables: &VariableStore) -> Result<Self> {
        Self::compile(config, variables, true)
    }

    fn compile(config: AliasRuleConfig, variables: &VariableStore, runtime: bool) -> Result<Self> {
        let pattern = variables.expand(&config.pattern)?;
        if pattern.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(format!(
                "alias `{}` pattern expands to an empty value",
                config.name
            )));
        }
        let commands = config
            .commands
            .iter()
            .map(|command| variables.expand(command))
            .collect::<Result<Vec<_>>>()?;
        if commands.iter().any(|command| command.trim().is_empty()) {
            return Err(MudClientError::ConfigValidation(format!(
                "alias `{}` command expands to an empty value",
                config.name
            )));
        }
        let regex = if config.match_type == AliasMatchType::Regex {
            Some(Regex::new(&pattern).map_err(|error| {
                MudClientError::ConfigValidation(format!(
                    "alias `{}` has invalid regex pattern: {error}",
                    config.name
                ))
            })?)
        } else {
            None
        };
        let available_captures = match config.match_type {
            AliasMatchType::Exact => 0,
            AliasMatchType::Prefix => 1,
            AliasMatchType::Regex => regex
                .as_ref()
                .map_or(0, |regex| regex.captures_len().saturating_sub(1)),
        };
        for command in &commands {
            validate_capture_references(
                command,
                available_captures,
                &format!("alias `{}`", config.name),
            )?;
        }
        Ok(Self {
            config,
            pattern,
            commands,
            regex,
            runtime,
        })
    }

    fn captures(&self, command: &str) -> Option<Vec<String>> {
        match self.config.match_type {
            AliasMatchType::Exact if command == self.pattern => Some(vec![command.to_string()]),
            AliasMatchType::Exact => None,
            AliasMatchType::Prefix if command == self.pattern => {
                Some(vec![command.to_string(), String::new()])
            }
            AliasMatchType::Prefix => command
                .strip_prefix(&self.pattern)
                .and_then(|remainder| remainder.strip_prefix(' '))
                .map(|remainder| vec![command.to_string(), remainder.to_string()]),
            AliasMatchType::Regex => self.regex.as_ref().and_then(|regex| {
                regex.captures(command).map(|captures| {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VariableConfig;
    use std::collections::BTreeMap;

    fn engine(rules: Vec<AliasRuleConfig>) -> AliasEngine {
        AliasEngine::new(&AliasConfig {
            rules,
            ..AliasConfig::default()
        })
        .expect("aliases should compile")
    }

    #[test]
    fn expands_regex_alias_with_capture() {
        let engine = engine(vec![AliasRuleConfig {
            name: "kill".to_string(),
            pattern: "^k\\s+(.+)$".to_string(),
            commands: vec!["kill {1}".to_string()],
            ..AliasRuleConfig::default()
        }]);

        assert_eq!(engine.expand("k bear").unwrap(), vec!["kill bear"]);
    }

    #[test]
    fn expands_exact_alias_to_multiple_commands() {
        let engine = engine(vec![AliasRuleConfig {
            name: "recall".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "rr".to_string(),
            commands: vec!["recall".to_string(), "look".to_string()],
            ..AliasRuleConfig::default()
        }]);

        assert_eq!(engine.expand("rr").unwrap(), vec!["recall", "look"]);
    }

    #[test]
    fn expands_prefix_alias_with_remainder() {
        let engine = engine(vec![AliasRuleConfig {
            name: "group".to_string(),
            match_type: AliasMatchType::Prefix,
            pattern: "gs".to_string(),
            commands: vec!["gtell {1}".to_string()],
            ..AliasRuleConfig::default()
        }]);

        assert_eq!(
            engine.expand("gs hello there").unwrap(),
            vec!["gtell hello there"]
        );
    }

    #[test]
    fn skips_disabled_aliases() {
        let engine = engine(vec![AliasRuleConfig {
            name: "disabled".to_string(),
            enabled: false,
            match_type: AliasMatchType::Exact,
            pattern: "x".to_string(),
            commands: vec!["look".to_string()],
            ..AliasRuleConfig::default()
        }]);

        assert_eq!(engine.expand("x").unwrap(), vec!["x"]);
    }

    #[test]
    fn prefers_higher_priority_alias() {
        let engine = engine(vec![
            AliasRuleConfig {
                name: "low".to_string(),
                priority: 1,
                match_type: AliasMatchType::Exact,
                pattern: "x".to_string(),
                commands: vec!["low".to_string()],
                ..AliasRuleConfig::default()
            },
            AliasRuleConfig {
                name: "high".to_string(),
                priority: 10,
                match_type: AliasMatchType::Exact,
                pattern: "x".to_string(),
                commands: vec!["high".to_string()],
                ..AliasRuleConfig::default()
            },
        ]);

        assert_eq!(engine.expand("x").unwrap(), vec!["high"]);
    }

    #[test]
    fn stops_recursive_alias_expansion() {
        let engine = AliasEngine::new(&AliasConfig {
            max_expansion_depth: 2,
            rules: vec![AliasRuleConfig {
                name: "loop".to_string(),
                match_type: AliasMatchType::Exact,
                pattern: "x".to_string(),
                commands: vec!["x".to_string()],
                ..AliasRuleConfig::default()
            }],
            ..AliasConfig::default()
        })
        .unwrap();

        assert!(engine.expand("x").is_err());
    }

    #[test]
    fn permits_exactly_the_configured_number_of_expansions() {
        let engine = AliasEngine::new(&AliasConfig {
            max_expansion_depth: 2,
            rules: vec![
                AliasRuleConfig {
                    name: "first".to_string(),
                    match_type: AliasMatchType::Exact,
                    pattern: "x".to_string(),
                    commands: vec!["y".to_string()],
                    ..AliasRuleConfig::default()
                },
                AliasRuleConfig {
                    name: "second".to_string(),
                    match_type: AliasMatchType::Exact,
                    pattern: "y".to_string(),
                    commands: vec!["look".to_string()],
                    ..AliasRuleConfig::default()
                },
            ],
            ..AliasConfig::default()
        })
        .unwrap();

        assert_eq!(engine.expand("x").unwrap(), ["look"]);
    }

    #[test]
    fn bounds_branching_alias_expansion() {
        let engine = AliasEngine::new(&AliasConfig {
            max_expanded_commands: 2,
            rules: vec![AliasRuleConfig {
                name: "fan-out".to_string(),
                match_type: AliasMatchType::Exact,
                pattern: "x".to_string(),
                commands: vec!["one".to_string(), "two".to_string(), "three".to_string()],
                ..AliasRuleConfig::default()
            }],
            ..AliasConfig::default()
        })
        .unwrap();

        assert!(engine.expand("x").is_err());
    }

    #[test]
    fn bounds_semicolon_separated_alias_expansion() {
        let engine = AliasEngine::new(&AliasConfig {
            max_expanded_commands: 2,
            rules: vec![AliasRuleConfig {
                name: "fan-out".to_string(),
                match_type: AliasMatchType::Exact,
                pattern: "x".to_string(),
                commands: vec!["one;two;three".to_string()],
                ..AliasRuleConfig::default()
            }],
            ..AliasConfig::default()
        })
        .unwrap();

        assert!(engine.expand("x").is_err());
    }

    #[test]
    fn expands_variables_in_alias_patterns_and_commands_before_captures() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([
                ("shortcut".to_string(), "attack".to_string()),
                ("verb".to_string(), "kill".to_string()),
            ]),
            ..VariableConfig::default()
        })
        .unwrap();
        let engine = AliasEngine::new_with_variables(
            &AliasConfig {
                rules: vec![AliasRuleConfig {
                    name: "variable-alias".to_string(),
                    match_type: AliasMatchType::Prefix,
                    pattern: "${shortcut}".to_string(),
                    commands: vec!["${verb} {1}".to_string()],
                    ..AliasRuleConfig::default()
                }],
                ..AliasConfig::default()
            },
            &variables,
        )
        .unwrap();

        assert_eq!(engine.expand("attack orc").unwrap(), vec!["kill orc"]);
    }

    #[test]
    fn runtime_alias_infers_arguments_after_variable_expansion() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([("argument".to_string(), "{1}".to_string())]),
            ..VariableConfig::default()
        })
        .unwrap();
        let mut engine =
            AliasEngine::new_with_variables(&AliasConfig::default(), &variables).unwrap();

        engine
            .add_runtime_alias("k", vec!["kill ${argument}".to_string()])
            .unwrap();

        assert_eq!(engine.expand("k orc").unwrap(), ["kill orc"]);
    }

    #[test]
    fn invalid_runtime_replacement_preserves_existing_alias() {
        let mut engine = engine(Vec::new());
        engine
            .add_runtime_alias("k", vec!["kill {1}".to_string()])
            .unwrap();

        assert!(
            engine
                .add_runtime_alias("k", vec!["kill {2}".to_string()])
                .is_err()
        );
        assert_eq!(engine.expand("k orc").unwrap(), ["kill orc"]);
    }
}
