use crate::{state::OutputStyle, ui::theme::parse_color};

pub(super) fn is_recognized_local_command(text: &str) -> bool {
    let Some(command) = text.strip_prefix('/') else {
        return false;
    };
    matches!(
        command.split_whitespace().next(),
        Some(
            "help"
                | "msdp"
                | "echo"
                | "clear"
                | "quit"
                | "reload"
                | "reconnect"
                | "lua"
                | "alias"
                | "trigger"
                | "triggers"
                | "highlight"
                | "handler"
                | "variable"
                | "event"
                | "toggle"
                | "map"
                | "path"
        )
    )
}

pub(super) fn preserves_variable_templates(text: &str) -> bool {
    let command = text.trim_start().strip_prefix('/').unwrap_or_default();
    matches!(
        command.split_whitespace().next(),
        Some("alias" | "trigger" | "triggers" | "handler" | "variable")
    )
}

pub(super) fn parse_echo(input: &str) -> std::result::Result<(String, OutputStyle), String> {
    let mut input = input.trim_start();
    let mut style = OutputStyle::default();

    loop {
        let Some((token, remainder)) = split_first_token(input) else {
            return Err(echo_usage());
        };
        if token == "--" {
            input = remainder.trim_start();
            break;
        }
        let (target, value, next) = if let Some(value) = token.strip_prefix("--fg=") {
            ("foreground", value, remainder)
        } else if let Some(value) = token.strip_prefix("--bg=") {
            ("background", value, remainder)
        } else if token == "--fg" || token == "--bg" {
            let Some((value, next)) = split_first_token(remainder.trim_start()) else {
                return Err(format!("{token} requires a color\n{}", echo_usage()));
            };
            (
                if token == "--fg" {
                    "foreground"
                } else {
                    "background"
                },
                value,
                next,
            )
        } else if token.starts_with("--") {
            return Err(format!("unknown echo option `{token}`\n{}", echo_usage()));
        } else {
            break;
        };
        if value.is_empty() || parse_color(value).is_none() {
            return Err(format!(
                "invalid echo {target} color `{value}`; use a named color or #RRGGBB"
            ));
        }
        if target == "foreground" {
            style.foreground = Some(value.to_string());
        } else {
            style.background = Some(value.to_string());
        }
        input = next.trim_start();
    }

    let message = input.trim();
    if message.is_empty() {
        return Err(echo_usage());
    }
    Ok((message.to_string(), style))
}

pub(super) fn split_first_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    Some((&input[..end], &input[end..]))
}

const MAX_REPEAT_EXPANDED_COMMANDS: usize = 10_000;

pub(super) fn split_game_commands(input: &str) -> std::result::Result<Vec<String>, String> {
    if input.trim_start().starts_with('/') {
        return Ok(vec![input.trim().to_string()]);
    }
    expand_repeat_commands(input, MAX_REPEAT_EXPANDED_COMMANDS)
}

fn expand_repeat_commands(
    input: &str,
    max_commands: usize,
) -> std::result::Result<Vec<String>, String> {
    let mut expanded = Vec::new();
    for command in split_top_level_commands(input)? {
        expand_repeat_command(&command, max_commands, &mut expanded)?;
    }
    Ok(expanded)
}

fn split_top_level_commands(input: &str) -> std::result::Result<Vec<String>, String> {
    let mut commands = Vec::new();
    let mut cursor = 0usize;
    while cursor < input.len() {
        let segment = &input[cursor..];
        let leading_whitespace = segment
            .char_indices()
            .find(|(_, value)| !value.is_whitespace())
            .map(|(index, _)| index)
            .unwrap_or(segment.len());
        cursor += leading_whitespace;
        if cursor >= input.len() {
            break;
        }

        let command_end = if let Some(open_index) = repeat_body_open_index(&input[cursor..]) {
            let close_index = matching_brace_index(&input[cursor..], open_index)?;
            let rest_start = cursor + close_index + '}'.len_utf8();
            input[rest_start..]
                .find(';')
                .map(|index| rest_start + index)
                .unwrap_or(input.len())
        } else {
            input[cursor..]
                .find(';')
                .map(|index| cursor + index)
                .unwrap_or(input.len())
        };
        push_trimmed_command(input, cursor, command_end, &mut commands);
        cursor = command_end;
        if let Some(value) = input[cursor..].chars().next()
            && value == ';'
        {
            cursor += value.len_utf8();
        }
    }
    Ok(commands)
}

fn repeat_body_open_index(input: &str) -> Option<usize> {
    let remainder = input.strip_prefix('#')?;
    let digit_count = remainder
        .char_indices()
        .take_while(|(_, value)| value.is_ascii_digit())
        .map(|(index, value)| index + value.len_utf8())
        .last()?;
    let after_count = &remainder[digit_count..];
    let whitespace = after_count
        .char_indices()
        .find(|(_, value)| !value.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(after_count.len());
    let body_start = 1 + digit_count + whitespace;
    input[body_start..].starts_with('{').then_some(body_start)
}

fn matching_brace_index(input: &str, open_index: usize) -> std::result::Result<usize, String> {
    let Some(after_open) = input[open_index..].strip_prefix('{') else {
        return Err("repeat command must include a braced body".to_string());
    };
    let mut depth = 1usize;
    let mut escaped = false;
    for (index, value) in after_open.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match value {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open_index + '{'.len_utf8() + index);
                }
            }
            _ => {}
        }
    }
    Err("repeat command is missing closing `}`".to_string())
}

fn push_trimmed_command(input: &str, start: usize, end: usize, commands: &mut Vec<String>) {
    let command = input[start..end].trim();
    if !command.is_empty() {
        commands.push(command.to_string());
    }
}

fn expand_repeat_command(
    command: &str,
    max_commands: usize,
    expanded: &mut Vec<String>,
) -> std::result::Result<(), String> {
    let Some(remainder) = command.trim_start().strip_prefix('#') else {
        expanded.push(command.to_string());
        return Ok(());
    };
    let digit_count = remainder
        .char_indices()
        .take_while(|(_, value)| value.is_ascii_digit())
        .map(|(index, value)| index + value.len_utf8())
        .last()
        .unwrap_or(0);
    if digit_count == 0 {
        expanded.push(command.to_string());
        return Ok(());
    }
    let count = remainder[..digit_count]
        .parse::<usize>()
        .map_err(|_| "repeat count is too large".to_string())?;
    let body_input = remainder[digit_count..].trim_start();
    let Some((body, rest)) = braced_body(body_input)? else {
        return Err("usage: #<count> {command[;command...]}".to_string());
    };
    if !rest.trim().is_empty() {
        return Err("repeat command must end after the closing `}`".to_string());
    }
    let commands = expand_repeat_commands(&body, max_commands)?;
    if commands.is_empty() {
        return Err("repeat command body must not be empty".to_string());
    }
    let added = count
        .checked_mul(commands.len())
        .and_then(|value| value.checked_add(expanded.len()))
        .ok_or_else(|| "repeat command expansion is too large".to_string())?;
    if added > max_commands {
        return Err(format!(
            "repeat command expansion exceeded maximum command count of {max_commands}"
        ));
    }
    for _ in 0..count {
        expanded.extend(commands.iter().cloned());
    }
    Ok(())
}

fn braced_body(input: &str) -> std::result::Result<Option<(String, &str)>, String> {
    if !input.starts_with('{') {
        return Ok(None);
    }
    let close_index = matching_brace_index(input, 0)?;
    Ok(Some((
        input['{'.len_utf8()..close_index].to_string(),
        &input[close_index + '}'.len_utf8()..],
    )))
}

fn echo_usage() -> String {
    "usage: /echo [--fg <color>] [--bg <color>] <text>".to_string()
}

#[cfg(test)]
mod tests {
    use super::split_game_commands;

    #[test]
    fn repeat_command_expands_braced_body() {
        assert_eq!(
            split_game_commands("#3 {look}").unwrap(),
            vec!["look", "look", "look"]
        );
    }

    #[test]
    fn repeat_command_repeats_only_its_braced_semicolon_group() {
        assert_eq!(
            split_game_commands("#2 {look;score};rest").unwrap(),
            vec!["look", "score", "look", "score", "rest"]
        );
    }

    #[test]
    fn repeat_command_can_be_nested() {
        assert_eq!(
            split_game_commands("#2 {#2 {look};score}").unwrap(),
            vec!["look", "look", "score", "look", "look", "score"]
        );
    }

    #[test]
    fn non_numeric_hash_commands_are_sent_normally() {
        assert_eq!(
            split_game_commands("#action thing").unwrap(),
            vec!["#action thing"]
        );
    }

    #[test]
    fn repeat_command_requires_braced_body() {
        assert_eq!(
            split_game_commands("#2 look").unwrap_err(),
            "usage: #<count> {command[;command...]}"
        );
    }

    #[test]
    fn non_repeat_braces_keep_original_semicolon_splitting() {
        assert_eq!(
            split_game_commands("say {a;b}").unwrap(),
            vec!["say {a", "b}"]
        );
    }
}
