use ratatui::{layout::Alignment, style::Color};

use crate::config::{PanelAlignment, PanelBorderStyle, PanelOptions, ThemeConfig};

pub use crate::color::parse_color;

// Snow stays neutral even when general theme roles use warm or saturated colors.
pub const SNOW_WHITE: Color = Color::White;
pub const SNOW_SOFT_WHITE: Color = Color::Gray;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub border_style: PanelBorderStyle,
    pub alignment: Alignment,
    pub foreground: Color,
    pub border: Color,
    pub title: Color,
    pub accent: Color,
    pub rain: Color,
    pub lightning: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub muted: Color,
    pub player: Color,
    pub enemy: Color,
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        Self {
            background: parse_color(&config.background).unwrap_or(Color::Reset),
            border_style: PanelBorderStyle::Plain,
            alignment: Alignment::Left,
            foreground: parse_color(&config.foreground).unwrap_or(Color::Gray),
            border: parse_color(&config.border).unwrap_or(Color::DarkGray),
            title: parse_color(&config.title).unwrap_or(Color::Yellow),
            accent: parse_color(&config.accent).unwrap_or(Color::Cyan),
            rain: parse_color(&config.rain).unwrap_or(Color::Cyan),
            lightning: parse_color(&config.lightning).unwrap_or(Color::LightYellow),
            success: parse_color(&config.success).unwrap_or(Color::Green),
            warning: parse_color(&config.warning).unwrap_or(Color::Yellow),
            danger: parse_color(&config.danger).unwrap_or(Color::Red),
            muted: parse_color(&config.muted).unwrap_or(Color::DarkGray),
            player: parse_color(&config.player).unwrap_or(Color::Cyan),
            enemy: parse_color(&config.enemy).unwrap_or(Color::Red),
        }
    }

    pub fn for_panel(&self, options: &PanelOptions) -> Self {
        let mut theme = *self;
        theme.border_style = options.border_style;
        theme.alignment = match options.alignment {
            PanelAlignment::Left => Alignment::Left,
            PanelAlignment::Center => Alignment::Center,
            PanelAlignment::Right => Alignment::Right,
        };
        for (key, value) in &options.theme {
            let target = match key.as_str() {
                "background" => &mut theme.background,
                "foreground" => &mut theme.foreground,
                "border" => &mut theme.border,
                "title" => &mut theme.title,
                "accent" => &mut theme.accent,
                "rain" => &mut theme.rain,
                "lightning" => &mut theme.lightning,
                "success" => &mut theme.success,
                "warning" => &mut theme.warning,
                "danger" => &mut theme.danger,
                "muted" => &mut theme.muted,
                "player" => &mut theme.player,
                "enemy" => &mut theme.enemy,
                _ => continue,
            };
            if let Some(color) = parse_color(value) {
                *target = color;
            }
        }
        theme
    }

    pub fn background_safe_foreground(&self) -> Color {
        Color::Black
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightning_defaults_to_bright_yellow_and_supports_panel_overrides() {
        let theme = Theme::from_config(&ThemeConfig::default());
        assert_eq!(theme.lightning, Color::Rgb(255, 255, 0));
        let mut options = PanelOptions::default();
        options
            .theme
            .insert("lightning".into(), "lightyellow".into());
        assert_eq!(theme.for_panel(&options).lightning, Color::LightYellow);
        assert_eq!(theme.lightning, Color::Rgb(255, 255, 0));
    }

    #[test]
    fn rain_color_parses_supported_formats_and_defaults() {
        assert_eq!(
            Theme::from_config(&ThemeConfig::default()).rain,
            Color::Rgb(97, 175, 239)
        );
        for (value, expected) in [
            ("blue", Color::Blue),
            ("index:123", Color::Indexed(123)),
            ("#123456", Color::Rgb(18, 52, 86)),
        ] {
            let config = ThemeConfig {
                rain: value.into(),
                ..ThemeConfig::default()
            };
            assert_eq!(Theme::from_config(&config).rain, expected);
        }
    }

    #[test]
    fn rain_panel_override_is_local_and_invalid_override_is_ignored() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut options = PanelOptions::default();
        options.theme.insert("rain".into(), "blue".into());
        let panel = theme.for_panel(&options);
        assert_eq!(panel.rain, Color::Blue);
        assert_eq!(panel.accent, theme.accent);
        assert_eq!(theme.rain, Color::Rgb(97, 175, 239));
        options.theme.insert("rain".into(), "invalid".into());
        assert_eq!(theme.for_panel(&options).rain, theme.rain);
    }
}
