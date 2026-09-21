use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    pub forward: String,
    pub backward: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathState {
    pub steps: Vec<PathStep>,
    pub position: usize,
    pub mapping: bool,
    pub mapping_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathResult {
    pub message: String,
    pub commands: Vec<String>,
    pub show_map: bool,
    pub variable: Option<(String, String)>,
    pub file: Option<(String, String)>,
}

impl PathState {
    pub fn execute(
        &mut self,
        input: &str,
        variables: &HashMap<String, String>,
    ) -> Result<PathResult, String> {
        let fields = parse_fields(input)?;
        let command = fields.first().map(String::as_str).unwrap_or("");
        let option = |name: &str| {
            let names = [
                "create", "destroy", "start", "stop", "describe", "get", "insert", "delete",
                "goto", "move", "walk", "run", "swap", "zip", "unzip", "map", "save", "load",
                "undo",
            ];
            let command = command.to_ascii_lowercase();
            command.eq_ignore_ascii_case(name)
                || (name.starts_with(&command)
                    && names
                        .iter()
                        .filter(|candidate| candidate.starts_with(&command))
                        .count()
                        == 1)
        };
        match command {
            "" | "help" => Ok(self.help()),
            _ if command.eq_ignore_ascii_case("mapping") => match fields.get(1).map(String::as_str)
            {
                Some("stop") => {
                    self.mapping = false;
                    Ok(self.message("Path mapping stopped."))
                }
                Some("save") => {
                    let name = self
                        .mapping_name
                        .clone()
                        .ok_or("usage: /path mapping <name> before saving")?;
                    Ok(PathResult {
                        message: format!("Path mapping `{name}` saved."),
                        file: Some((name, self.serialize("both")?)),
                        ..PathResult::default()
                    })
                }
                Some(name) if !name.is_empty() => {
                    self.steps.clear();
                    self.position = 0;
                    self.mapping = true;
                    self.mapping_name = Some(name.to_string());
                    Ok(self.message(format!("Path mapping `{name}` started.")))
                }
                _ => Err("usage: /path mapping <name|stop|save>".to_string()),
            },
            _ if option("create") => {
                self.steps.clear();
                self.position = 0;
                self.mapping = true;
                Ok(self.message("Path created; movement mapping started."))
            }
            _ if option("destroy") => {
                self.steps.clear();
                self.position = 0;
                self.mapping = false;
                Ok(self.message("Path destroyed."))
            }
            _ if option("start") => {
                self.mapping = true;
                Ok(self.message("Path movement mapping started."))
            }
            _ if option("stop") => {
                self.mapping = false;
                Ok(self.message("Path movement mapping stopped."))
            }
            _ if option("describe") => Ok(self.message(self.describe())),
            _ if option("get") => {
                let value = match fields.get(1).map(String::as_str) {
                    Some(value) if value.eq_ignore_ascii_case("position") => {
                        self.position.to_string()
                    }
                    Some(value) if value.eq_ignore_ascii_case("length") => {
                        self.steps.len().to_string()
                    }
                    _ => return Err("usage: /path get <length|position>".to_string()),
                };
                Ok(self.message(value))
            }
            _ if option("insert") => {
                let forward = fields
                    .get(1)
                    .cloned()
                    .ok_or("usage: /path insert <forward> [backward]")?;
                let backward = fields.get(2).cloned().unwrap_or_default();
                self.steps
                    .insert(self.position, PathStep { forward, backward });
                self.position += 1;
                Ok(self.message("Path step inserted."))
            }
            _ if option("delete") => {
                if self.steps.pop().is_none() {
                    return Err("Path is empty.".to_string());
                }
                self.position = self.position.min(self.steps.len());
                Ok(self.message("Last path step deleted."))
            }
            _ if option("undo") => {
                if self.steps.pop().is_none() {
                    return Err("Path is empty.".to_string());
                }
                self.position = self.position.min(self.steps.len());
                Ok(self.message("Last path step undone."))
            }
            _ if option("goto") => {
                let target = fields
                    .get(1)
                    .map(String::as_str)
                    .ok_or("usage: /path goto <start|end|position>")?;
                self.position = if target.eq_ignore_ascii_case("start") {
                    0
                } else if target.eq_ignore_ascii_case("end") {
                    self.steps.len()
                } else {
                    target
                        .parse::<usize>()
                        .map_err(|_| "path position must be a number")?
                        .min(self.steps.len())
                };
                Ok(self.message(format!("Path position set to {}.", self.position)))
            }
            _ if option("move") => {
                let (direction, amount) = match fields.get(1).map(String::as_str) {
                    Some("forward") => (1isize, fields.get(2).map(String::as_str).unwrap_or("1")),
                    Some("backward") | Some("backwards") => {
                        (-1isize, fields.get(2).map(String::as_str).unwrap_or("1"))
                    }
                    Some(value) => (1isize, value),
                    None => (1isize, "1"),
                };
                let delta = amount
                    .parse::<isize>()
                    .map_err(|_| "usage: /path move <number>".to_string())?;
                self.position = (self.position as isize + direction * delta)
                    .clamp(0, self.steps.len() as isize) as usize;
                Ok(self.message(format!("Path position set to {}.", self.position)))
            }
            _ if option("walk") => {
                let backward = fields.get(1).is_some_and(|value| {
                    value.eq_ignore_ascii_case("backward")
                        || value.eq_ignore_ascii_case("backwards")
                });
                if backward {
                    if self.position == 0 {
                        return Err("Already at the start of the path.".to_string());
                    }
                    let step = &self.steps[self.position - 1];
                    self.position -= 1;
                    if step.backward.is_empty() {
                        return Err("The previous path step has no backward command.".to_string());
                    }
                    Ok(PathResult {
                        message: format!("Walking backward: {}", step.backward),
                        commands: vec![step.backward.clone()],
                        ..PathResult::default()
                    })
                } else {
                    if self.position >= self.steps.len() {
                        return Err("Already at the end of the path.".to_string());
                    }
                    let step = &self.steps[self.position];
                    self.position += 1;
                    Ok(PathResult {
                        message: format!("Walking forward: {}", step.forward),
                        commands: vec![step.forward.clone()],
                        ..PathResult::default()
                    })
                }
            }
            _ if option("run") => {
                let commands = self.steps[self.position..]
                    .iter()
                    .map(|step| step.forward.clone())
                    .collect::<Vec<_>>();
                self.position = self.steps.len();
                Ok(PathResult {
                    message: format!(
                        "Running {} path step{}.",
                        commands.len(),
                        if commands.len() == 1 { "" } else { "s" }
                    ),
                    commands,
                    ..PathResult::default()
                })
            }
            _ if option("swap") => {
                self.steps.reverse();
                for step in &mut self.steps {
                    std::mem::swap(&mut step.forward, &mut step.backward);
                }
                self.position = self.steps.len().saturating_sub(self.position);
                Ok(self.message("Path direction swapped."))
            }
            _ if option("zip") => Ok(self.message(self.zip())),
            _ if option("unzip") => {
                let value = fields.get(1).ok_or("usage: /path unzip <speedwalk>")?;
                self.steps = unzip(value)?;
                self.position = 0;
                Ok(self.message(format!(
                    "Path with {} step{} loaded.",
                    self.steps.len(),
                    if self.steps.len() == 1 { "" } else { "s" }
                )))
            }
            _ if option("map") => Ok(PathResult {
                show_map: true,
                ..PathResult::default()
            }),
            _ if option("save") => {
                let variable = fields
                    .get(2)
                    .cloned()
                    .ok_or("usage: /path save <forward|backward|both> <variable>")?;
                let direction = fields.get(1).map(String::as_str).unwrap_or("forward");
                let value = self.serialize(direction)?;
                Ok(PathResult {
                    message: format!("Path saved to {}.", variable),
                    variable: Some((variable, value)),
                    ..PathResult::default()
                })
            }
            _ if option("load") => {
                let variable = fields.get(1).ok_or("usage: /path load <variable>")?;
                let value = variables
                    .get(variable)
                    .ok_or_else(|| format!("Variable `{variable}` is not defined."))?;
                self.steps = value
                    .split(';')
                    .filter(|value| !value.is_empty())
                    .map(|value| PathStep {
                        forward: value.to_string(),
                        backward: reverse_command(value),
                    })
                    .collect();
                self.position = 0;
                self.mapping = false;
                Ok(self.message(format!(
                    "Path with {} step{} loaded.",
                    self.steps.len(),
                    if self.steps.len() == 1 { "" } else { "s" }
                )))
            }
            _ => Err(format!("Unknown path command `{command}`. Try /path help.")),
        }
    }

    pub fn record(&mut self, command: &str) {
        if self.mapping && is_direction(command) {
            self.steps.truncate(self.position);
            self.steps.push(PathStep {
                forward: command.to_string(),
                backward: reverse_command(command),
            });
            self.position = self.steps.len();
        }
    }

    fn message(&self, message: impl Into<String>) -> PathResult {
        PathResult {
            message: message.into(),
            ..PathResult::default()
        }
    }
    fn help(&self) -> PathResult {
        self.message("# Path Commands\n\n/path mapping <name>\n/path mapping stop\n/path mapping save\n/path create|destroy|start|stop\n/path insert <forward> [backward]\n/path delete\n/path describe\n/path get length|position\n/path goto start|end|<position>\n/path move <number>\n/path walk [forward|backward]\n/path run\n/path swap\n/path zip|unzip <speedwalk>\n/path map\n/path save <forward|backward|both> <variable>\n/path load <variable>")
    }
    fn describe(&self) -> String {
        format!(
            "Path has {} step{}; position {} of {}{}.",
            self.steps.len(),
            if self.steps.len() == 1 { "" } else { "s" },
            self.position,
            self.steps.len(),
            if self.mapping {
                "; mapping is active"
            } else {
                ""
            }
        )
    }
    fn zip(&self) -> String {
        let mut result = String::new();
        let mut index = 0;
        while index < self.steps.len() {
            let command = &self.steps[index].forward;
            if !is_direction(command) {
                if !result.is_empty() {
                    result.push(';');
                }
                result.push_str(command);
                index += 1;
                continue;
            }
            let mut count = 1;
            while index + count < self.steps.len() && self.steps[index + count].forward == *command
            {
                count += 1;
            }
            if count > 1 {
                result.push_str(&count.to_string());
            }
            result.push_str(command);
            index += count;
        }
        result
    }
    fn serialize(&self, direction: &str) -> Result<String, String> {
        match direction.to_ascii_lowercase().as_str() {
            "forward" => Ok(self
                .steps
                .iter()
                .map(|step| step.forward.as_str())
                .collect::<Vec<_>>()
                .join(";")),
            "backward" => Ok(self
                .steps
                .iter()
                .rev()
                .map(|step| {
                    if step.backward.is_empty() {
                        step.forward.as_str()
                    } else {
                        step.backward.as_str()
                    }
                })
                .collect::<Vec<_>>()
                .join(";")),
            "both" => Ok(self.zip()),
            _ => Err("usage: /path save <forward|backward|both> <variable>".to_string()),
        }
    }
}

fn parse_fields(input: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.trim().chars() {
        match character {
            '{' if !quoted => quoted = true,
            '}' if quoted => quoted = false,
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    fields.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if quoted {
        return Err("unclosed path argument".to_string());
    }
    if !current.is_empty() {
        fields.push(current);
    }
    Ok(fields)
}

fn is_direction(command: &str) -> bool {
    matches!(
        command.to_ascii_lowercase().as_str(),
        "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw" | "u" | "d"
    )
}
fn reverse_command(command: &str) -> String {
    match command.to_ascii_lowercase().as_str() {
        "n" => "s",
        "ne" => "sw",
        "e" => "w",
        "se" => "nw",
        "s" => "n",
        "sw" => "ne",
        "w" => "e",
        "nw" => "se",
        "u" => "d",
        "d" => "u",
        _ => command,
    }
    .to_string()
}
fn unzip(value: &str) -> Result<Vec<PathStep>, String> {
    let mut steps = Vec::new();
    let mut count = 0usize;
    let characters = value.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < characters.len() {
        if characters[index].is_ascii_digit() {
            count = count * 10 + characters[index].to_digit(10).unwrap_or(0) as usize;
            index += 1;
            continue;
        }
        let mut direction = characters[index].to_string();
        if index + 1 < characters.len() {
            let compound = format!("{}{}", characters[index], characters[index + 1]);
            if is_direction(&compound) {
                direction = compound;
                index += 1;
            }
        }
        if !is_direction(&direction) {
            return Err(format!("invalid speedwalk direction `{direction}`"));
        }
        for _ in 0..count.max(1) {
            steps.push(PathStep {
                forward: direction.clone(),
                backward: reverse_command(&direction),
            });
        }
        count = 0;
        index += 1;
    }
    if count > 0 {
        return Err("speedwalk ends with a repeat count".to_string());
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_runs_a_path() {
        let mut path = PathState::default();
        path.execute("create", &HashMap::new()).unwrap();
        path.record("n");
        path.record("e");
        assert_eq!(path.steps[0].backward, "s");
        path.execute("goto start", &HashMap::new()).unwrap();
        let result = path.execute("run", &HashMap::new()).unwrap();
        assert_eq!(result.commands, ["n", "e"]);
        assert_eq!(path.position, 2);
    }

    #[test]
    fn zip_and_unzip_preserve_direction_steps() {
        let mut path = PathState::default();
        path.execute("unzip 3n2e", &HashMap::new()).unwrap();
        assert_eq!(path.steps.len(), 5);
        assert_eq!(
            path.execute("zip", &HashMap::new()).unwrap().message,
            "3n2e"
        );
    }

    #[test]
    fn ambiguous_abbreviation_is_rejected() {
        let mut path = PathState::default();
        let error = path.execute("m", &HashMap::new()).unwrap_err();
        assert!(error.contains("Unknown path command"));
    }
}
