use ratatui::style::Color;

use crate::config::ThemeConfig;

pub use crate::color::parse_color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
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

    pub fn background_safe_foreground(&self) -> Color {
        Color::Black
    }
}
