use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    animation::weather::WeatherKind,
    config::{
        GaugeConfig, LayoutConfig, MapRenderConfig, PanelConfig, PanelMode, PanelOptions,
        WeatherConfig,
    },
    state::{AppState, GroupMember, SocialChannel, SocialMessage},
    ui::{
        gauges::{GaugeKind, GaugeValue, compact_gauge, compact_line, render_gauge, status_line},
        layout::UiMode,
        map::{render_map, render_nearby_map},
        panels::panel,
        theme::Theme,
    },
};

fn render_info_panel(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    title: &str,
    weather_config: &WeatherConfig,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let time = display_time(state.world.time.as_deref());
    let weather = state.world.weather.as_deref().unwrap_or("--");
    let marker = weather_marker(weather, weather_config);
    let mut lines = vec![Line::from(vec![
        Span::styled("Time ", Style::new().fg(theme.muted)),
        Span::styled(time, Style::new().fg(theme.foreground)),
    ])];
    lines.extend(weather_lines(weather, marker, inner.width as usize, theme));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

pub fn render_info(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    panels: &PanelConfig,
    weather: &WeatherConfig,
) {
    render_info_panel(area, buf, state, theme, &panels.info.title, weather);
}

pub fn render_details(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    config: ResponsivePanelConfig<'_>,
) {
    let gauges = config.gauges;
    let layout = config.layout;
    let panels = config.panels;
    let mode = config.mode;
    let mut kinds = Vec::new();
    if layout.show_opponent && panel_visible(&panels.opponent, mode) {
        kinds.push(DetailPanel::Opponent);
    }
    if layout.show_group && panel_visible(&panels.group, mode) {
        kinds.push(DetailPanel::Group);
    }
    if layout.show_character && panel_visible(&panels.character, mode) {
        kinds.push(DetailPanel::Character);
    }
    if kinds.is_empty() {
        return;
    }

    kinds.sort_by_key(|kind| std::cmp::Reverse(kind.priority(panels)));
    let direction = pane_direction(area);
    let available = pane_extent(area, direction);
    let mut used = 0_u16;
    kinds.retain(|kind| {
        let minimum = kind.minimum(panels, direction);
        if used.saturating_add(minimum) > available {
            return false;
        }
        used = used.saturating_add(minimum);
        true
    });
    if kinds.is_empty() {
        return;
    }
    let rows = Layout::default()
        .direction(direction)
        .constraints(
            kinds
                .iter()
                .map(|kind| Constraint::Min(kind.minimum(panels, direction))),
        )
        .split(area);
    for (kind, row) in kinds.into_iter().zip(rows.iter().copied()) {
        match kind {
            DetailPanel::Opponent => {
                render_opponent_panel(row, buf, state, theme, gauges, &panels.opponent.title);
            }
            DetailPanel::Group => {
                render_group_panel(row, buf, state, theme, gauges, &panels.group.title);
            }
            DetailPanel::Character => {
                render_character_panel(row, buf, state, theme, gauges, &panels.character.title);
            }
        }
    }
}

pub fn render_status_dashboard(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    config: ResponsivePanelConfig<'_>,
) {
    let panels = config.panels;
    let weather = config.weather;
    for (kind, column) in dashboard_panel_areas(area, config) {
        match kind {
            DashboardPanel::World => render_info(column, buf, state, theme, panels, weather),
            DashboardPanel::Social => {
                render_social_panel(column, buf, state, theme, &panels.social.title);
            }
            DashboardPanel::NearbyMap => {
                render_nearby_map(column, buf, &state.map, theme, config.map, "Nearby Map");
            }
        }
    }
}

pub fn status_dashboard_social_area(
    area: ratatui::layout::Rect,
    config: ResponsivePanelConfig<'_>,
) -> Option<ratatui::layout::Rect> {
    dashboard_panel_areas(area, config)
        .into_iter()
        .find_map(|(kind, area)| (kind == DashboardPanel::Social).then_some(area))
}

pub fn social_scroll_max(state: &AppState, width: u16, height: u16, theme: &Theme) -> usize {
    let inner_width = width.saturating_sub(2) as usize;
    let inner_height = height.saturating_sub(2) as usize;
    social_wrapped_rows(state.social.messages.iter().collect(), inner_width, theme)
        .len()
        .saturating_sub(inner_height)
}

pub fn render_classic_sidebar(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    config: ResponsivePanelConfig<'_>,
) {
    let panels = config.panels;
    let mut optional = Vec::new();
    if panel_visible(&panels.info, UiMode::FullHd) {
        optional.push(ClassicSidebarPanel::Info);
    }
    if config.layout.show_opponent && panel_visible(&panels.opponent, UiMode::FullHd) {
        optional.push(ClassicSidebarPanel::Opponent);
    }
    if config.layout.show_group && panel_visible(&panels.group, UiMode::FullHd) {
        optional.push(ClassicSidebarPanel::Group);
    }
    if config.layout.show_character && panel_visible(&panels.character, UiMode::FullHd) {
        optional.push(ClassicSidebarPanel::Character);
    }
    optional.sort_by_key(|kind| std::cmp::Reverse(kind.priority(panels)));
    let map_height = panels.map.min_height.min(area.height);
    let mut used = map_height;
    optional.retain(|kind| {
        let minimum = kind.min_height(panels);
        if used.saturating_add(minimum) > area.height {
            return false;
        }
        used = used.saturating_add(minimum);
        true
    });
    let mut kinds = vec![ClassicSidebarPanel::Map];
    kinds.extend(optional);
    kinds.sort_by_key(|kind| kind.display_order());
    let panel_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(kinds.iter().map(|kind| kind.sidebar_constraint(panels)))
        .split(area);
    for (kind, row) in kinds.into_iter().zip(panel_rows.iter().copied()) {
        match kind {
            ClassicSidebarPanel::Info => {
                render_info(row, buf, state, theme, panels, config.weather);
            }
            ClassicSidebarPanel::Map => {
                render_map(row, buf, &state.map, theme, config.map, &panels.map.title)
            }
            ClassicSidebarPanel::Opponent => render_opponent_panel(
                row,
                buf,
                state,
                theme,
                config.gauges,
                &panels.opponent.title,
            ),
            ClassicSidebarPanel::Group => {
                render_group_panel(row, buf, state, theme, config.gauges, &panels.group.title)
            }
            ClassicSidebarPanel::Character => render_character_panel(
                row,
                buf,
                state,
                theme,
                config.gauges,
                &panels.character.title,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResponsivePanelConfig<'a> {
    gauges: &'a GaugeConfig,
    layout: &'a LayoutConfig,
    map: &'a MapRenderConfig,
    panels: &'a PanelConfig,
    weather: &'a WeatherConfig,
    mode: UiMode,
}

impl<'a> ResponsivePanelConfig<'a> {
    pub fn new(
        gauges: &'a GaugeConfig,
        layout: &'a LayoutConfig,
        map: &'a MapRenderConfig,
        panels: &'a PanelConfig,
        weather: &'a WeatherConfig,
        mode: UiMode,
        _expanded: bool,
    ) -> Self {
        Self {
            gauges,
            layout,
            map,
            panels,
            weather,
            mode,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ClassicSidebarPanel {
    Info,
    Map,
    Opponent,
    Group,
    Character,
}

impl ClassicSidebarPanel {
    fn priority(self, panels: &PanelConfig) -> i32 {
        match self {
            Self::Info => panels.info.priority,
            Self::Map => panels.map.priority,
            Self::Opponent => panels.opponent.priority,
            Self::Group => panels.group.priority,
            Self::Character => panels.character.priority,
        }
    }

    fn min_height(self, panels: &PanelConfig) -> u16 {
        match self {
            Self::Info => panels.info.min_height,
            Self::Map => panels.map.min_height,
            Self::Opponent => panels.opponent.min_height,
            Self::Group => panels.group.min_height,
            Self::Character => panels.character.min_height,
        }
    }

    fn display_order(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Map => 1,
            Self::Opponent => 2,
            Self::Group => 3,
            Self::Character => 4,
        }
    }

    fn sidebar_constraint(self, panels: &PanelConfig) -> Constraint {
        match self {
            Self::Info => Constraint::Length(self.min_height(panels)),
            Self::Opponent => Constraint::Length(self.min_height(panels)),
            _ => Constraint::Min(self.min_height(panels)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DetailPanel {
    Opponent,
    Group,
    Character,
}

impl DetailPanel {
    fn priority(self, panels: &PanelConfig) -> i32 {
        match self {
            Self::Opponent => panels.opponent.priority,
            Self::Group => panels.group.priority,
            Self::Character => panels.character.priority,
        }
    }

    fn min_height(self, panels: &PanelConfig) -> u16 {
        match self {
            Self::Opponent => panels.opponent.min_height,
            Self::Group => panels.group.min_height,
            Self::Character => panels.character.min_height,
        }
    }

    fn min_width(self, panels: &PanelConfig) -> u16 {
        match self {
            Self::Opponent => panels.opponent.min_width,
            Self::Group => panels.group.min_width,
            Self::Character => panels.character.min_width,
        }
    }

    fn minimum(self, panels: &PanelConfig, direction: Direction) -> u16 {
        match direction {
            Direction::Horizontal => self.min_width(panels),
            Direction::Vertical => self.min_height(panels),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardPanel {
    World,
    Social,
    NearbyMap,
}

impl DashboardPanel {
    fn min_width(self, panels: &PanelConfig) -> u16 {
        match self {
            Self::World => panels.info.min_width,
            Self::Social => panels.social.min_width,
            Self::NearbyMap => panels.map.min_width,
        }
    }

    fn min_height(self, panels: &PanelConfig) -> u16 {
        match self {
            Self::World => panels.info.min_height,
            Self::Social => panels.social.min_height,
            Self::NearbyMap => panels.map.min_height,
        }
    }

    fn minimum(self, panels: &PanelConfig, direction: Direction) -> u16 {
        match direction {
            Direction::Horizontal => self.min_width(panels),
            Direction::Vertical => self.min_height(panels),
        }
    }
}

fn render_social_panel(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let height = inner.height as usize;
    let width = inner.width as usize;
    let lines = social_lines(
        state.social.messages.iter().collect::<Vec<_>>(),
        width,
        height,
        state.social.scroll_offset,
        theme,
    );
    Paragraph::new(lines).render(inner, buf);
}

fn social_lines(
    messages: Vec<&SocialMessage>,
    width: usize,
    height: usize,
    scroll_offset: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    if messages.is_empty() {
        return vec![Line::from(Span::styled(
            "No social messages",
            Style::new().fg(theme.muted),
        ))];
    }
    let rows = social_wrapped_rows(messages, width, theme);
    let max_offset = rows.len().saturating_sub(height);
    let visible_end = rows.len().saturating_sub(scroll_offset.min(max_offset));
    let visible_start = visible_end.saturating_sub(height);
    rows.into_iter()
        .skip(visible_start)
        .take(visible_end.saturating_sub(visible_start))
        .collect()
}

fn social_wrapped_rows(
    messages: Vec<&SocialMessage>,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    messages
        .into_iter()
        .flat_map(|message| wrap_styled_line(social_line(message, theme), width))
        .collect()
}

fn social_line(message: &SocialMessage, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("[{}]", message.timestamp),
            Style::new().fg(theme.muted),
        ),
        Span::styled(
            format!("({}) ", message.channel.label()),
            Style::new().fg(social_channel_color(message.channel, theme)),
        ),
        Span::styled(message.prefix.clone(), Style::new().fg(theme.muted)),
        Span::styled(message.text.clone(), Style::new().fg(theme.foreground)),
    ])
}

fn wrap_styled_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width == 0 || line.spans.is_empty() {
        return vec![line];
    }
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;
    for span in line.spans {
        for token in styled_tokens(span.content.as_ref(), span.style) {
            push_wrapped_token(
                token.text,
                token.style,
                width,
                &mut rows,
                &mut current,
                &mut current_width,
            );
        }
    }
    rows.push(Line::from(current));
    rows
}

struct StyledToken {
    text: String,
    style: Style,
}

fn styled_tokens(text: &str, style: Style) -> Vec<StyledToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_is_whitespace = None;
    for value in text.chars() {
        let is_whitespace = value.is_whitespace();
        if current.is_empty() || current_is_whitespace == Some(is_whitespace) {
            current.push(value);
            current_is_whitespace = Some(is_whitespace);
        } else {
            tokens.push(StyledToken {
                text: std::mem::take(&mut current),
                style,
            });
            current.push(value);
            current_is_whitespace = Some(is_whitespace);
        }
    }
    if !current.is_empty() {
        tokens.push(StyledToken {
            text: current,
            style,
        });
    }
    tokens
}

fn push_wrapped_token(
    token: String,
    style: Style,
    width: usize,
    rows: &mut Vec<Line<'static>>,
    current: &mut Vec<Span<'static>>,
    current_width: &mut usize,
) {
    let token_width = display_width(&token);
    if token_width <= width {
        if *current_width + token_width > width && !current.is_empty() {
            rows.push(Line::from(std::mem::take(current)));
            *current_width = 0;
        }
        current.push(Span::styled(token, style));
        *current_width += token_width;
        return;
    }
    for value in token.chars() {
        let value_width = value.width().unwrap_or(0);
        if *current_width + value_width > width && !current.is_empty() {
            rows.push(Line::from(std::mem::take(current)));
            *current_width = 0;
        }
        current.push(Span::styled(value.to_string(), style));
        *current_width += value_width;
    }
}

fn display_width(value: &str) -> usize {
    value.chars().map(|value| value.width().unwrap_or(0)).sum()
}

fn social_channel_color(channel: SocialChannel, theme: &Theme) -> ratatui::style::Color {
    match channel {
        SocialChannel::Tells | SocialChannel::Chats | SocialChannel::Group => theme.success,
        SocialChannel::Says => theme.foreground,
        SocialChannel::Narrates | SocialChannel::Yells | SocialChannel::Sings => theme.warning,
    }
}

fn pane_direction(area: ratatui::layout::Rect) -> Direction {
    if area.width >= area.height.saturating_mul(2) {
        Direction::Horizontal
    } else {
        Direction::Vertical
    }
}

fn pane_extent(area: ratatui::layout::Rect, direction: Direction) -> u16 {
    match direction {
        Direction::Horizontal => area.width,
        Direction::Vertical => area.height,
    }
}

fn dashboard_panel_areas(
    area: ratatui::layout::Rect,
    config: ResponsivePanelConfig<'_>,
) -> Vec<(DashboardPanel, ratatui::layout::Rect)> {
    let panels = config.panels;
    let mode = config.mode;
    let mut kinds = Vec::new();
    if panel_visible(&panels.info, mode) {
        kinds.push(DashboardPanel::World);
    }
    if config.layout.show_social && panel_visible(&panels.social, mode) {
        kinds.push(DashboardPanel::Social);
    }
    kinds.push(DashboardPanel::NearbyMap);
    let direction = pane_direction(area);
    let available = pane_extent(area, direction);
    let mut used = 0_u16;
    kinds.retain(|kind| {
        let minimum = kind.minimum(panels, direction);
        if used.saturating_add(minimum) > available {
            return false;
        }
        used = used.saturating_add(minimum);
        true
    });
    if kinds.is_empty() {
        return Vec::new();
    }
    let columns = Layout::default()
        .direction(direction)
        .constraints(
            kinds
                .iter()
                .map(|kind| Constraint::Min(kind.minimum(panels, direction))),
        )
        .split(area);
    kinds.into_iter().zip(columns.iter().copied()).collect()
}

fn panel_visible(panel: &PanelOptions, mode: UiMode) -> bool {
    panel.enabled
        && (panel.visible_modes.is_empty()
            || panel.visible_modes.iter().any(|visible_mode| {
                matches!(
                    (visible_mode, mode),
                    (PanelMode::Mobile, UiMode::Mobile)
                        | (PanelMode::Tablet, UiMode::Tablet)
                        | (PanelMode::FullHd, UiMode::FullHd)
                        | (PanelMode::Ultrawide, UiMode::Ultrawide)
                )
            }))
}

fn weather_marker(weather: &str, config: &WeatherConfig) -> &'static str {
    if !config.enabled || !config.show_info_marker {
        return "";
    }
    let kind = WeatherKind::classify(weather);
    if kind == WeatherKind::Clear {
        ""
    } else {
        kind.frame(0)
    }
}

fn display_time(time: Option<&str>) -> String {
    let Some(time) = time else {
        return "--".to_string();
    };
    if let Some(index) = time.find("AM").or_else(|| time.find("PM")) {
        return time[..index.saturating_add(2)].to_string();
    }
    time.to_string()
}

fn weather_lines<'a>(weather: &str, marker: &str, width: usize, theme: &Theme) -> Vec<Line<'a>> {
    const LABEL: &str = "Weather ";
    const INDENT: &str = "        ";

    let first_width = width.saturating_sub(LABEL.len()).max(1);
    let next_width = width.saturating_sub(INDENT.len()).max(1);
    let mut wrapped = wrap_words(weather, first_width, next_width);
    if wrapped.is_empty() {
        wrapped.push(String::from("--"));
    }

    wrapped
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                let mut spans = vec![Span::styled(LABEL, Style::new().fg(theme.muted))];
                if !marker.is_empty() {
                    spans.push(Span::styled(
                        marker.to_string(),
                        Style::new().fg(theme.accent),
                    ));
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(line, Style::new().fg(theme.foreground)));
                Line::from(spans)
            } else {
                Line::from(vec![
                    Span::raw(INDENT),
                    Span::styled(line, Style::new().fg(theme.foreground)),
                ])
            }
        })
        .collect()
}

fn wrap_words(text: &str, first_width: usize, next_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = first_width.max(1);

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }
        if current.len() + 1 + word.len() <= current_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
            current_width = next_width.max(1);
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn render_opponent_panel(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    gauges: &GaugeConfig,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    if let Some(name) = &state.opponent.name {
        let name = opponent_title_line(name, state.opponent.level.as_deref(), theme);
        Paragraph::new(name).render(inner, buf);
        if inner.height > 1 {
            let gauge_area = ratatui::layout::Rect {
                y: inner.y.saturating_add(1),
                height: 1,
                ..inner
            };
            render_gauge(
                gauge_area,
                buf,
                GaugeKind::Health,
                opponent_health(state),
                theme,
                gauges,
            );
        }
    } else {
        let line = Line::from(Span::styled(
            "No current target",
            Style::new().fg(theme.muted),
        ));
        Paragraph::new(vec![line]).render(inner, buf);
    }
}

fn render_group_panel(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    gauges: &GaugeConfig,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let lines = if state.group.members.is_empty() {
        vec![Line::from(Span::styled(
            "No group",
            Style::new().fg(theme.muted),
        ))]
    } else {
        group_lines(
            &state.group.members,
            inner.width as usize,
            inner.height as usize,
            theme,
            gauges,
        )
    };
    Paragraph::new(lines).render(inner, buf);
}

fn group_lines<'a>(
    members: &[GroupMember],
    width: usize,
    height: usize,
    theme: &Theme,
    gauges: &GaugeConfig,
) -> Vec<Line<'a>> {
    if height == 0 {
        return Vec::new();
    }
    if members.len().saturating_mul(2) <= height && width >= 32 {
        return rich_group_lines(members, theme, gauges);
    }
    compact_group_lines(members, width, height, theme)
}

fn rich_group_lines<'a>(
    members: &[GroupMember],
    theme: &Theme,
    gauges: &GaugeConfig,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(members.len().saturating_mul(2));
    for member in members {
        lines.push(Line::from(Span::styled(
            member.name.clone(),
            Style::new().fg(theme.foreground),
        )));
        lines.push(status_line(vec![
            compact_gauge(
                GaugeKind::Health,
                percent_value(member.health_percent),
                gauges,
            ),
            compact_gauge(GaugeKind::Mana, percent_value(member.mana_percent), gauges),
            compact_gauge(
                GaugeKind::Movement,
                percent_value(member.movement_percent),
                gauges,
            ),
        ]));
    }
    lines
}

fn compact_group_lines<'a>(
    members: &[GroupMember],
    width: usize,
    height: usize,
    theme: &Theme,
) -> Vec<Line<'a>> {
    const MIN_CELL_WIDTH: usize = 20;

    let max_columns = (width / MIN_CELL_WIDTH).max(1);
    let needed_columns = members.len().div_ceil(height).max(1);
    let columns = needed_columns.min(max_columns);
    let cell_width = (width / columns).max(1);
    let capacity = columns.saturating_mul(height);
    let (visible_members, hidden) = if members.len() > capacity {
        let visible = capacity.saturating_sub(1);
        (visible, members.len().saturating_sub(visible))
    } else {
        (members.len(), 0)
    };

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::new();
        for column in 0..columns {
            let slot = row.saturating_mul(columns).saturating_add(column);
            if slot >= capacity {
                continue;
            }
            if slot < visible_members {
                spans.extend(group_member_cell(&members[slot], cell_width, theme));
            } else if hidden > 0 && slot == visible_members {
                spans.push(Span::styled(
                    fit_cell(&format!("+{hidden} more"), cell_width),
                    Style::new().fg(theme.muted),
                ));
            } else {
                spans.push(Span::raw(" ".repeat(cell_width)));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn group_member_cell<'a>(member: &GroupMember, width: usize, theme: &Theme) -> Vec<Span<'a>> {
    const VITALS_WIDTH: usize = 12;

    if width < VITALS_WIDTH + 1 {
        return vec![Span::styled(
            fit_cell(&member.name, width),
            Style::new().fg(theme.foreground),
        )];
    }

    let name_width = width.saturating_sub(VITALS_WIDTH).max(1);
    let name = truncate_cell(&member.name, name_width);
    let name_len = name.chars().count();

    let mut spans = vec![
        Span::styled(name, Style::new().fg(theme.foreground)),
        Span::raw(" ".repeat(name_width.saturating_sub(name_len))),
        Span::styled("H", Style::new().fg(GaugeKind::Health.color())),
        Span::styled(
            format!("{:>3}", percent_text(member.health_percent)),
            Style::new().fg(GaugeKind::Health.color()),
        ),
        Span::styled("M", Style::new().fg(GaugeKind::Mana.color())),
        Span::styled(
            format!("{:>3}", percent_text(member.mana_percent)),
            Style::new().fg(GaugeKind::Mana.color()),
        ),
        Span::styled("V", Style::new().fg(GaugeKind::Movement.color())),
        Span::styled(
            format!("{:>3}", percent_text(member.movement_percent)),
            Style::new().fg(GaugeKind::Movement.color()),
        ),
    ];
    let used_width = name_width.saturating_add(VITALS_WIDTH);
    if width > used_width {
        spans.push(Span::raw(" ".repeat(width - used_width)));
    }
    spans
}

fn percent_text(percent: Option<i64>) -> String {
    percent
        .map(|value| value.clamp(0, 100).to_string())
        .unwrap_or_else(|| "--".to_string())
}

fn truncate_cell(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let length = value.chars().count();
    if length <= width {
        return value.to_string();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }
    let mut output = value.chars().take(width - 3).collect::<String>();
    output.push_str("...");
    output
}

fn fit_cell(value: &str, width: usize) -> String {
    let mut output = truncate_cell(value, width);
    let length = output.chars().count();
    if length < width {
        output.push_str(&" ".repeat(width - length));
    }
    output
}

fn render_character_panel(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    gauges: &GaugeConfig,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    if inner.height < 6 {
        render_compact(inner, buf, state, theme);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    render_gauge(
        rows[0],
        buf,
        GaugeKind::Health,
        health(state),
        theme,
        gauges,
    );
    render_gauge(rows[1], buf, GaugeKind::Mana, mana(state), theme, gauges);
    render_gauge(
        rows[2],
        buf,
        GaugeKind::Movement,
        movement(state),
        theme,
        gauges,
    );

    Paragraph::new(character_sheet_lines(state, theme)).render(rows[4], buf);
    render_gauge(rows[5], buf, GaugeKind::Tnl, tnl(state), theme, gauges);
}

pub fn compact_status<'a>(state: &AppState, theme: &Theme) -> Line<'a> {
    status_line(vec![
        compact_line(GaugeKind::Health, health(state), theme),
        compact_line(GaugeKind::Mana, mana(state), theme),
        compact_line(GaugeKind::Movement, movement(state), theme),
    ])
}

fn render_compact(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
) {
    Paragraph::new(compact_status(state, theme)).render(area, buf);
}

fn health(state: &AppState) -> GaugeValue {
    GaugeValue {
        current: state.character.health,
        max: state.character.health_max,
    }
}

fn mana(state: &AppState) -> GaugeValue {
    GaugeValue {
        current: state.character.mana,
        max: state.character.mana_max,
    }
}

fn movement(state: &AppState) -> GaugeValue {
    GaugeValue {
        current: state.character.movement,
        max: state.character.movement_max,
    }
}

fn tnl(state: &AppState) -> GaugeValue {
    GaugeValue {
        current: state.character.experience,
        max: state.character.experience_max,
    }
}

fn character_sheet_lines<'a>(state: &AppState, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = vec![
        metric_line(
            &[
                ("Lvl", state.character.level),
                ("OB", state.character.offensive_bonus),
                ("DB", state.character.dodge),
                ("PB", state.character.parry),
                ("Spd", state.character.attack_speed),
            ],
            theme,
        ),
        metric_line(
            &[
                ("Str", state.character.strength),
                ("Int", state.character.intelligence),
                ("Wil", state.character.will),
                ("Dex", state.character.dexterity),
                ("Con", state.character.constitution),
                ("Lea", state.character.learning),
            ],
            theme,
        ),
    ];

    if let Some(class_lines) = class_specific_lines(state, theme) {
        lines.push(Line::default());
        lines.extend(class_lines);
    }

    lines
}

fn class_specific_lines<'a>(state: &AppState, theme: &Theme) -> Option<Vec<Line<'a>>> {
    match highest_class(state) {
        Some(CharacterClass::Mage) => Some(vec![metric_line(
            &[
                ("Mana Regen", state.character.stamina_regeneration),
                ("Spell Power", state.character.spell_power),
                ("Spell Pen", state.character.spell_pen),
            ],
            theme,
        )]),
        Some(CharacterClass::Mystic) => Some(vec![metric_line(
            &[
                ("Willpower", state.character.willpower),
                ("Spirits", state.character.spirit),
                ("Health Regen", state.character.health_regeneration),
                ("Movement Regen", state.character.movement_regeneration),
            ],
            theme,
        )]),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterClass {
    Mage,
    Mystic,
}

fn highest_class(state: &AppState) -> Option<CharacterClass> {
    let mage = state.character.mage_level?;
    let mystic = state.character.mystic_level.unwrap_or(0);
    let warrior = state.character.warrior_level.unwrap_or(0);
    let ranger = state.character.ranger_level.unwrap_or(0);
    if mage > mystic && mage > warrior && mage > ranger {
        return Some(CharacterClass::Mage);
    }

    let mystic = state.character.mystic_level?;
    if mystic > mage && mystic > warrior && mystic > ranger {
        return Some(CharacterClass::Mystic);
    }

    None
}

fn metric_line<'a>(values: &[(&str, Option<i64>)], theme: &Theme) -> Line<'a> {
    let mut spans = Vec::new();
    for (index, (label, value)) in values.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("{label} "),
            Style::new().fg(theme.muted),
        ));
        spans.push(Span::styled(
            value
                .map(|number| number.to_string())
                .unwrap_or_else(|| "--".to_string()),
            Style::new().fg(theme.foreground),
        ));
    }
    Line::from(spans)
}

fn opponent_health(state: &AppState) -> GaugeValue {
    let max = match state.opponent.health_max {
        Some(max) if max > 0 => Some(max),
        _ => Some(100),
    };
    GaugeValue {
        current: state.opponent.health,
        max,
    }
}

fn opponent_title_line(name: &str, level: Option<&str>, theme: &Theme) -> Line<'static> {
    let label = match level {
        Some(level) if !level.trim().is_empty() => format!("{name} (lvl {level})"),
        _ => name.to_string(),
    };
    Line::from(Span::styled(label, Style::new().fg(theme.enemy))).bold()
}

fn percent_value(percent: Option<i64>) -> GaugeValue {
    GaugeValue {
        current: percent,
        max: Some(100),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{buffer::Buffer, layout::Rect, style::Color};

    use crate::{
        config::{GaugeConfig, LayoutConfig, MapRenderConfig},
        state::AppState,
        ui::theme::Theme,
    };

    use super::*;

    #[test]
    fn detail_panels_drop_lower_priority_content_when_height_is_limited() {
        let state = AppState::new(&crate::config::AppConfig::default());
        let layout = LayoutConfig {
            show_group: true,
            ..LayoutConfig::default()
        };
        let panels = PanelConfig::default();
        let area = Rect::new(0, 0, 48, 6);
        let mut buffer = Buffer::empty(area);

        render_details(
            area,
            &mut buffer,
            &state,
            &theme(),
            ResponsivePanelConfig::new(
                &GaugeConfig::default(),
                &layout,
                &MapRenderConfig::default(),
                &panels,
                &WeatherConfig::default(),
                UiMode::FullHd,
                false,
            ),
        );

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("Opponent")));
        assert!(lines.iter().any(|line| line.contains("Group")));
        assert!(!lines.iter().any(|line| line.contains("Character")));
    }

    #[test]
    fn classic_sidebar_keeps_world_panel_compact() {
        let panels = PanelConfig::default();

        assert_eq!(
            ClassicSidebarPanel::Info.sidebar_constraint(&panels),
            Constraint::Length(panels.info.min_height)
        );
        assert_eq!(
            ClassicSidebarPanel::Map.sidebar_constraint(&panels),
            Constraint::Min(panels.map.min_height)
        );
    }

    #[test]
    fn small_group_uses_rich_two_line_layout_when_it_fits() {
        let members = group_members(2);

        let lines = group_lines(&members, 56, 4, &theme(), &GaugeConfig::default());
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(rendered.len(), 4);
        assert!(rendered[0].contains("Member01"));
        assert!(rendered[1].contains("HP"));
        assert!(rendered[2].contains("Member02"));
        assert!(rendered[3].contains("MP"));
    }

    #[test]
    fn large_group_uses_multicolumn_compact_rows() {
        let members = group_members(20);

        let lines = group_lines(&members, 68, 8, &theme(), &GaugeConfig::default());
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>();
        let joined = rendered.join("\n");

        assert_eq!(rendered.len(), 8);
        assert!(joined.contains("Member01"));
        assert!(joined.contains("Member20"));
        assert!(joined.contains("H100"));
        assert!(joined.contains("M 71"));
        assert!(joined.contains("V  8"));
        assert!(!joined.contains("more"));
    }

    #[test]
    fn overflowing_group_shows_hidden_member_count() {
        let members = group_members(10);

        let lines = group_lines(&members, 40, 3, &theme(), &GaugeConfig::default());
        let joined = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(joined.contains("Member01"));
        assert!(joined.contains("Member05"));
        assert!(joined.contains("+5 more"));
        assert!(!joined.contains("Member10"));
    }

    #[test]
    fn info_panel_wraps_long_weather_text() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.world.time = Some("Morning".to_string());
        state.world.weather =
            Some("Above the fields, not a cloud can be seen in the sky.".to_string());
        let area = Rect::new(0, 0, 32, 5);
        let mut buffer = Buffer::empty(area);

        render_info_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            "Info",
            &WeatherConfig::default(),
        );

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("Weather Above")));
        assert!(lines.iter().any(|line| line.contains("can be seen")));
    }

    #[test]
    fn info_panel_trims_rots_time_suffix() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.world.time = Some("It is about 8:00 AM on a quiet morning".to_string());
        state.world.weather = Some("Clear".to_string());
        let area = Rect::new(0, 0, 32, 5);
        let mut buffer = Buffer::empty(area);

        render_info_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            "Info",
            &WeatherConfig::default(),
        );

        let lines = buffer_lines(&buffer, area);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Time It is about 8:00 AM"))
        );
        assert!(!lines.iter().any(|line| line.contains("8:00 AM on")));
    }

    #[test]
    fn status_dashboard_renders_world_social_and_nearby_map_without_duplicate_details() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.world.time = Some("It is about 8:00 AM on a quiet morning".to_string());
        state
            .social
            .push(SocialChannel::Tells, "12:34", "from Gimli - ", "hello");
        state.map.create();
        let area = Rect::new(0, 0, 120, 7);
        let mut buffer = Buffer::empty(area);
        let mut panels = PanelConfig::default();
        panels.info.title = "World".to_string();

        render_status_dashboard(
            area,
            &mut buffer,
            &state,
            &theme(),
            ResponsivePanelConfig::new(
                &GaugeConfig::default(),
                &LayoutConfig::default(),
                &MapRenderConfig::default(),
                &panels,
                &WeatherConfig::default(),
                UiMode::Ultrawide,
                true,
            ),
        );

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("World")));
        assert!(lines.iter().any(|line| line.contains("Social")));
        assert!(lines.iter().any(|line| line.contains("[12:34](tell)")));
        assert!(lines.iter().any(|line| line.contains("from Gimli - ")));
        assert!(lines.iter().any(|line| line.contains("hello")));
        assert!(lines.iter().any(|line| line.contains("Nearby Map")));
        assert!(!lines.iter().any(|line| line.contains("Opponent")));
        assert!(!lines.iter().any(|line| line.contains("Group")));
        assert!(!lines.iter().any(|line| line.contains("Character")));
    }

    #[test]
    fn social_panel_wraps_long_messages() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.social.push(
            SocialChannel::Chats,
            "12:34",
            "Bilbo - ",
            "this is a very long social message",
        );
        let area = Rect::new(0, 0, 28, 5);
        let mut buffer = Buffer::empty(area);

        render_social_panel(area, &mut buffer, &state, &theme(), "Social");

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("[12:34](chat)")));
        assert!(lines.iter().any(|line| line.contains("long social")));
    }

    #[test]
    fn social_panel_scrolls_wrapped_rows() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.social.push(
            SocialChannel::Chats,
            "12:34",
            "Bilbo - ",
            "alpha beta gamma delta epsilon zeta eta theta",
        );
        let theme = theme();
        let max_offset = social_scroll_max(&state, 28, 4, &theme);

        state.social.scroll_up_to(3, max_offset);
        let lines = social_lines(
            state.social.messages.iter().collect(),
            26,
            2,
            state.social.scroll_offset,
            &theme,
        );
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(max_offset > 0);
        assert!(rendered.contains("[12:34](chat)"));
        assert!(!rendered.contains("theta"));
    }

    #[test]
    fn opponent_panel_renders_health_as_gauge_with_percent() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.opponent.name = Some("Orc Captain".to_string());
        state.opponent.level = Some("12".to_string());
        state.opponent.health = Some(68);
        let area = Rect::new(0, 0, 36, 7);
        let mut buffer = Buffer::empty(area);

        render_opponent_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            &GaugeConfig::default(),
            "Opponent",
        );

        let lines = buffer_lines(&buffer, area);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("Orc Captain (lvl 12)"))
        );
        assert!(lines.iter().any(|line| line.contains("Health")));
        assert!(lines.iter().any(|line| line.contains("68 / 100")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("████") || line.contains("####"))
        );
    }

    #[test]
    fn opponent_panel_treats_zero_max_as_percentage_scale() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.opponent.name = Some("Orc Captain".to_string());
        state.opponent.health = Some(68);
        state.opponent.health_max = Some(0);
        let area = Rect::new(0, 0, 36, 7);
        let mut buffer = Buffer::empty(area);

        render_opponent_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            &GaugeConfig::default(),
            "Opponent",
        );

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("68 / 100")));
    }

    #[test]
    fn character_panel_renders_sheet_without_name_connection_or_animation() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.connection = crate::state::ConnectionStatus::Connected;
        state.character.name = Some("Jeggred".to_string());
        state.character.level = Some(57);
        state.character.health = Some(465);
        state.character.health_max = Some(465);
        state.character.mana = Some(71);
        state.character.mana_max = Some(71);
        state.character.movement = Some(176);
        state.character.movement_max = Some(176);
        state.character.experience = Some(137000);
        state.character.experience_max = Some(200000);
        state.character.offensive_bonus = Some(134);
        state.character.dodge = Some(29);
        state.character.parry = Some(58);
        state.character.attack_speed = Some(12);
        state.character.strength = Some(20);
        state.character.intelligence = Some(17);
        state.character.will = Some(18);
        state.character.dexterity = Some(19);
        state.character.constitution = Some(21);
        state.character.learning = Some(16);
        state.character.willpower = Some(44);
        state.character.spell_save = Some(55);
        state.character.spirit = Some(2310);
        state.character.spell_power = Some(7);
        state.character.spell_pen = Some(3);
        state.character.mage_level = Some(57);
        state.character.mystic_level = Some(20);
        state.character.warrior_level = Some(30);
        state.character.ranger_level = Some(25);
        state.character.stamina_regeneration = Some(9);
        let area = Rect::new(0, 0, 56, 12);
        let mut buffer = Buffer::empty(area);

        render_character_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            &GaugeConfig::default(),
            "Character",
        );

        let lines = buffer_lines(&buffer, area);
        assert!(lines.iter().any(|line| line.contains("Health")));
        assert!(lines.iter().any(|line| line.contains("Lvl 57")));
        assert!(lines.iter().any(|line| line.contains("OB 134")));
        assert!(lines.iter().any(|line| line.contains("Str 20")
            && line.contains("Int 17")
            && line.contains("Wil 18")
            && line.contains("Dex 19")
            && line.contains("Con 21")
            && line.contains("Lea 16")));
        let movement_index = lines
            .iter()
            .position(|line| line.contains("Movement"))
            .expect("movement gauge row");
        assert!(!lines[movement_index + 1].contains("Lvl"));
        assert!(lines[movement_index + 2].contains("Lvl 57"));
        let stats_index = lines
            .iter()
            .position(|line| line.contains("Str 20"))
            .expect("base stats row");
        assert!(lines[stats_index + 1].trim_matches(['│', ' ']).is_empty());
        assert!(lines.iter().any(|line| line.contains("Mana Regen 9")
            && line.contains("Spell Power 7")
            && line.contains("Spell Pen 3")));
        assert!(lines.iter().any(|line| line.contains("TNL")));
        assert!(!lines.iter().any(|line| line.contains("Jeggred")));
        assert!(!lines.iter().any(|line| line.contains("Connected")));
        assert!(!lines.iter().any(|line| line.contains("Face")));
    }

    #[test]
    fn character_panel_renders_mystic_specific_stats_when_mystic_is_highest() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.character.health = Some(465);
        state.character.health_max = Some(465);
        state.character.mana = Some(71);
        state.character.mana_max = Some(71);
        state.character.movement = Some(176);
        state.character.movement_max = Some(176);
        state.character.level = Some(57);
        state.character.strength = Some(20);
        state.character.mystic_level = Some(57);
        state.character.mage_level = Some(20);
        state.character.warrior_level = Some(30);
        state.character.ranger_level = Some(25);
        state.character.willpower = Some(44);
        state.character.spirit = Some(2310);
        state.character.health_regeneration = Some(11);
        state.character.movement_regeneration = Some(15);
        let area = Rect::new(0, 0, 80, 12);
        let mut buffer = Buffer::empty(area);

        render_character_panel(
            area,
            &mut buffer,
            &state,
            &theme(),
            &GaugeConfig::default(),
            "Character",
        );

        let lines = buffer_lines(&buffer, area);
        let stats_index = lines
            .iter()
            .position(|line| line.contains("Str 20"))
            .expect("base stats row");
        assert!(lines[stats_index + 1].trim_matches(['│', ' ']).is_empty());
        assert!(lines.iter().any(|line| line.contains("Willpower 44")
            && line.contains("Spirits 2310")
            && line.contains("Health Regen 11")
            && line.contains("Movement Regen 15")));
        assert!(!lines.iter().any(|line| line.contains("Mana Regen")));
    }

    fn buffer_lines(buffer: &Buffer, area: Rect) -> Vec<String> {
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width)
                    .map(|x| {
                        buffer
                            .cell((x, y))
                            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
                            .unwrap_or(' ')
                    })
                    .collect()
            })
            .collect()
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn group_members(count: usize) -> Vec<GroupMember> {
        (1..=count)
            .map(|index| GroupMember {
                name: format!("Member{index:02}"),
                health_percent: Some(100),
                mana_percent: Some(71),
                movement_percent: Some(8),
            })
            .collect()
    }

    fn theme() -> Theme {
        Theme {
            foreground: Color::White,
            border: Color::Gray,
            title: Color::Yellow,
            accent: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            danger: Color::Red,
            muted: Color::DarkGray,
            player: Color::Cyan,
            enemy: Color::Red,
        }
    }
}
