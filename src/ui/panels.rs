use ratatui::{
    style::Style,
    widgets::{Block, Borders},
};

use crate::ui::theme::Theme;

pub fn panel<'a>(title: &'a str, theme: &Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.border))
        .title_style(Style::new().fg(theme.title))
}
