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
        panels::{PanelCache, PanelId, PanelRenderer},
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
            stacked_map_height: None,
        },
        None,
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
        PaneRole::Social => config.panels.social.min_width,
        PaneRole::NearbyMap => config.panels.map.min_width,
        PaneRole::Opponent => config.panels.opponent.min_width,
        PaneRole::Group => config.panels.group.min_width,
        PaneRole::Character => config.panels.character.min_width,
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
    cache: Option<&PanelCache>,
) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let layout = resolve_layout(area, &config.layout, layout_minimums(config), overrides);
    if let Some(cache) = cache {
        cache.begin_frame();
    }
    let renderer = PanelRenderer { theme, cache };
    let classic_sidebar = layout.mode == UiMode::FullHd;
    if !classic_sidebar && layout.map.width > 0 && layout.map.height > 0 {
        renderer.render(
            PanelId::Map,
            &config.panels.map,
            layout.map,
            frame.buffer_mut(),
            |buf, theme| {
                render_map(
                    layout.map,
                    buf,
                    &state.map,
                    theme,
                    &config.map,
                    &config.panels.map.title,
                )
            },
        );
    }
    if layout.output.width > 0 && layout.output.height > 0 {
        renderer.render(
            PanelId::Output,
            &config.panels.output,
            layout.output,
            frame.buffer_mut(),
            |buf, theme| {
                render_output(
                    layout.output,
                    buf,
                    state,
                    theme,
                    &config.panels.output.title,
                )
            },
        );
    }
    if let Some(status_area) = layout.compact_status {
        Paragraph::new(compact_status(state, theme))
            .style(ratatui::style::Style::new().bg(theme.background))
            .render(status_area, frame.buffer_mut());
    }
    for pane in [layout.top, layout.left, layout.right]
        .into_iter()
        .flatten()
    {
        render_assigned_pane(
            frame,
            config,
            state,
            &renderer,
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
            &theme.for_panel(&config.panels.input),
            &config.panels.input.title,
        );
    }
    place_cursor(
        frame,
        layout.input,
        state,
        &theme.for_panel(&config.panels.input),
    );
    if let Some(cache) = cache {
        cache.end_frame();
    }
}

fn render_assigned_pane(
    frame: &mut Frame,
    config: &AppConfig,
    state: &AppState,
    renderer: &PanelRenderer<'_>,
    area: Rect,
    role: PaneRole,
    mode: UiMode,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = renderer.theme;
    match role {
        PaneRole::Map => renderer.render(
            PanelId::Map,
            &config.panels.map,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                render_map(
                    area,
                    buf,
                    &state.map,
                    theme,
                    &config.map,
                    &config.panels.map.title,
                )
            },
        ),
        PaneRole::None => {}
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
            )
            .with_cache(renderer.cache),
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
            )
            .with_cache(renderer.cache),
        ),
        PaneRole::Info => renderer.render(
            PanelId::Info,
            &config.panels.info,
            area,
            frame.buffer_mut(),
            |buf, theme| render_info(area, buf, state, theme, &config.panels, &config.weather),
        ),
        PaneRole::Social => renderer.render(
            PanelId::Social,
            &config.panels.social,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                crate::ui::character::render_social_panel(
                    area,
                    buf,
                    state,
                    theme,
                    &config.panels.social.title,
                )
            },
        ),
        PaneRole::NearbyMap => renderer.render(
            PanelId::NearbyMap,
            &config.panels.map,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                crate::ui::map::render_nearby_map(
                    area,
                    buf,
                    &state.map,
                    theme,
                    &config.map,
                    "Nearby Map",
                )
            },
        ),
        PaneRole::Opponent => renderer.render(
            PanelId::Opponent,
            &config.panels.opponent,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                crate::ui::character::render_opponent_panel(
                    area,
                    buf,
                    state,
                    theme,
                    &config.gauges,
                    &config.panels.opponent.title,
                )
            },
        ),
        PaneRole::Group => renderer.render(
            PanelId::Group,
            &config.panels.group,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                crate::ui::character::render_group_panel(
                    area,
                    buf,
                    state,
                    theme,
                    &config.gauges,
                    &config.panels.group.title,
                )
            },
        ),
        PaneRole::Character => renderer.render(
            PanelId::Character,
            &config.panels.character,
            area,
            frame.buffer_mut(),
            |buf, theme| {
                crate::ui::character::render_character_panel(
                    area,
                    buf,
                    state,
                    theme,
                    &config.gauges,
                    &config.panels.character.title,
                )
            },
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
            )
            .with_cache(renderer.cache),
            &config.layout.full_hd.sidebar_panels,
        ),
    }
}

fn place_cursor(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if let Some(position) = crate::ui::input::input_cursor_position(area, state, theme) {
        frame.set_cursor_position(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::App,
        config::{PanelAlignment, PanelBorderStyle},
        state::OutputCategory,
    };
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    #[test]
    fn tablet_compact_status_paints_the_theme_background() {
        let config = AppConfig::default();
        let app = App::new(config.clone());
        let theme = Theme::from_config(&config.colors);
        let area = Rect::new(0, 0, 100, 30);
        let layout = resolve_layout(
            area,
            &config.layout,
            layout_minimums(&config),
            LayoutOverrides::default(),
        );
        let status_area = layout.compact_status.expect("tablet status area");
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

        terminal
            .draw(|frame| {
                render_with_overrides(
                    frame,
                    &config,
                    &app.state,
                    &theme,
                    LayoutOverrides::default(),
                    None,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for y in status_area.top()..status_area.bottom() {
            for x in status_area.left()..status_area.right() {
                assert_eq!(buffer[(x, y)].bg, theme.background);
            }
        }
    }

    #[test]
    fn customized_panels_render_in_all_profiles_and_tiny_areas() {
        for (width, height) in [
            (1, 1),
            (2, 2),
            (20, 8),
            (70, 24),
            (100, 35),
            (160, 45),
            (240, 60),
        ] {
            for border in [
                PanelBorderStyle::Plain,
                PanelBorderStyle::Rounded,
                PanelBorderStyle::Double,
                PanelBorderStyle::Thick,
                PanelBorderStyle::None,
            ] {
                let mut config = AppConfig::default();
                config.layout.show_group = true;
                for options in [
                    &mut config.panels.info,
                    &mut config.panels.social,
                    &mut config.panels.map,
                    &mut config.panels.opponent,
                    &mut config.panels.group,
                    &mut config.panels.character,
                    &mut config.panels.output,
                    &mut config.panels.input,
                ] {
                    options.border_style = border;
                    options.alignment = PanelAlignment::Right;
                    options.theme.insert("border".into(), "red".into());
                    options.theme.insert("background".into(), "blue".into());
                }
                config.panels.map.refresh_ms = 1000;
                config.panels.output.refresh_ms = 1000;
                let mut app = App::new(config);
                app.state.push_output("hello", OutputCategory::Normal);
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                for _ in 0..2 {
                    terminal.draw(|frame| app.render(frame)).unwrap();
                }
                let area = resolve_layout(
                    Rect::new(0, 0, width, height),
                    &app.config.layout,
                    layout_minimums(&app.config),
                    LayoutOverrides::default(),
                )
                .output;
                if area.width >= 7 && area.height >= 3 {
                    let buf = terminal.backend().buffer();
                    assert_eq!(buf[(area.right() - 2, area.y + 1)].symbol(), "o");
                    assert_eq!(buf[(area.x + 1, area.y + 1)].bg, Color::Blue);
                    if border != PanelBorderStyle::None {
                        assert_eq!(buf[(area.x, area.y + 1)].fg, Color::Red);
                    }
                }
            }
        }
    }

    #[test]
    fn aligned_input_cursor_uses_unicode_display_width() {
        for alignment in [
            PanelAlignment::Left,
            PanelAlignment::Center,
            PanelAlignment::Right,
        ] {
            let mut config = AppConfig::default();
            config.panels.input.alignment = alignment;
            config.panels.input.border_style = PanelBorderStyle::None;
            let mut app = App::new(config);
            app.state.input = "é界!".into();
            app.state.cursor = "é".len();
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let input = resolve_layout(
                Rect::new(0, 0, 80, 24),
                &app.config.layout,
                layout_minimums(&app.config),
                LayoutOverrides::default(),
            )
            .input;
            let available = input.width - 3;
            let padding = match alignment {
                PanelAlignment::Left => 0,
                PanelAlignment::Center => available / 2 - 2,
                PanelAlignment::Right => available - 4,
            };
            let cursor = terminal.get_cursor_position().unwrap();
            assert_eq!(
                (cursor.x, cursor.y),
                (input.x + 1 + padding + 1, input.y + 1)
            );
            assert_eq!(
                terminal.backend().buffer()[(input.x + 1 + padding, input.y + 1)].symbol(),
                "é"
            );
        }
    }

    #[tokio::test]
    async fn user_input_refreshes_cached_output_immediately() {
        use crate::events::TerminalEvent;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut config = AppConfig::default();
        config.panels.output.refresh_ms = 60_000;
        let mut app = App::new(config);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.state.clear_output();
        app.state.push_output("OLD", OutputCategory::Normal);
        terminal.draw(|frame| app.render(frame)).unwrap();
        app.state.clear_output();
        app.state.push_output("NEW", OutputCategory::Normal);
        terminal.draw(|frame| app.render(frame)).unwrap();
        let content = |terminal: &Terminal<TestBackend>| {
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        assert!(content(&terminal).contains("OLD"));
        assert!(!content(&terminal).contains("NEW"));
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            &tx,
        )
        .await;
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(content(&terminal).contains("NEW"));
        assert_eq!(app.state.input, "x");
    }
}
