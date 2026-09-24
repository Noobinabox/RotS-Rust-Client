use std::collections::BTreeMap;

use crate::{
    config::VariableConfig,
    error::{MudClientError, Result},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableSource {
    Config,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableEntry {
    pub name: String,
    pub value: String,
    pub source: VariableSource,
}

#[derive(Debug, Clone)]
pub struct VariableStore {
    configured: BTreeMap<String, String>,
    runtime: BTreeMap<String, String>,
    max_expansion_depth: usize,
    max_expanded_bytes: usize,
}

impl VariableStore {
    /// Preserve raw templates and runtime ownership, not their expanded values.
    pub fn runtime_values(&self) -> BTreeMap<String, String> {
        self.runtime.clone()
    }

    /// Load the whole snapshot before validation so forward references do not
    /// depend on map iteration order.
    pub fn with_runtime_values(
        config: &VariableConfig,
        runtime: BTreeMap<String, String>,
    ) -> Result<Self> {
        let store = Self {
            configured: config.values.clone(),
            runtime,
            max_expansion_depth: config.max_expansion_depth,
            max_expanded_bytes: config.max_expanded_bytes,
        };
        store.validate_all()?;
        Ok(store)
    }
    pub fn new(config: &VariableConfig) -> Result<Self> {
        let store = Self {
            configured: config.values.clone(),
            runtime: BTreeMap::new(),
            max_expansion_depth: config.max_expansion_depth,
            max_expanded_bytes: config.max_expanded_bytes,
        };
        store.validate_all()?;
        Ok(store)
    }

    pub fn empty() -> Self {
        Self {
            configured: BTreeMap::new(),
            runtime: BTreeMap::new(),
            max_expansion_depth: VariableConfig::default().max_expansion_depth,
            max_expanded_bytes: VariableConfig::default().max_expanded_bytes,
        }
    }

    pub fn with_config(&self, config: &VariableConfig) -> Result<Self> {
        let store = Self {
            configured: config.values.clone(),
            runtime: self.runtime.clone(),
            max_expansion_depth: config.max_expansion_depth,
            max_expanded_bytes: config.max_expanded_bytes,
        };
        store.validate_all()?;
        Ok(store)
    }

    pub fn with_runtime(&self, name: &str, value: &str) -> Result<Self> {
        validate_name(name)?;
        let mut store = self.clone();
        store.runtime.insert(name.to_string(), value.to_string());
        store.validate_all()?;
        Ok(store)
    }

    pub fn without_runtime(&self, name: &str) -> Result<Self> {
        validate_name(name)?;
        if !self.runtime.contains_key(name) {
            return Err(MudClientError::ConfigValidation(format!(
                "runtime variable `{name}` is not defined"
            )));
        }
        let mut store = self.clone();
        store.runtime.remove(name);
        store.validate_all()?;
        Ok(store)
    }

    pub fn expand(&self, template: &str) -> Result<String> {
        self.expand_text(template, &mut Vec::new())
    }

    pub fn entries(&self) -> Result<Vec<VariableEntry>> {
        let mut names = self.configured.keys().cloned().collect::<Vec<_>>();
        names.extend(
            self.runtime
                .keys()
                .filter(|name| !self.configured.contains_key(*name))
                .cloned(),
        );
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let source = if self.runtime.contains_key(&name) {
                    VariableSource::Runtime
                } else {
                    VariableSource::Config
                };
                let value = self.resolve(&name, &mut Vec::new())?;
                Ok(VariableEntry {
                    name,
                    value,
                    source,
                })
            })
            .collect()
    }

    fn validate_all(&self) -> Result<()> {
        if self.max_expansion_depth == 0 {
            return Err(MudClientError::ConfigValidation(
                "variables.max_expansion_depth must be greater than zero".to_string(),
            ));
        }
        if self.max_expanded_bytes == 0 {
            return Err(MudClientError::ConfigValidation(
                "variables.max_expanded_bytes must be greater than zero".to_string(),
            ));
        }
        for name in self.configured.keys().chain(self.runtime.keys()) {
            validate_name(name)?;
            self.resolve(name, &mut Vec::new())?;
        }
        Ok(())
    }

    fn resolve(&self, name: &str, stack: &mut Vec<String>) -> Result<String> {
        if stack.iter().any(|current| current == name) {
            let mut chain = stack.clone();
            chain.push(name.to_string());
            return Err(MudClientError::ConfigValidation(format!(
                "recursive variable reference: {}",
                chain.join(" -> ")
            )));
        }
        if stack.len() >= self.max_expansion_depth {
            return Err(MudClientError::ConfigValidation(format!(
                "variable expansion exceeded maximum depth of {}",
                self.max_expansion_depth
            )));
        }
        let value = self
            .runtime
            .get(name)
            .or_else(|| self.configured.get(name))
            .ok_or_else(|| {
                MudClientError::ConfigValidation(format!("variable `{name}` is not defined"))
            })?;
        stack.push(name.to_string());
        let expanded = self.expand_text(value, stack);
        stack.pop();
        expanded
    }

    fn expand_text(&self, template: &str, stack: &mut Vec<String>) -> Result<String> {
        let mut output = String::new();
        let mut chars = template.chars().peekable();
        while let Some(character) = chars.next() {
            if character != '$' {
                output.push(character);
                self.check_size(&output)?;
                continue;
            }
            if chars.peek() == Some(&'$') {
                chars.next();
                if chars.peek() == Some(&'{') {
                    output.push('$');
                } else {
                    output.push_str("$$");
                }
                self.check_size(&output)?;
                continue;
            }
            if chars.peek() != Some(&'{') {
                output.push('$');
                self.check_size(&output)?;
                continue;
            }
            chars.next();
            let mut name = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == '}' {
                    closed = true;
                    break;
                }
                name.push(next);
            }
            if !closed {
                return Err(MudClientError::ConfigValidation(
                    "unterminated variable reference".to_string(),
                ));
            }
            validate_name(&name)?;
            output.push_str(&self.resolve(&name, stack)?);
            self.check_size(&output)?;
        }
        Ok(output)
    }

    fn check_size(&self, value: &str) -> Result<()> {
        if value.len() > self.max_expanded_bytes {
            return Err(MudClientError::ConfigValidation(format!(
                "variable expansion exceeded maximum size of {} bytes",
                self.max_expanded_bytes
            )));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid_start = chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    if !valid_start || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(MudClientError::ConfigValidation(format!(
            "variable name `{name}` must start with an ASCII letter or underscore and contain only ASCII letters, digits, or underscores"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(values: &[(&str, &str)]) -> VariableStore {
        VariableStore::new(&VariableConfig {
            values: values
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            ..VariableConfig::default()
        })
        .expect("variables should validate")
    }

    #[test]
    fn expands_nested_values_and_escaped_references() {
        let variables = store(&[("target", "orc"), ("command", "kill ${target}")]);

        assert_eq!(
            variables.expand("${command};say $${target}").unwrap(),
            "kill orc;say ${target}"
        );
    }

    #[test]
    fn runtime_values_override_and_reveal_configured_values() {
        let configured = store(&[("target", "orc")]);
        let runtime = configured.with_runtime("target", "troll").unwrap();

        assert_eq!(runtime.expand("${target}").unwrap(), "troll");
        assert_eq!(
            runtime
                .without_runtime("target")
                .unwrap()
                .expand("${target}")
                .unwrap(),
            "orc"
        );
    }

    #[test]
    fn rejects_missing_recursive_and_invalid_variables() {
        assert!(store(&[]).expand("${missing}").is_err());
        assert!(
            VariableStore::new(&VariableConfig {
                values: BTreeMap::from([
                    ("one".to_string(), "${two}".to_string()),
                    ("two".to_string(), "${one}".to_string()),
                ]),
                ..VariableConfig::default()
            })
            .is_err()
        );
        assert!(store(&[]).with_runtime("not-valid", "value").is_err());
    }

    #[test]
    fn rejects_expansion_beyond_byte_limit() {
        let variables = VariableStore::new(&VariableConfig {
            max_expanded_bytes: 4,
            values: BTreeMap::from([("large".to_string(), "12345".to_string())]),
            ..VariableConfig::default()
        });

        assert!(variables.is_err());
    }
}
