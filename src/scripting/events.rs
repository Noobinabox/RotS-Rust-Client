use std::collections::VecDeque;

use regex::Regex;

use crate::{
    config::{EventConfig, EventHandlerConfig, MatchType},
    error::{MudClientError, Result},
    events::ScriptEvent,
    scripting::templates::{
        has_capture_reference, substitute_captures, validate_capture_references,
    },
    scripting::variables::VariableStore,
    state::ScriptEventRecord,
};

#[derive(Debug, Clone)]
pub struct EventEngine {
    enabled: bool,
    max_dispatch_depth: usize,
    max_events_per_dispatch: usize,
    max_commands_per_dispatch: usize,
    handlers: Vec<CompiledHandler>,
    variables: VariableStore,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventDispatch {
    pub records: Vec<ScriptEventRecord>,
    pub commands: Vec<String>,
    pub notifications: Vec<String>,
    pub lua: Vec<LuaEventHook>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaEventHook {
    pub function: String,
    pub name: String,
    pub source: String,
    pub captures: Vec<String>,
}

#[derive(Debug, Clone)]
struct CompiledHandler {
    config: EventHandlerConfig,
    event: String,
    commands: Vec<String>,
    emit: Vec<String>,
    notification: Option<String>,
    regex: Option<Regex>,
    runtime: bool,
}

impl EventEngine {
    pub fn new(config: &EventConfig) -> Result<Self> {
        Self::new_with_variables(config, &VariableStore::empty())
    }

    pub fn new_with_variables(config: &EventConfig, variables: &VariableStore) -> Result<Self> {
        let mut handlers = config
            .handlers
            .iter()
            .cloned()
            .map(|handler| CompiledHandler::new(handler, variables, false))
            .collect::<Result<Vec<_>>>()?;
        sort_handlers(&mut handlers);
        Ok(Self {
            enabled: config.enabled,
            max_dispatch_depth: config.max_dispatch_depth,
            max_events_per_dispatch: config.max_events_per_dispatch,
            max_commands_per_dispatch: config.max_commands_per_dispatch,
            handlers,
            variables: variables.clone(),
        })
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_dispatch_depth: 1,
            max_events_per_dispatch: 1,
            max_commands_per_dispatch: 1,
            handlers: Vec::new(),
            variables: VariableStore::empty(),
        }
    }

    pub fn set_variables(&mut self, variables: &VariableStore) -> Result<()> {
        let mut handlers = self
            .handlers
            .iter()
            .map(|handler| CompiledHandler::new(handler.config.clone(), variables, handler.runtime))
            .collect::<Result<Vec<_>>>()?;
        sort_handlers(&mut handlers);
        self.handlers = handlers;
        self.variables = variables.clone();
        Ok(())
    }

    pub fn add_runtime_handler(
        &mut self,
        event: &str,
        commands: Vec<String>,
        match_type: Option<MatchType>,
    ) -> Result<()> {
        let event = event.trim();
        if event.is_empty() {
            return Err(MudClientError::ConfigValidation(
                "event handler pattern must not be empty".to_string(),
            ));
        }
        if commands.is_empty() || commands.iter().any(|command| command.trim().is_empty()) {
            return Err(MudClientError::ConfigValidation(
                "event handler must include at least one command".to_string(),
            ));
        }
        if commands.len() > self.max_commands_per_dispatch {
            return Err(MudClientError::ConfigValidation(format!(
                "event handler has {} commands but events.max_commands_per_dispatch is {}",
                commands.len(),
                self.max_commands_per_dispatch
            )));
        }
        let uses_captures = commands
            .iter()
            .map(|command| self.variables.expand(command))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .map(|command| has_capture_reference(command))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|has_capture| has_capture);
        let config = EventHandlerConfig {
            name: format!("runtime:{event}"),
            enabled: true,
            priority: 10_000,
            match_type: match_type.unwrap_or(if uses_captures {
                MatchType::Regex
            } else {
                MatchType::Plain
            }),
            event: event.to_string(),
            commands,
            ..EventHandlerConfig::default()
        };
        let compiled = CompiledHandler::new(config, &self.variables, true)?;
        self.handlers
            .retain(|handler| !handler.runtime || handler.config.event != event);
        self.handlers.push(compiled);
        sort_handlers(&mut self.handlers);
        Ok(())
    }

    pub fn dispatch(&self, event: ScriptEvent, source: impl Into<String>) -> Result<EventDispatch> {
        if !self.enabled {
            return Ok(EventDispatch::default());
        }

        let mut queue = VecDeque::from([(event, source.into(), 0_usize)]);
        let mut dispatch = EventDispatch::default();
        while let Some((event, source, depth)) = queue.pop_front() {
            if depth >= self.max_dispatch_depth {
                return Err(MudClientError::ConfigValidation(format!(
                    "event dispatch exceeded maximum depth of {}",
                    self.max_dispatch_depth
                )));
            }
            if dispatch.records.len() >= self.max_events_per_dispatch {
                return Err(MudClientError::ConfigValidation(format!(
                    "event dispatch exceeded maximum event count of {}",
                    self.max_events_per_dispatch
                )));
            }

            let event_name = event.name();
            dispatch.records.push(ScriptEventRecord {
                name: event_name.clone(),
                source: source.clone(),
            });
            for handler in &self.handlers {
                let Some(captures) = handler.captures(&event_name) else {
                    continue;
                };
                let remaining_commands = self
                    .max_commands_per_dispatch
                    .saturating_sub(dispatch.commands.len());
                if handler.commands.len() > remaining_commands {
                    return Err(MudClientError::ConfigValidation(format!(
                        "event dispatch exceeded maximum command count of {}",
                        self.max_commands_per_dispatch
                    )));
                }
                dispatch.commands.extend(
                    handler
                        .commands
                        .iter()
                        .map(|command| substitute_captures(command, &captures)),
                );
                if let Some(notification) = &handler.notification {
                    dispatch
                        .notifications
                        .push(substitute_captures(notification, &captures));
                }
                if let Some(function) = &handler.config.lua {
                    dispatch.lua.push(LuaEventHook {
                        function: function.clone(),
                        name: event_name.clone(),
                        source: source.clone(),
                        captures: captures.clone(),
                    });
                }
                for emitted in &handler.emit {
                    if dispatch.records.len().saturating_add(queue.len())
                        >= self.max_events_per_dispatch
                    {
                        return Err(MudClientError::ConfigValidation(format!(
                            "event dispatch exceeded maximum event count of {}",
                            self.max_events_per_dispatch
                        )));
                    }
                    let emitted = substitute_captures(emitted, &captures);
                    queue.push_back((
                        ScriptEvent::parse(emitted),
                        format!("event:{event_name}"),
                        depth + 1,
                    ));
                }
            }
        }
        Ok(dispatch)
    }

    pub fn handler_configs(&self) -> Vec<&EventHandlerConfig> {
        self.handlers
            .iter()
            .map(|handler| &handler.config)
            .collect()
    }

    pub fn remove_runtime_handler(&mut self, event: &str) -> bool {
        let event = event.trim();
        let before = self.handlers.len();
        self.handlers
            .retain(|handler| !handler.runtime || handler.config.event != event);
        before != self.handlers.len()
    }

    pub fn clear_runtime_handlers(&mut self) -> usize {
        let before = self.handlers.len();
        self.handlers.retain(|handler| !handler.runtime);
        before - self.handlers.len()
    }

    pub fn handler_entries(&self) -> Vec<(&EventHandlerConfig, bool)> {
        self.handlers
            .iter()
            .map(|handler| (&handler.config, handler.runtime))
            .collect()
    }

    pub fn runtime_configs(&self) -> Vec<EventHandlerConfig> {
        self.handlers
            .iter()
            .filter(|handler| handler.runtime)
            .map(|handler| handler.config.clone())
            .collect()
    }

    pub fn add_runtime_config(&mut self, config: EventHandlerConfig) -> Result<()> {
        if config.commands.is_empty()
            || config
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
        {
            return Err(MudClientError::ConfigValidation(
                "runtime event handler must include at least one command".to_string(),
            ));
        }
        if config.commands.len() > self.max_commands_per_dispatch {
            return Err(MudClientError::ConfigValidation(format!(
                "runtime event handler has {} commands but events.max_commands_per_dispatch is {}",
                config.commands.len(),
                self.max_commands_per_dispatch
            )));
        }
        let compiled = CompiledHandler::new(config, &self.variables, true)?;
        self.handlers
            .retain(|handler| !handler.runtime || handler.config.event != compiled.config.event);
        self.handlers.push(compiled);
        sort_handlers(&mut self.handlers);
        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl CompiledHandler {
    fn new(config: EventHandlerConfig, variables: &VariableStore, runtime: bool) -> Result<Self> {
        let event = variables.expand(&config.event)?;
        if event.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(format!(
                "event handler `{}` pattern expands to an empty value",
                config.name
            )));
        }
        let commands = config
            .commands
            .iter()
            .map(|command| variables.expand(command))
            .collect::<Result<Vec<_>>>()?;
        let emit = config
            .emit
            .iter()
            .map(|event| variables.expand(event))
            .collect::<Result<Vec<_>>>()?;
        let notification = config
            .notification
            .as_deref()
            .map(|notification| variables.expand(notification))
            .transpose()?;
        if commands.iter().any(|command| command.trim().is_empty())
            || emit.iter().any(|event| event.trim().is_empty())
            || notification
                .as_ref()
                .is_some_and(|notification| notification.trim().is_empty())
        {
            return Err(MudClientError::ConfigValidation(format!(
                "event handler `{}` action expands to an empty value",
                config.name
            )));
        }
        let regex = if config.match_type == MatchType::Regex {
            Some(Regex::new(&event).map_err(|error| {
                MudClientError::ConfigValidation(format!(
                    "event handler `{}` has invalid regex pattern: {error}",
                    config.name
                ))
            })?)
        } else {
            None
        };
        let available_captures = regex
            .as_ref()
            .map_or(0, |regex| regex.captures_len().saturating_sub(1));
        for template in commands.iter().chain(&emit).chain(notification.iter()) {
            validate_capture_references(
                template,
                available_captures,
                &format!("event handler `{}`", config.name),
            )?;
        }
        Ok(Self {
            config,
            event,
            commands,
            emit,
            notification,
            regex,
            runtime,
        })
    }

    fn captures(&self, event_name: &str) -> Option<Vec<String>> {
        if !self.config.enabled {
            return None;
        }
        match self.config.match_type {
            MatchType::Plain if self.event == event_name => Some(vec![event_name.to_string()]),
            MatchType::Plain => None,
            MatchType::Regex => self.regex.as_ref().and_then(|regex| {
                regex.captures(event_name).map(|captures| {
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

fn sort_handlers(handlers: &mut [CompiledHandler]) {
    handlers.sort_by(|left, right| {
        right
            .runtime
            .cmp(&left.runtime)
            .then_with(|| right.config.priority.cmp(&left.config.priority))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VariableConfig;
    use std::collections::BTreeMap;

    fn engine(handlers: Vec<EventHandlerConfig>) -> EventEngine {
        EventEngine::new(&EventConfig {
            handlers,
            ..EventConfig::default()
        })
        .expect("event handlers should compile")
    }

    #[test]
    fn dispatches_exact_handler_actions() {
        let engine = engine(vec![EventHandlerConfig {
            name: "low-health".to_string(),
            event: "LowHealth".to_string(),
            commands: vec!["flee".to_string()],
            notification: Some("Health is low".to_string()),
            ..EventHandlerConfig::default()
        }]);

        let dispatch = engine
            .dispatch(ScriptEvent::LowHealth, "trigger:wounded")
            .expect("event should dispatch");

        assert_eq!(dispatch.commands, ["flee"]);
        assert_eq!(dispatch.notifications, ["Health is low"]);
        assert_eq!(dispatch.records[0].name, "LowHealth");
    }

    #[test]
    fn substitutes_regex_captures_and_cascades_in_fifo_order() {
        let engine = engine(vec![
            EventHandlerConfig {
                name: "enemy".to_string(),
                priority: 100,
                match_type: MatchType::Regex,
                event: "^EnemyEntered:(.+)$".to_string(),
                commands: vec!["target {1}".to_string()],
                emit: vec!["TargetChanged:{1}".to_string()],
                ..EventHandlerConfig::default()
            },
            EventHandlerConfig {
                name: "target".to_string(),
                event: "TargetChanged:Orc".to_string(),
                notification: Some("Targeting Orc".to_string()),
                ..EventHandlerConfig::default()
            },
        ]);

        let dispatch = engine
            .dispatch(
                ScriptEvent::EnemyEntered {
                    name: "Orc".to_string(),
                },
                "trigger:arrival",
            )
            .expect("event cascade should dispatch");

        assert_eq!(dispatch.commands, ["target Orc"]);
        assert_eq!(dispatch.notifications, ["Targeting Orc"]);
        assert_eq!(
            dispatch
                .records
                .iter()
                .map(|record| record.name.as_str())
                .collect::<Vec<_>>(),
            ["EnemyEntered:Orc", "TargetChanged:Orc"]
        );
    }

    #[test]
    fn rejects_recursive_event_chains_at_depth_limit() {
        let engine = EventEngine::new(&EventConfig {
            max_dispatch_depth: 2,
            handlers: vec![EventHandlerConfig {
                name: "loop".to_string(),
                event: "Loop".to_string(),
                emit: vec!["Loop".to_string()],
                ..EventHandlerConfig::default()
            }],
            ..EventConfig::default()
        })
        .expect("event handler should compile");

        assert!(
            engine
                .dispatch(ScriptEvent::Custom("Loop".to_string()), "manual")
                .is_err()
        );
    }

    #[test]
    fn rejects_dispatches_over_command_limit() {
        let engine = EventEngine::new(&EventConfig {
            max_commands_per_dispatch: 1,
            handlers: vec![EventHandlerConfig {
                name: "commands".to_string(),
                event: "Many".to_string(),
                commands: vec!["one".to_string(), "two".to_string()],
                ..EventHandlerConfig::default()
            }],
            ..EventConfig::default()
        })
        .expect("event handler should compile");

        assert!(
            engine
                .dispatch(ScriptEvent::Custom("Many".to_string()), "manual")
                .is_err()
        );
    }

    #[test]
    fn honors_priority_and_disabled_handlers() {
        let engine = engine(vec![
            EventHandlerConfig {
                name: "low".to_string(),
                priority: 1,
                event: "Ready".to_string(),
                commands: vec!["low".to_string()],
                ..EventHandlerConfig::default()
            },
            EventHandlerConfig {
                name: "disabled".to_string(),
                enabled: false,
                priority: 200,
                event: "Ready".to_string(),
                commands: vec!["disabled".to_string()],
                ..EventHandlerConfig::default()
            },
            EventHandlerConfig {
                name: "high".to_string(),
                priority: 100,
                event: "Ready".to_string(),
                commands: vec!["high".to_string()],
                ..EventHandlerConfig::default()
            },
        ]);

        let dispatch = engine
            .dispatch(ScriptEvent::Custom("Ready".to_string()), "manual")
            .expect("event should dispatch");

        assert_eq!(dispatch.commands, ["high", "low"]);
    }

    #[test]
    fn preserves_configuration_order_for_equal_priority_handlers() {
        let engine = engine(vec![
            EventHandlerConfig {
                name: "z-first".to_string(),
                event: "Ready".to_string(),
                commands: vec!["first".to_string()],
                ..EventHandlerConfig::default()
            },
            EventHandlerConfig {
                name: "a-second".to_string(),
                event: "Ready".to_string(),
                commands: vec!["second".to_string()],
                ..EventHandlerConfig::default()
            },
        ]);

        let dispatch = engine
            .dispatch(ScriptEvent::Custom("Ready".to_string()), "manual")
            .expect("event should dispatch");

        assert_eq!(dispatch.commands, ["first", "second"]);
    }

    #[test]
    fn rejects_invalid_capture_references() {
        let result = EventEngine::new(&EventConfig {
            handlers: vec![EventHandlerConfig {
                name: "bad-capture".to_string(),
                match_type: MatchType::Regex,
                event: "^Enemy:(.+)$".to_string(),
                commands: vec!["target {2}".to_string()],
                ..EventHandlerConfig::default()
            }],
            ..EventConfig::default()
        });

        assert!(result.is_err());
    }

    #[test]
    fn expands_variables_in_handler_patterns_and_actions() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([
                ("event".to_string(), "^Enemy:(.+)$".to_string()),
                ("verb".to_string(), "target".to_string()),
                ("notice".to_string(), "Enemy seen".to_string()),
            ]),
            ..VariableConfig::default()
        })
        .unwrap();
        let engine = EventEngine::new_with_variables(
            &EventConfig {
                handlers: vec![EventHandlerConfig {
                    name: "variable-event".to_string(),
                    match_type: MatchType::Regex,
                    event: "${event}".to_string(),
                    commands: vec!["${verb} {1}".to_string()],
                    emit: vec!["Targeted:{1}".to_string()],
                    notification: Some("${notice}: {1}".to_string()),
                    ..EventHandlerConfig::default()
                }],
                ..EventConfig::default()
            },
            &variables,
        )
        .unwrap();

        let dispatch = engine
            .dispatch(ScriptEvent::Custom("Enemy:Orc".to_string()), "test")
            .unwrap();

        assert_eq!(dispatch.commands, ["target Orc"]);
        assert_eq!(dispatch.notifications, ["Enemy seen: Orc"]);
        assert_eq!(dispatch.records[1].name, "Targeted:Orc");
    }

    #[test]
    fn runtime_handler_infers_regex_and_replaces_same_event() {
        let mut engine = engine(Vec::new());
        engine
            .add_runtime_handler("^Enemy:(.+)$", vec!["target {1}".to_string()], None)
            .unwrap();
        engine
            .add_runtime_handler("^Enemy:(.+)$", vec!["consider {1}".to_string()], None)
            .unwrap();

        let dispatch = engine
            .dispatch(ScriptEvent::Custom("Enemy:Orc".to_string()), "test")
            .unwrap();

        assert_eq!(dispatch.commands, ["consider Orc"]);
        assert_eq!(engine.runtime_configs().len(), 1);
        assert_eq!(engine.runtime_configs()[0].match_type, MatchType::Regex);
    }

    #[test]
    fn invalid_runtime_replacement_preserves_existing_handler() {
        let mut engine = engine(Vec::new());
        engine
            .add_runtime_handler("Ready", vec!["look".to_string()], None)
            .unwrap();

        assert!(
            engine
                .add_runtime_handler(
                    "Ready",
                    vec!["target {2}".to_string()],
                    Some(MatchType::Regex),
                )
                .is_err()
        );
        assert_eq!(
            engine
                .dispatch(ScriptEvent::Custom("Ready".to_string()), "test")
                .unwrap()
                .commands,
            ["look"]
        );
    }

    #[test]
    fn runtime_handler_templates_recompile_with_variables() {
        let variables = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([
                ("event".to_string(), "Ready".to_string()),
                ("command".to_string(), "look".to_string()),
            ]),
            ..VariableConfig::default()
        })
        .unwrap();
        let mut engine =
            EventEngine::new_with_variables(&EventConfig::default(), &variables).unwrap();
        engine
            .add_runtime_handler(
                "${event}",
                vec!["${command}".to_string()],
                Some(MatchType::Plain),
            )
            .unwrap();
        let updated = VariableStore::new(&VariableConfig {
            values: BTreeMap::from([
                ("event".to_string(), "Changed".to_string()),
                ("command".to_string(), "score".to_string()),
            ]),
            ..VariableConfig::default()
        })
        .unwrap();

        engine.set_variables(&updated).unwrap();

        assert_eq!(
            engine
                .dispatch(ScriptEvent::Custom("Changed".to_string()), "test")
                .unwrap()
                .commands,
            ["score"]
        );
        assert!(engine.runtime_configs()[0].event.contains("${event}"));
    }

    #[test]
    fn runtime_handler_does_not_override_global_disable() {
        let mut engine = EventEngine::new(&EventConfig {
            enabled: false,
            ..EventConfig::default()
        })
        .unwrap();

        engine
            .add_runtime_handler("Ready", vec!["look".to_string()], None)
            .unwrap();

        assert!(!engine.is_enabled());
        assert!(
            engine
                .dispatch(ScriptEvent::Custom("Ready".to_string()), "test")
                .unwrap()
                .commands
                .is_empty()
        );
    }

    #[test]
    fn restored_runtime_handler_obeys_new_command_limit() {
        let mut engine = EventEngine::new(&EventConfig {
            max_commands_per_dispatch: 1,
            ..EventConfig::default()
        })
        .unwrap();
        let result = engine.add_runtime_config(EventHandlerConfig {
            name: "runtime:Ready".to_string(),
            priority: 10_000,
            event: "Ready".to_string(),
            commands: vec!["look".to_string(), "score".to_string()],
            ..EventHandlerConfig::default()
        });

        assert!(result.is_err());
        assert!(engine.runtime_configs().is_empty());
    }
}
