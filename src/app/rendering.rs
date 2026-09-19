use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Widget},
};

use crate::{
    config::{AppConfig, PaneRole},
    state::AppState,
    ui::{
        character::{
            ResponsivePanelConfig, compact_status, render_classic_sidebar, render_details,
            render_info, render_status_dashboard,
        },
        input::render_input,
        layout::{LayoutMinimums, LayoutOverrides, MIN_CENTER_WIDTH, UiMode, resolve_layout},
        map::render_map,
        output::render_output,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame, config: &AppConfig, state: &AppState, theme: &Theme) {
    render_with_overrides(
        frame,
        config,
        state,
        theme,
        LayoutOverrides {
            left_width: None,
            right_width: None,
        },
    );
}

pub(super) fn layout_minimums(config: &AppConfig) -> LayoutMinimums {
    LayoutMinimums {
        map_width: config.panels.map.min_width,
        map_height: config.panels.map.min_height,
        output_width: config.panels.output.min_width,
        output_height: config.panels.output.min_height,
        input_height: config.panels.input.min_height,
    }
}

pub(super) fn pane_min_width(role: PaneRole, config: &AppConfig) -> u16 {
    match role {
        PaneRole::Map | PaneRole::ClassicSidebar => config.panels.map.min_width,
        PaneRole::Info => config.panels.info.min_width,
        PaneRole::Details | PaneRole::StatusDashboard => MIN_CENTER_WIDTH,
        PaneRole::None => 0,
    }
}

pub(super) fn render_with_overrides(
    frame: &mut Frame,
    config: &AppConfig,
    state: &AppState,
    theme: &Theme,
    overrides: LayoutOverrides,
) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let layout = resolve_layout(area, &config.layout, layout_minimums(config), overrides);
    let classic_sidebar = layout.mode == UiMode::FullHd;
    if !classic_sidebar && layout.map.width > 0 && layout.map.height > 0 {
        render_map(
            layout.map,
            frame.buffer_mut(),
            &state.map,
            theme,
            &config.map,
            &config.panels.map.title,
        );
    }
    if layout.output.width > 0 && layout.output.height > 0 {
        render_output(
            layout.output,
            frame.buffer_mut(),
            state,
            theme,
            &config.panels.output.title,
        );
    }
    if let Some(status_area) = layout.compact_status {
        Paragraph::new(compact_status(state, theme)).render(status_area, frame.buffer_mut());
    }
    for pane in [layout.top, layout.left, layout.right]
        .into_iter()
        .flatten()
        .filter(|pane| pane.role != PaneRole::Map)
    {
        render_assigned_pane(
            frame,
            config,
            state,
            theme,
            pane.area,
            pane.role,
            layout.mode,
        );
    }
    if layout.input.width > 0 && layout.input.height > 0 {
        render_input(
            layout.input,
            frame.buffer_mut(),
            state,
            theme,
            &config.panels.input.title,
        );
    }
    place_cursor(frame, layout.input, input_cursor(state));
}

fn render_assigned_pane(
    frame: &mut Frame,
    config: &AppConfig,
    state: &AppState,
    theme: &Theme,
    area: Rect,
    role: PaneRole,
    mode: UiMode,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match role {
        PaneRole::Map | PaneRole::None => {}
        PaneRole::StatusDashboard => render_status_dashboard(
            area,
            frame.buffer_mut(),
            state,
            theme,
            ResponsivePanelConfig::new(
                &config.gauges,
                &config.layout,
                &config.map,
                &config.panels,
                &config.weather,
                mode,
                mode == UiMode::Ultrawide,
            ),
        ),
        PaneRole::Details => render_details(
            area,
            frame.buffer_mut(),
            state,
            theme,
            ResponsivePanelConfig::new(
                &config.gauges,
                &config.layout,
                &config.map,
                &config.panels,
                &config.weather,
                mode,
                mode == UiMode::Ultrawide,
            ),
        ),
        PaneRole::Info => render_info(
            area,
            frame.buffer_mut(),
            state,
            theme,
            &config.panels,
            &config.weather,
        ),
        PaneRole::ClassicSidebar => render_classic_sidebar(
            area,
            frame.buffer_mut(),
            state,
            theme,
            ResponsivePanelConfig::new(
                &config.gauges,
                &config.layout,
                &config.map,
                &config.panels,
                &config.weather,
                mode,
                false,
            ),
        ),
    }
}

fn input_cursor(state: &AppState) -> usize {
    if state.output_view.search_active {
        state.output_view.search_cursor.saturating_add(1)
    } else if state.submitted_input_selected {
        state.submitted_input.as_ref().map_or(0, String::len)
    } else {
        state.cursor
    }
}

fn place_cursor(frame: &mut Frame, area: Rect, cursor: usize) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let cursor_x = area.x.saturating_add(1).saturating_add(cursor as u16);
    let cursor_y = area.y.saturating_add(1);
    frame.set_cursor_position((cursor_x.min(area.right().saturating_sub(2)), cursor_y));
}
