use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::ui::layout::{Divider, ResolvedLayout};

pub(super) fn mouse_over_output(mouse: MouseEvent, layout: &ResolvedLayout) -> bool {
    rect_contains(layout.output, mouse.column, mouse.row)
}

pub(super) fn divider_at(mouse: MouseEvent, layout: &ResolvedLayout) -> Option<Divider> {
    if layout
        .left_divider
        .is_some_and(|area| rect_contains(area, mouse.column, mouse.row))
    {
        return Some(Divider::Left);
    }
    layout
        .right_divider
        .filter(|area| rect_contains(*area, mouse.column, mouse.row))
        .map(|_| Divider::Right)
}

pub(super) fn rect_contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}
