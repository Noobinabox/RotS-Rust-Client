use ratatui::{layout::Alignment, style::Color};

use crate::config::{PanelAlignment, PanelBorderStyle, PanelOptions, ThemeConfig};

pub use crate::color::parse_color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub border_style: PanelBorderStyle,
    pub alignment: Alignment,
    pub foreground: Color,
    pub border: Color,
    pub title: Color,
    pub accent: Color,
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
