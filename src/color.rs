use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnsiColors {
    pub foregrounds: Vec<AnsiColor>,
    pub backgrounds: Vec<AnsiColor>,
}

#[derive(Debug, Clone, Default)]
pub struct AnsiColorState {
    foreground: Option<AnsiColor>,
    background: Option<AnsiColor>,
}

impl AnsiColorState {
    pub fn inspect(&mut self, value: &str) -> AnsiColors {
        let mut colors = AnsiColors::default();
        let mut remaining = value;
        while let Some(index) = remaining.find("\x1b[") {
            let (plain, rest) = remaining.split_at(index);
            if !plain.is_empty() {
                colors.record(self.foreground, self.background);
            }
            let Some(end) = rest.find('m') else {
                break;
            };
            self.apply_sgr(&rest[2..end]);
            remaining = &rest[end + 1..];
        }
        if !remaining.is_empty() {
            colors.record(self.foreground, self.background);
        }
        colors
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn apply_sgr(&mut self, sequence: &str) {
        let Some(values) = parse_sgr_parameters(sequence) else {
            return;
        };
        let mut index = 0;
        while index < values.len() {
            match values[index] {
                0 => self.reset(),
                30..=37 => self.foreground = Some(ansi_color(values[index] - 30, false)),
                39 => self.foreground = None,
                40..=47 => self.background = Some(ansi_color(values[index] - 40, false)),
                49 => self.background = None,
                90..=97 => self.foreground = Some(ansi_color(values[index] - 90, true)),
                100..=107 => self.background = Some(ansi_color(values[index] - 100, true)),
                38 | 48 if values.get(index + 1) == Some(&5) => {
                    if let Some(color) = values
                        .get(index + 2)
                        .and_then(|value| u8::try_from(*value).ok())
                        .map(AnsiColor::Indexed)
                    {
                        if values[index] == 38 {
                            self.foreground = Some(color);
                        } else {
                            self.background = Some(color);
                        }
                        index += 2;
                    }
                }
                38 | 48 if values.get(index + 1) == Some(&2) => {
                    if let (Some(red), Some(green), Some(blue)) = (
                        values
                            .get(index + 2)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 3)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 4)
                            .and_then(|value| u8::try_from(*value).ok()),
                    ) {
                        let color = AnsiColor::Rgb(red, green, blue);
                        if values[index] == 38 {
                            self.foreground = Some(color);
                        } else {
                            self.background = Some(color);
                        }
                        index += 4;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}

pub fn parse_sgr_parameters(sequence: &str) -> Option<Vec<u16>> {
    sequence
        .split(';')
        .map(|part| {
            if part.is_empty() {
                Some(0)
            } else {
                part.parse::<u16>().ok()
            }
        })
        .collect()
}

impl AnsiColors {
    fn record(&mut self, foreground: Option<AnsiColor>, background: Option<AnsiColor>) {
        if let Some(color) = foreground
            && !self.foregrounds.contains(&color)
        {
            self.foregrounds.push(color);
        }
        if let Some(color) = background
            && !self.backgrounds.contains(&color)
        {
            self.backgrounds.push(color);
        }
    }
}

pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "white" => Some(Color::White),
        "darkgray" | "dark-gray" | "darkgrey" | "dark-grey" | "brightblack" | "bright-black" => {
            Some(Color::DarkGray)
        }
        "lightred" | "light-red" | "brightred" | "bright-red" => Some(Color::LightRed),
        "lightgreen" | "light-green" | "brightgreen" | "bright-green" => Some(Color::LightGreen),
        "lightyellow" | "light-yellow" | "brightyellow" | "bright-yellow" => {
            Some(Color::LightYellow)
        }
        "lightblue" | "light-blue" | "brightblue" | "bright-blue" => Some(Color::LightBlue),
        "lightmagenta" | "light-magenta" | "brightmagenta" | "bright-magenta" => {
            Some(Color::LightMagenta)
        }
        "lightcyan" | "light-cyan" | "brightcyan" | "bright-cyan" => Some(Color::LightCyan),
        "brightwhite" | "bright-white" => Some(Color::White),
        _ if value.to_ascii_lowercase().strip_prefix("index:").is_some() => value
            .to_ascii_lowercase()
            .strip_prefix("index:")?
            .parse::<u8>()
            .ok()
            .map(Color::Indexed),
        _ if value.len() == 7 && value.starts_with('#') => {
            let red = u8::from_str_radix(&value[1..3], 16).ok()?;
            let green = u8::from_str_radix(&value[3..5], 16).ok()?;
            let blue = u8::from_str_radix(&value[5..7], 16).ok()?;
            Some(Color::Rgb(red, green, blue))
        }

        _ => None,
    }
}

pub fn parse_ansi_color(value: &str) -> Option<AnsiColor> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(AnsiColor::Black),
        "red" => Some(AnsiColor::Red),
        "green" => Some(AnsiColor::Green),
        "yellow" => Some(AnsiColor::Yellow),
        "blue" => Some(AnsiColor::Blue),
        "magenta" => Some(AnsiColor::Magenta),
        "cyan" => Some(AnsiColor::Cyan),
        "gray" | "grey" | "white" => Some(AnsiColor::White),
        "darkgray" | "dark-gray" | "darkgrey" | "dark-grey" | "brightblack" | "bright-black" => {
            Some(AnsiColor::BrightBlack)
        }
        "lightred" | "light-red" | "brightred" | "bright-red" => Some(AnsiColor::BrightRed),
        "lightgreen" | "light-green" | "brightgreen" | "bright-green" => {
            Some(AnsiColor::BrightGreen)
        }
        "lightyellow" | "light-yellow" | "brightyellow" | "bright-yellow" => {
            Some(AnsiColor::BrightYellow)
        }
        "lightblue" | "light-blue" | "brightblue" | "bright-blue" => Some(AnsiColor::BrightBlue),
        "lightmagenta" | "light-magenta" | "brightmagenta" | "bright-magenta" => {
            Some(AnsiColor::BrightMagenta)
        }
        "lightcyan" | "light-cyan" | "brightcyan" | "bright-cyan" => Some(AnsiColor::BrightCyan),
        "brightwhite" | "bright-white" => Some(AnsiColor::BrightWhite),
        _ if value.to_ascii_lowercase().strip_prefix("index:").is_some() => value
            .to_ascii_lowercase()
            .strip_prefix("index:")?
            .parse::<u8>()
            .ok()
            .map(AnsiColor::Indexed),
        _ if value.len() == 7 && value.starts_with('#') => {
            let red = u8::from_str_radix(&value[1..3], 16).ok()?;
            let green = u8::from_str_radix(&value[3..5], 16).ok()?;
            let blue = u8::from_str_radix(&value[5..7], 16).ok()?;
            Some(AnsiColor::Rgb(red, green, blue))
        }
        _ => None,
    }
}

fn ansi_color(index: u16, bright: bool) -> AnsiColor {
    match (index, bright) {
        (0, false) => AnsiColor::Black,
        (1, false) => AnsiColor::Red,
        (2, false) => AnsiColor::Green,
        (3, false) => AnsiColor::Yellow,
        (4, false) => AnsiColor::Blue,
        (5, false) => AnsiColor::Magenta,
        (6, false) => AnsiColor::Cyan,
        (7, false) => AnsiColor::White,
        (0, true) => AnsiColor::BrightBlack,
        (1, true) => AnsiColor::BrightRed,
        (2, true) => AnsiColor::BrightGreen,
        (3, true) => AnsiColor::BrightYellow,
        (4, true) => AnsiColor::BrightBlue,
        (5, true) => AnsiColor::BrightMagenta,
        (6, true) => AnsiColor::BrightCyan,
        (7, true) => AnsiColor::BrightWhite,
        _ => AnsiColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ansi_bright_color_names() {
        assert_eq!(parse_color("darkgray"), Some(Color::DarkGray));
        assert_eq!(parse_color("bright-red"), Some(Color::LightRed));
        assert_eq!(parse_color("lightred"), Some(Color::LightRed));
        assert_eq!(parse_color("brightcyan"), Some(Color::LightCyan));
        assert_eq!(parse_color("index:123"), Some(Color::Indexed(123)));
    }

    #[test]
    fn tracks_foreground_background_and_inherited_colors() {
        let mut state = AnsiColorState::default();

        let first = state.inspect("\x1b[31;44mred on blue");
        assert_eq!(first.foregrounds, [AnsiColor::Red]);
        assert_eq!(first.backgrounds, [AnsiColor::Blue]);

        let inherited = state.inspect("still colored\x1b[0m");
        assert_eq!(inherited.foregrounds, [AnsiColor::Red]);
        assert_eq!(inherited.backgrounds, [AnsiColor::Blue]);
        assert_eq!(state.inspect("plain"), AnsiColors::default());
    }

    #[test]
    fn tracks_indexed_and_rgb_colors() {
        let mut state = AnsiColorState::default();
        let colors = state.inspect("\x1b[38;5;123;48;2;10;20;30mcolored\x1b[0m");

        assert_eq!(colors.foregrounds, [AnsiColor::Indexed(123)]);
        assert_eq!(colors.backgrounds, [AnsiColor::Rgb(10, 20, 30)]);
    }

    #[test]
    fn rejects_malformed_sgr_without_coercing_it() {
        let mut state = AnsiColorState::default();
        assert_eq!(
            state.inspect("\x1b[38;bogus;5;196mDanger"),
            AnsiColors::default()
        );

        let colors = state.inspect("\x1b[44mblue\x1b[;31mred");
        assert_eq!(colors.backgrounds, [AnsiColor::Blue]);
        assert_eq!(colors.foregrounds, [AnsiColor::Red]);
    }

    #[test]
    fn distinguishes_base_and_bright_ansi_white() {
        let mut state = AnsiColorState::default();
        assert_eq!(
            state.inspect("\x1b[37mwhite").foregrounds,
            [AnsiColor::White]
        );
        assert_eq!(parse_ansi_color("white"), Some(AnsiColor::White));
        assert_eq!(
            state.inspect("\x1b[97mbright").foregrounds,
            [AnsiColor::BrightWhite]
        );
        assert_eq!(
            parse_ansi_color("brightwhite"),
            Some(AnsiColor::BrightWhite)
        );
    }
}
