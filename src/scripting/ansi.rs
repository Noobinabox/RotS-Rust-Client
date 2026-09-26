//! Compact SGR continuation state shared by source and display processing.

use crate::color::parse_sgr_parameters;

#[derive(Debug, Clone, Default)]
pub struct AnsiStyleState {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    foreground: Option<String>,
    background: Option<String>,
}

impl AnsiStyleState {
    pub fn consume(&mut self, raw: &str) {
        let mut remaining = raw;
        while let Some(start) = remaining.find("\x1b[") {
            remaining = &remaining[start + 2..];
            let Some(end) = remaining.find(|character: char| ('@'..='~').contains(&character))
            else {
                break;
            };
            if remaining.as_bytes()[end] == b'm'
                && let Some(values) = parse_sgr_parameters(&remaining[..end])
            {
                self.apply(&values);
            }
            remaining = &remaining[end + 1..];
        }
    }

    /// A complete style prefix, including reset, independent of earlier output.
    pub fn prefix(&self) -> String {
        let mut parts = vec!["0"];
        if self.bold {
            parts.push("1");
        }
        if self.dim {
            parts.push("2");
        }
        if self.italic {
            parts.push("3");
        }
        if self.underline {
            parts.push("4");
        }
        if let Some(value) = &self.foreground {
            parts.push(value);
        }
        if let Some(value) = &self.background {
            parts.push(value);
        }
        format!("\x1b[{}m", parts.join(";"))
    }

    fn apply(&mut self, values: &[u16]) {
        let mut index = 0;
        while index < values.len() {
            match values[index] {
                0 => *self = Self::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                30..=37 | 90..=97 => self.foreground = Some(values[index].to_string()),
                40..=47 | 100..=107 => self.background = Some(values[index].to_string()),
                39 => self.foreground = None,
                49 => self.background = None,
                38 | 48 => {
                    let components = match values.get(index + 1) {
                        Some(5) => 1,
                        Some(2) => 3,
                        _ => {
                            index += 1;
                            continue;
                        }
                    };
                    if let Some(colors) = values.get(index + 2..index + 2 + components)
                        && colors.iter().all(|value| *value <= 255)
                    {
                        let sequence = values[index..index + 2 + components]
                            .iter()
                            .map(u16::to_string)
                            .collect::<Vec<_>>()
                            .join(";");
                        if values[index] == 38 {
                            self.foreground = Some(sequence);
                        } else {
                            self.background = Some(sequence);
                        }
                        index += 1 + components;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_resets_attributes_and_extended_colors() {
        let mut state = AnsiStyleState::default();
        state.consume("\x1b[1;2;3;4;38;2;12;34;56;48;5;17mtext");
        assert_eq!(state.prefix(), "\x1b[0;1;2;3;4;38;2;12;34;56;48;5;17m");
        state.consume("\x1b[22;23;24;39m");
        assert_eq!(state.prefix(), "\x1b[0;48;5;17m");
        state.consume("\x1b[m");
        assert_eq!(state.prefix(), "\x1b[0m");
    }

    #[test]
    fn ignores_non_sgr_and_malformed_parameters() {
        let mut state = AnsiStyleState::default();
        state.consume("\x1b[31mred\x1b[2J\x1b[bogusm");
        assert_eq!(state.prefix(), "\x1b[0;31m");
    }
}
