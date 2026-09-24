use std::{
    cell::OnceCell,
    collections::{BTreeMap, HashMap, VecDeque},
};

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    config::MapRenderConfig,
    map::{DoorState, Exit, ExitFlag, MapState, RoomFlag},
    ui::{
        panels::panel,
        theme::{Theme, parse_color},
    },
};

pub fn render_map(
    area: Rect,
    buf: &mut ratatui::buffer::Buffer,
    map: &MapState,
    theme: &Theme,
    config: &MapRenderConfig,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let lines = map_lines(inner, map, theme, config);
    Paragraph::new(lines).render(inner, buf);
}

pub fn render_nearby_map(
    area: Rect,
    buf: &mut ratatui::buffer::Buffer,
    map: &MapState,
    theme: &Theme,
    config: &MapRenderConfig,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let lines = nearby_map_lines(inner, map, theme, config);
    Paragraph::new(lines).render(inner, buf);
}

pub fn map_snapshot_lines(
    width: u16,
    height: u16,
    map: &MapState,
    theme: &Theme,
    config: &MapRenderConfig,
) -> Vec<String> {
    let mut lines = map_lines(Rect::new(0, 0, width, height), map, theme, config)
        .into_iter()
        .map(|line| styled_snapshot_line(line, width as usize, theme.foreground))
        .collect::<Vec<_>>();
    lines.resize_with(height as usize, || " ".repeat(width as usize));
    lines
}

fn nearby_map_lines<'a>(
    area: Rect,
    map: &MapState,
    theme: &Theme,
    config: &MapRenderConfig,
) -> Vec<Line<'a>> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let Some(current_id) = map.current_room.as_ref() else {
        return vec![Line::from(Span::styled(
            "No map",
            Style::new().fg(theme.muted),
        ))];
    };
    if !map.rooms.contains_key(current_id) {
        return vec![Line::from(Span::styled(
            "Missing room",
            Style::new().fg(theme.danger),
        ))];
    }

    let width = area.width as usize;
    let height = area.height as usize;
    let center_x = (width / 2) as i32;
    let center_y = (height / 2) as i32;
    let max_x = center_x;
    let max_y = center_y;
    let mut cells = vec![vec![' '; width]; height];
    let mut styles: BTreeMap<(usize, usize), Color> = BTreeMap::new();
    let positions = visible_positions(map, current_id);
    let markers = MapMarkers::new(map);

    for (room_id, (room_x, room_y)) in &positions {
        if room_x.abs() > max_x || room_y.abs() > max_y {
            continue;
        }
        let Some(room) = map.rooms.get(room_id) else {
            continue;
        };
        if room.flags.contains(&RoomFlag::Hide)
            || (room.flags.contains(&RoomFlag::Void) && room_id != current_id)
        {
            continue;
        }
        draw_room_marker_without_vertical_indicators(
            &mut cells,
            &mut styles,
            &markers,
            theme,
            config,
            room_id,
            (center_x + room_x, center_y + room_y),
        );
    }
    for (room_id, (room_x, room_y)) in &positions {
        if room_x.abs() > max_x || room_y.abs() > max_y {
            continue;
        }
        let Some(room) = map.rooms.get(room_id) else {
            continue;
        };
        if room.flags.contains(&RoomFlag::Hide)
            || (room.flags.contains(&RoomFlag::Void) && room_id != current_id)
        {
            continue;
        }
        draw_vertical_exit_indicators(
            &mut cells,
            &mut styles,
            room,
            theme,
            (center_x + room_x, center_y + room_y),
        );
    }

    cells
        .into_iter()
        .enumerate()
        .map(|(y, row)| {
            let spans = row
                .into_iter()
                .enumerate()
                .map(|(x, ch)| {
                    let color = styles.get(&(x, y)).copied().unwrap_or(theme.foreground);
                    Span::styled(ch.to_string(), Style::new().fg(color))
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn styled_snapshot_line(line: Line<'_>, width: usize, default_color: Color) -> String {
    let mut fitted = String::new();
    let mut used = 0;
    let mut active_color = None;
    'spans: for span in line.spans {
        let color = span.style.fg.unwrap_or(default_color);
        for value in span.content.chars() {
            let value_width = value.width().unwrap_or(0);
            if used + value_width > width {
                break 'spans;
            }
            if active_color != Some(color) {
                fitted.push_str(&ansi_foreground(color));
                active_color = Some(color);
            }
            fitted.push(value);
            used += value_width;
        }
    }
    fitted.push_str("\x1b[0m");
    let padding = width.saturating_sub(used);
    fitted.extend(std::iter::repeat_n(' ', padding));
    fitted
}

fn ansi_foreground(color: Color) -> String {
    match color {
        Color::Reset => "\x1b[39m".to_string(),
        Color::Black => "\x1b[30m".to_string(),
        Color::Red => "\x1b[31m".to_string(),
        Color::Green => "\x1b[32m".to_string(),
        Color::Yellow => "\x1b[33m".to_string(),
        Color::Blue => "\x1b[34m".to_string(),
        Color::Magenta => "\x1b[35m".to_string(),
        Color::Cyan => "\x1b[36m".to_string(),
        Color::Gray => "\x1b[37m".to_string(),
        Color::DarkGray => "\x1b[90m".to_string(),
        Color::LightRed => "\x1b[91m".to_string(),
        Color::LightGreen => "\x1b[92m".to_string(),
        Color::LightYellow => "\x1b[93m".to_string(),
        Color::LightBlue => "\x1b[94m".to_string(),
        Color::LightMagenta => "\x1b[95m".to_string(),
        Color::LightCyan => "\x1b[96m".to_string(),
        Color::White => "\x1b[97m".to_string(),
        Color::Indexed(index) => format!("\x1b[38;5;{index}m"),
        Color::Rgb(red, green, blue) => format!("\x1b[38;2;{red};{green};{blue}m"),
    }
}

fn map_lines<'a>(
    area: Rect,
    map: &MapState,
    theme: &Theme,
    config: &MapRenderConfig,
) -> Vec<Line<'a>> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let Some(current_id) = map.current_room.as_ref() else {
        return vec![Line::from(Span::styled(
            "No map. /map create",
            Style::new().fg(theme.muted),
        ))];
    };
    let Some(current) = map.rooms.get(current_id) else {
        return vec![Line::from(Span::styled(
            "Current map room is missing.",
            Style::new().fg(theme.danger),
        ))];
    };

    let width = area.width as usize;
    let height = area.height as usize;
    let mut cells = vec![vec![' '; width]; height];
    let mut styles: BTreeMap<(usize, usize), Color> = BTreeMap::new();
    let mut exit_overlays = BTreeMap::new();
    let positions = visible_positions(map, current_id);
    let markers = MapMarkers::new(map);
    let center_x = (width / 2) as i32;
    let center_y = (height / 2) as i32;

    for (room_id, (room_x, room_y)) in &positions {
        let Some(room) = map.rooms.get(room_id) else {
            continue;
        };
        if room.flags.contains(&RoomFlag::Hide) {
            continue;
        }
        let x = center_x + room_x * config.room_spacing_columns;
        let y = center_y + room_y * config.room_spacing_rows;
        if !inside(width, height, x, y) {
            continue;
        }
        draw_terrain_field(
            &mut cells,
            &mut styles,
            config,
            &room.terrain,
            room_id,
            (x, y),
        );
    }

    for (room_id, (room_x, room_y)) in &positions {
        let Some(room) = map.rooms.get(room_id) else {
            continue;
        };
        if room.flags.contains(&RoomFlag::Hide) {
            continue;
        }
        let x = center_x + room_x * config.room_spacing_columns;
        let y = center_y + room_y * config.room_spacing_rows;
        if !inside(width, height, x, y) {
            continue;
        }
        if !config.show_links {
            continue;
        }
        for exit in room.exits.values() {
            if exit.flags.contains(&ExitFlag::Hide) {
                continue;
            }
            let Some(target_id) = exit.to.as_ref() else {
                continue;
            };
            let Some(target_room) = map.rooms.get(target_id) else {
                continue;
            };
            let Some((target_x, target_y)) = positions.get(target_id) else {
                continue;
            };
            let route_link = route_terrain(&room.terrain)
                && route_terrain(&target_room.terrain)
                && target_room.z == room.z;
            let default_link_color = if route_link {
                terrain_style(config, &room.terrain)
                    .map(|style| style.color)
                    .unwrap_or(theme.foreground)
            } else {
                parse_color(&config.link_color).unwrap_or(theme.muted)
            };
            let link_color = exit_color(config, theme, room, target_room, exit, default_link_color);
            draw_connection(
                &mut cells,
                &mut styles,
                (x, y),
                (
                    center_x + target_x * config.room_spacing_columns,
                    center_y + target_y * config.room_spacing_rows,
                ),
                ConnectionStyle {
                    unicode: map.unicode,
                    color: link_color,
                    route_link,
                },
            );
            if let Some((symbol, color)) = door_marker(config, exit.door, theme) {
                let key = exit_overlay_key("door", room_id, target_id);
                let overlay = ExitOverlay {
                    from: (x, y),
                    to: (
                        center_x + target_x * config.room_spacing_columns,
                        center_y + target_y * config.room_spacing_rows,
                    ),
                    symbol,
                    color,
                };
                if room_id == current_id || !exit_overlays.contains_key(&key) {
                    exit_overlays.insert(key, overlay);
                }
            }
            if let Some((symbol, color)) = teleport_marker(config, exit, theme) {
                let key = exit_overlay_key("teleport", room_id, target_id);
                let overlay = ExitOverlay {
                    from: (x, y),
                    to: (
                        center_x + target_x * config.room_spacing_columns,
                        center_y + target_y * config.room_spacing_rows,
                    ),
                    symbol,
                    color,
                };
                if room_id == current_id || !exit_overlays.contains_key(&key) {
                    exit_overlays.insert(key, overlay);
                }
            }
        }
    }

    for overlay in exit_overlays.into_values() {
        draw_exit_overlay(&mut cells, &mut styles, overlay);
    }

    for (room_id, (room_x, room_y)) in &positions {
        if room_id != current_id {
            draw_room_marker(
                &mut cells,
                &mut styles,
                &markers,
                theme,
                config,
                room_id,
                (
                    center_x + room_x * config.room_spacing_columns,
                    center_y + room_y * config.room_spacing_rows,
                ),
            );
        }
    }
    if let Some((room_x, room_y)) = positions.get(current_id) {
        draw_room_marker(
            &mut cells,
            &mut styles,
            &markers,
            theme,
            config,
            current_id,
            (
                center_x + room_x * config.room_spacing_columns,
                center_y + room_y * config.room_spacing_rows,
            ),
        );
    }

    let mut lines: Vec<Line<'a>> = cells
        .into_iter()
        .enumerate()
        .map(|(y, row)| {
            let spans = row
                .into_iter()
                .enumerate()
                .map(|(x, ch)| {
                    let color = styles.get(&(x, y)).copied().unwrap_or(theme.foreground);
                    Span::styled(ch.to_string(), Style::new().fg(color))
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();

    if height > 1
        && let Some(first) = lines.first_mut()
    {
        let title = map_header(current);
        *first = Line::from(Span::styled(
            truncate(&title, width),
            Style::new().fg(theme.title),
        ));
    }
    lines
}

fn map_header(current: &crate::map::Room) -> String {
    let exits = if current.exits.is_empty() {
        "--".to_string()
    } else {
        let labels = current
            .exit_order
            .iter()
            .filter(|direction| current.exits.contains_key(*direction))
            .map(|direction| direction.to_ascii_uppercase())
            .collect::<Vec<_>>();
        if labels.is_empty() {
            current
                .exits
                .keys()
                .map(|direction| direction.to_ascii_uppercase())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            labels.join(" ")
        }
    };
    format!(" {}    Exits: {exits}", current.name_or_default())
}

fn draw_room_marker(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    markers: &MapMarkers<'_>,
    theme: &Theme,
    config: &MapRenderConfig,
    room_id: &str,
    position: (i32, i32),
) {
    draw_room_marker_internal(
        cells, styles, markers, theme, config, room_id, position, true,
    );
}

fn draw_room_marker_without_vertical_indicators(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    markers: &MapMarkers<'_>,
    theme: &Theme,
    config: &MapRenderConfig,
    room_id: &str,
    position: (i32, i32),
) {
    draw_room_marker_internal(
        cells, styles, markers, theme, config, room_id, position, false,
    );
}

fn draw_room_marker_internal(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    markers: &MapMarkers<'_>,
    theme: &Theme,
    config: &MapRenderConfig,
    room_id: &str,
    position: (i32, i32),
    show_vertical_indicators: bool,
) {
    let map = markers.map;
    let Some(room) = map.rooms.get(room_id) else {
        return;
    };
    if room.flags.contains(&RoomFlag::Hide)
        || !inside(
            cells.first().map_or(0, Vec::len),
            cells.len(),
            position.0,
            position.1,
        )
    {
        return;
    }
    let is_current = map.current_room.as_deref() == Some(room_id);
    if room.flags.contains(&RoomFlag::Void) && !is_current {
        return;
    }
    let (symbol, color) = if is_current {
        (
            config.current_room_symbol.clone(),
            parse_color(&config.current_room_color).unwrap_or(theme.accent),
        )
    } else if !room.symbol.is_empty() {
        (
            room.symbol.clone(),
            room_flag_color(config, theme, room).unwrap_or(theme.foreground),
        )
    } else if route_terrain(&room.terrain) {
        let color = terrain_style(config, &room.terrain)
            .map(|style| style.color)
            .unwrap_or(theme.foreground);
        (
            markers.route_symbol(room_id).to_string(),
            room_flag_color(config, theme, room).unwrap_or(color),
        )
    } else if let Some(style) = terrain_style(config, &room.terrain) {
        (
            style.symbol,
            room_flag_color(config, theme, room).unwrap_or(style.color),
        )
    } else {
        (
            config.stub_symbol.clone(),
            room_flag_color(config, theme, room).unwrap_or(theme.muted),
        )
    };
    put_text(cells, styles, position.0, position.1, &symbol, color);
    if show_vertical_indicators {
        draw_vertical_exit_indicators(cells, styles, room, theme, position);
    }
    if map.show_vnums {
        put_text(
            cells,
            styles,
            position.0.saturating_add(1),
            position.1,
            &room.id,
            theme.muted,
        );
    }
}

fn draw_vertical_exit_indicators(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    room: &crate::map::Room,
    theme: &Theme,
    position: (i32, i32),
) {
    let mut offset = 1;
    if room.exits.contains_key("u") {
        put_char(
            cells,
            styles,
            position.0 + offset,
            position.1,
            '↑',
            theme.warning,
        );
        offset += 1;
    }
    if room.exits.contains_key("d") {
        put_char(
            cells,
            styles,
            position.0 + offset,
            position.1,
            '↓',
            theme.warning,
        );
    }
}

fn route_terrain(terrain: &str) -> bool {
    matches!(
        terrain.trim().to_ascii_lowercase().as_str(),
        "road" | "city"
    )
}

// Build incoming route connections at most once per pane, only if a visible
// marker needs them. Borrow IDs for this frame without mutating map state.
struct MapMarkers<'a> {
    map: &'a MapState,
    incoming_routes: OnceCell<HashMap<&'a str, usize>>,
}

impl<'a> MapMarkers<'a> {
    fn new(map: &'a MapState) -> Self {
        Self {
            map,
            incoming_routes: OnceCell::new(),
        }
    }

    fn incoming_routes(&self) -> &HashMap<&'a str, usize> {
        self.incoming_routes.get_or_init(|| {
            let map = self.map;
            let mut incoming_routes = HashMap::new();
            for (source_id, source) in &map.rooms {
                if !route_terrain(&source.terrain) {
                    continue;
                }
                for exit in source.exits.values() {
                    let Some(target_id) = exit.to.as_deref() else {
                        continue;
                    };
                    let Some(target) = map.rooms.get(target_id) else {
                        continue;
                    };
                    if source_id == target_id || source.z != target.z {
                        continue;
                    }
                    if let Some(bit) = opposite_direction(&exit.direction).and_then(direction_bit) {
                        *incoming_routes.entry(target_id).or_insert(0) |= bit;
                    }
                }
            }
            incoming_routes
        })
    }

    fn route_symbol(&self, room_id: &str) -> char {
        let map = self.map;
        const NESW_LINE: [char; 16] = [
            '∘', '╹', '╺', '┗', '╻', '┃', '┏', '┣', '╸', '┛', '━', '┻', '┓', '┫', '┳', '╋',
        ];
        let Some(room) = map.rooms.get(room_id) else {
            return NESW_LINE[0];
        };
        let outgoing_mask = room
            .exits
            .values()
            .filter_map(|exit| {
                let target = exit
                    .to
                    .as_ref()
                    .and_then(|target_id| map.rooms.get(target_id))?;
                if !route_terrain(&target.terrain) || target.z != room.z {
                    return None;
                }
                match exit.direction.as_str() {
                    "n" => Some(1),
                    "e" => Some(2),
                    "s" => Some(4),
                    "w" => Some(8),
                    _ => None,
                }
            })
            .fold(0, |mask, direction| mask | direction);
        let incoming_mask = self.incoming_routes().get(room_id).copied().unwrap_or(0);
        let mask = outgoing_mask | incoming_mask;
        NESW_LINE[mask]
    }
}

fn direction_bit(direction: &str) -> Option<usize> {
    match direction {
        "n" => Some(1),
        "e" => Some(2),
        "s" => Some(4),
        "w" => Some(8),
        _ => None,
    }
}

fn opposite_direction(direction: &str) -> Option<&'static str> {
    match direction {
        "n" | "north" => Some("s"),
        "e" | "east" => Some("w"),
        "s" | "south" => Some("n"),
        "w" | "west" => Some("e"),
        _ => None,
    }
}

fn room_flag_color(
    config: &MapRenderConfig,
    theme: &Theme,
    room: &crate::map::Room,
) -> Option<Color> {
    if room.flags.contains(&RoomFlag::Invis) {
        Some(parse_color(&config.invis_color).unwrap_or(theme.muted))
    } else if room.flags.contains(&RoomFlag::Hide) {
        Some(parse_color(&config.hide_color).unwrap_or(theme.muted))
    } else if room.flags.contains(&RoomFlag::Avoid) {
        Some(parse_color(&config.avoid_color).unwrap_or(theme.warning))
    } else if room.flags.contains(&RoomFlag::Block) {
        Some(parse_color(&config.block_color).unwrap_or(theme.danger))
    } else if room.flags.contains(&RoomFlag::Fog) {
        Some(parse_color(&config.fog_color).unwrap_or(theme.accent))
    } else if room.flags.contains(&RoomFlag::Void) {
        Some(parse_color(&config.void_color).unwrap_or(theme.muted))
    } else {
        None
    }
}

fn exit_color(
    config: &MapRenderConfig,
    theme: &Theme,
    from: &crate::map::Room,
    to: &crate::map::Room,
    exit: &Exit,
    default_color: Color,
) -> Color {
    if exit.flags.contains(&ExitFlag::Invis) || to.flags.contains(&RoomFlag::Invis) {
        parse_color(&config.invis_color).unwrap_or(theme.muted)
    } else if exit.flags.contains(&ExitFlag::Hide) || to.flags.contains(&RoomFlag::Hide) {
        parse_color(&config.hide_color).unwrap_or(theme.muted)
    } else if exit.flags.contains(&ExitFlag::Avoid) || to.flags.contains(&RoomFlag::Avoid) {
        parse_color(&config.avoid_color).unwrap_or(theme.warning)
    } else if exit.flags.contains(&ExitFlag::Block) || to.flags.contains(&RoomFlag::Block) {
        parse_color(&config.block_color).unwrap_or(theme.danger)
    } else if to.flags.contains(&RoomFlag::Fog) {
        parse_color(&config.fog_color).unwrap_or(theme.accent)
    } else if from.flags.contains(&RoomFlag::Void) || to.flags.contains(&RoomFlag::Void) {
        parse_color(&config.void_color).unwrap_or(theme.muted)
    } else {
        default_color
    }
}

fn door_marker(
    config: &MapRenderConfig,
    door: Option<DoorState>,
    theme: &Theme,
) -> Option<(String, Color)> {
    if !config.doors.show || config.doors.glyph.is_empty() {
        return None;
    }
    let color = match door? {
        DoorState::Open => &config.doors.open_color,
        DoorState::Closed => &config.doors.closed_color,
        DoorState::Pickable => &config.doors.pickable_color,
        DoorState::Locked => &config.doors.locked_color,
        DoorState::Trigger => &config.doors.trigger_color,
        DoorState::Unknown => &config.doors.unknown_color,
    };
    Some((
        config.doors.glyph.clone(),
        parse_color(color).unwrap_or(theme.foreground),
    ))
}

fn teleport_marker(
    config: &MapRenderConfig,
    exit: &Exit,
    theme: &Theme,
) -> Option<(String, Color)> {
    if !config.teleport.show
        || config.teleport.glyph.is_empty()
        || !exit.flags.contains(&ExitFlag::Teleport)
    {
        return None;
    }
    Some((
        config.teleport.glyph.clone(),
        parse_color(&config.teleport.color).unwrap_or(theme.accent),
    ))
}

#[derive(Debug, Clone)]
struct TerrainStyle {
    symbol: String,
    color: Color,
}

struct ConnectionStyle {
    unicode: bool,
    color: Color,
    route_link: bool,
}

struct ExitOverlay {
    from: (i32, i32),
    to: (i32, i32),
    symbol: String,
    color: Color,
}

fn exit_overlay_key(kind: &str, from: &str, to: &str) -> (String, String, String) {
    if from <= to {
        (kind.to_string(), from.to_string(), to.to_string())
    } else {
        (kind.to_string(), to.to_string(), from.to_string())
    }
}

fn terrain_style(config: &MapRenderConfig, terrain: &str) -> Option<TerrainStyle> {
    let normalized = terrain.trim().to_ascii_lowercase();
    let style = config
        .terrain
        .iter()
        .find(|(name, _)| name.trim().to_ascii_lowercase() == normalized)
        .map(|(_, style)| style)?;
    Some(TerrainStyle {
        symbol: style.symbol.clone(),
        color: parse_color(&style.color).unwrap_or(Color::Gray),
    })
}

fn draw_terrain_field(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    config: &MapRenderConfig,
    terrain: &str,
    room_id: &str,
    center: (i32, i32),
) {
    let Some(style) = terrain_style(config, terrain) else {
        return;
    };
    let Some(raw_style) = terrain_config(config, terrain) else {
        return;
    };
    if raw_style.density.trim().is_empty() {
        return;
    }
    let spread = terrain_spread(&raw_style.spread);
    for dy in -spread..=spread {
        for dx in -spread..=spread {
            if dx == 0 && dy == 0 {
                continue;
            }
            let distance = dx.abs().max(dy.abs());
            let threshold =
                terrain_threshold(&raw_style.density, &raw_style.fade, distance, spread);
            if terrain_hash(room_id, dx, dy) % 100 >= threshold {
                continue;
            }
            if raw_style.double {
                put_text(
                    cells,
                    styles,
                    center.0 + dx,
                    center.1 + dy,
                    &style.symbol,
                    style.color,
                );
            } else if let Some(symbol) = style.symbol.chars().next() {
                put_char(
                    cells,
                    styles,
                    center.0 + dx,
                    center.1 + dy,
                    symbol,
                    style.color,
                );
            }
        }
    }
}

fn terrain_config<'a>(
    config: &'a MapRenderConfig,
    terrain: &str,
) -> Option<&'a crate::config::MapTerrainConfig> {
    let normalized = terrain.trim().to_ascii_lowercase();
    config
        .terrain
        .iter()
        .find(|(name, _)| name.trim().to_ascii_lowercase() == normalized)
        .map(|(_, style)| style)
}

fn terrain_spread(spread: &str) -> i32 {
    match spread.trim().to_ascii_lowercase().as_str() {
        "narrow" => 1,
        "wide" => 3,
        "vast" => 5,
        _ => 2,
    }
}

fn terrain_threshold(density: &str, fade: &str, distance: i32, spread: i32) -> u64 {
    let base = match density.trim().to_ascii_lowercase().as_str() {
        "dense" => 82,
        "sparse" => 42,
        "scant" => 18,
        _ => 62,
    };
    let fade_step = if spread <= 0 { 0 } else { 40 / spread as u64 };
    match fade.trim().to_ascii_lowercase().as_str() {
        "fadein" => (base / 2 + distance.max(0) as u64 * fade_step).min(96),
        "fadeout" => base
            .saturating_sub(distance.max(0) as u64 * fade_step)
            .max(8),
        _ => base,
    }
}

fn terrain_hash(room_id: &str, dx: i32, dy: i32) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in room_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash ^= dx as u64;
    hash = hash.wrapping_mul(1_099_511_628_211);
    hash ^= dy as u64;
    hash = hash.wrapping_mul(1_099_511_628_211);
    hash
}

fn visible_positions(map: &MapState, current_id: &str) -> BTreeMap<String, (i32, i32)> {
    let mut positions = BTreeMap::from([(current_id.to_string(), (0, 0))]);
    let mut occupied = BTreeMap::from([((0, 0), current_id.to_string())]);
    let mut queue = VecDeque::from([VisibleNode {
        room_id: current_id.to_string(),
        from_id: None,
        incoming_direction: None,
    }]);
    let Some(current_z) = map.rooms.get(current_id).map(|room| room.z) else {
        return positions;
    };

    while let Some(node) = queue.pop_front() {
        let room_id = node.room_id;
        let Some(room) = map.rooms.get(&room_id) else {
            continue;
        };
        if room.flags.contains(&RoomFlag::Fog) && room_id != current_id {
            continue;
        }
        let Some((x, y)) = positions.get(&room_id).copied() else {
            continue;
        };
        for exit in traversable_exits(
            room,
            node.from_id.as_deref(),
            node.incoming_direction.as_deref(),
        ) {
            if exit.flags.contains(&ExitFlag::Hide) {
                continue;
            }
            let Some(target_id) = exit.to.as_ref() else {
                continue;
            };
            let Some(target_room) = map.rooms.get(target_id) else {
                continue;
            };
            if target_room.flags.contains(&RoomFlag::Hide) {
                continue;
            }
            if positions.contains_key(target_id) {
                continue;
            }
            let position = if target_room.z == current_z {
                let Some((dx, dy)) = direction_offset(&exit.direction) else {
                    continue;
                };
                Some((x + dx, y + dy))
            } else if is_vertical_direction(&exit.direction) {
                None
            } else {
                continue;
            };
            if let Some(mut position) = position {
                if room.flags.contains(&RoomFlag::Void)
                    && !target_room.flags.contains(&RoomFlag::Void)
                    && occupied.contains_key(&position)
                    && let Some((dx, dy)) = direction_offset(&exit.direction)
                {
                    position = next_free_position(position, (dx, dy), &occupied);
                }
                if occupied.contains_key(&position) && !target_room.flags.contains(&RoomFlag::Void)
                {
                    continue;
                }
                occupied
                    .entry(position)
                    .or_insert_with(|| target_id.clone());
                positions.insert(target_id.clone(), position);
                queue.push_back(VisibleNode {
                    room_id: target_id.clone(),
                    from_id: Some(room_id.clone()),
                    incoming_direction: Some(exit.direction.clone()),
                });
            }
        }
    }

    positions
}

fn next_free_position(
    mut position: (i32, i32),
    direction: (i32, i32),
    occupied: &BTreeMap<(i32, i32), String>,
) -> (i32, i32) {
    for _ in 0..16 {
        if !occupied.contains_key(&position) {
            return position;
        }
        position = (position.0 + direction.0, position.1 + direction.1);
    }
    position
}

#[derive(Debug, Clone)]
struct VisibleNode {
    room_id: String,
    from_id: Option<String>,
    incoming_direction: Option<String>,
}

fn traversable_exits<'a>(
    room: &'a crate::map::Room,
    from_id: Option<&str>,
    incoming_direction: Option<&str>,
) -> Vec<&'a Exit> {
    if !room.flags.contains(&RoomFlag::Void) || incoming_direction.is_none() {
        return room.exits.values().collect();
    }
    if let Some(exit) = incoming_direction.and_then(|direction| room.exits.get(direction)) {
        return vec![exit];
    }
    let outbound = room
        .exits
        .values()
        .filter(|exit| exit.to.as_deref() != from_id)
        .collect::<Vec<_>>();
    if room.exits.len() == 2 && outbound.len() == 1 {
        outbound
    } else {
        room.exits.values().collect()
    }
}

fn direction_offset(direction: &str) -> Option<(i32, i32)> {
    match direction {
        "n" | "north" => Some((0, -1)),
        "ne" | "northeast" => Some((1, -1)),
        "e" | "east" => Some((1, 0)),
        "se" | "southeast" => Some((1, 1)),
        "s" | "south" => Some((0, 1)),
        "sw" | "southwest" => Some((-1, 1)),
        "w" | "west" => Some((-1, 0)),
        "nw" | "northwest" => Some((-1, -1)),
        _ => None,
    }
}

fn is_vertical_direction(direction: &str) -> bool {
    matches!(direction, "u" | "up" | "d" | "down")
}

fn draw_connection(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    from: (i32, i32),
    to: (i32, i32),
    connection: ConnectionStyle,
) {
    let distance_x = to.0 - from.0;
    let distance_y = to.1 - from.1;
    let dx = distance_x.signum();
    let dy = distance_y.signum();
    let steps = distance_x.abs().max(distance_y.abs());
    if steps <= 1 {
        return;
    }
    for step in 1..steps {
        let x = from.0 + distance_x * step / steps;
        let y = from.1 + distance_y * step / steps;
        let ch = match (dx, dy, connection.unicode, connection.route_link) {
            (0, _, true, true) => '┃',
            (_, 0, true, true) => '━',
            (0, _, true, false) => '│',
            (_, 0, true, false) => '─',
            (_, _, true, _) if dx == dy => '＼',
            (_, _, true, _) => '／',
            (0, _, false, _) => '|',
            (_, 0, false, _) => '-',
            (_, _, false, _) if dx == dy => '\\',
            _ => '/',
        };
        put_char(cells, styles, x, y, ch, connection.color);
    }
}

fn draw_exit_overlay(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    overlay: ExitOverlay,
) {
    let x = overlay.from.0 + (overlay.to.0 - overlay.from.0) / 2;
    let y = overlay.from.1 + (overlay.to.1 - overlay.from.1) / 2;
    put_text(cells, styles, x, y, &overlay.symbol, overlay.color);
}

fn put_text(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    x: i32,
    y: i32,
    text: &str,
    color: Color,
) {
    for (offset, ch) in text.chars().enumerate() {
        put_char(cells, styles, x + offset as i32, y, ch, color);
    }
}

fn put_char(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    x: i32,
    y: i32,
    ch: char,
    color: Color,
) {
    if !inside(cells.first().map_or(0, Vec::len), cells.len(), x, y) {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    cells[y][x] = ch;
    styles.insert((x, y), color);
}

fn inside(width: usize, height: usize, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height
}

fn truncate(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        config::MapRenderConfig,
        map::{Exit, MapState, Room},
        state::plain_text,
    };
    use unicode_width::UnicodeWidthStr;

    use super::*;

    #[test]
    fn snapshot_fills_requested_dimensions_without_a_map() {
        let lines = map_snapshot_lines(20, 6, &MapState::default(), &theme(), &map_config());

        assert_eq!(lines.len(), 6);
        assert!(
            lines
                .iter()
                .all(|line| UnicodeWidthStr::width(plain_text(line).as_str()) == 20)
        );
        assert!(lines[0].contains("No map"));
    }

    #[test]
    fn snapshot_clips_wide_content_to_requested_width() {
        let line = styled_snapshot_line(
            Line::from(Span::styled(
                "123456789界",
                Style::new().fg(Color::Rgb(1, 2, 3)),
            )),
            10,
            Color::White,
        );
        let plain = plain_text(&line);

        assert_eq!(UnicodeWidthStr::width(plain.as_str()), 10);
        assert!(!plain.contains('界'));
        assert!(line.contains("\x1b[38;2;1;2;3m"));
    }

    #[test]
    fn snapshot_preserves_named_indexed_and_rgb_colors() {
        let line = Line::from(vec![
            Span::styled("A", Style::new().fg(Color::Red)),
            Span::styled("B", Style::new().fg(Color::Indexed(123))),
            Span::styled("C", Style::new().fg(Color::Rgb(4, 5, 6))),
        ]);

        let snapshot = styled_snapshot_line(line, 3, Color::White);

        assert_eq!(plain_text(&snapshot), "ABC");
        assert!(snapshot.contains("\x1b[31m"));
        assert!(snapshot.contains("\x1b[38;5;123m"));
        assert!(snapshot.contains("\x1b[38;2;4;5;6m"));
    }

    #[test]
    fn one_row_snapshot_preserves_current_room_marker() {
        let mut map = MapState::default();
        map.create();

        let lines = map_snapshot_lines(9, 1, &map, &theme(), &map_config());

        assert_eq!(lines.len(), 1);
        assert_eq!(plain_text(&lines[0]).chars().nth(4), Some('X'));
    }

    #[test]
    fn map_lines_render_current_room() {
        let mut map = MapState::default();
        map.create();
        let room = map.rooms.get_mut("1").expect("room should exist");
        for direction in ["w", "e", "s"] {
            room.exits.insert(
                direction.to_string(),
                Exit {
                    direction: direction.to_string(),
                    ..Exit::default()
                },
            );
        }
        room.exit_order = vec!["w".to_string(), "e".to_string(), "s".to_string()];
        let theme = Theme {
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
            ..Theme::from_config(&crate::config::ThemeConfig::default())
        };

        let lines = map_lines(Rect::new(0, 0, 32, 5), &map, &theme, &map_config());

        assert_eq!(lines.len(), 5);
        let rendered = plain_lines(lines);
        assert!(rendered[0].contains("Room 1"));
        assert!(rendered[0].contains("Exits: W E S"));
        assert!(!rendered[0].contains("1 Room 1"));
    }

    #[test]
    fn draw_connection_handles_non_square_offsets() {
        let mut cells = vec![vec![' '; 8]; 5];
        let mut styles = BTreeMap::new();

        draw_connection(
            &mut cells,
            &mut styles,
            (1, 1),
            (6, 3),
            ConnectionStyle {
                unicode: true,
                color: Color::Gray,
                route_link: false,
            },
        );

        assert!(!styles.is_empty());
    }

    #[test]
    fn current_room_marker_is_drawn_over_connections() {
        let mut map = MapState::default();
        map.current_room = Some("6945".to_string());
        map.rooms.insert(
            "6945".to_string(),
            room(
                "6945",
                "A Shadowy Forest",
                10,
                10,
                [("s", "6948"), ("w", "6944")],
            ),
        );
        map.rooms
            .insert("6948".to_string(), room("6948", "", 10, 10, []));
        map.rooms
            .insert("6944".to_string(), room("6944", "", 10, 10, []));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));
        let center_x = 10;
        let center_y = 4;

        assert_eq!(lines[center_y].chars().nth(center_x), Some('X'));
        assert_eq!(lines[center_y].chars().nth(center_x - 3), Some('∘'));
        assert_eq!(lines[center_y + 2].chars().nth(center_x), Some('∘'));
    }

    #[test]
    fn current_room_marker_is_drawn_after_overlapping_rooms() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Center", 0, 0, [("e", "2"), ("w", "3")]),
        );
        map.rooms
            .insert("2".to_string(), room("2", "", 0, 0, [("w", "5")]));
        map.rooms
            .insert("3".to_string(), room("3", "", 0, 0, [("e", "6")]));
        map.rooms.insert("5".to_string(), room("5", "", 0, 0, []));
        map.rooms.insert("6".to_string(), room("6", "", 0, 0, []));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(10), Some('X'));
    }

    #[test]
    fn visible_positions_hide_overlapping_rooms_after_first_occupant() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Center", 0, 0, [("e", "2"), ("w", "3")]),
        );
        map.rooms
            .insert("2".to_string(), room("2", "", 0, 0, [("w", "5")]));
        map.rooms
            .insert("3".to_string(), room("3", "", 0, 0, [("e", "6")]));
        map.rooms.insert("5".to_string(), room("5", "", 0, 0, []));
        map.rooms.insert("6".to_string(), room("6", "", 0, 0, []));

        let positions = visible_positions(&map, "1");

        assert_eq!(positions.get("1"), Some(&(0, 0)));
        assert_eq!(positions.get("2"), Some(&(1, 0)));
        assert_eq!(positions.get("3"), Some(&(-1, 0)));
        assert!(!positions.contains_key("5"));
        assert!(!positions.contains_key("6"));
    }

    #[test]
    fn visible_positions_do_not_traverse_hidden_overlapping_rooms() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Center", 0, 0, [("e", "2"), ("w", "3")]),
        );
        map.rooms
            .insert("2".to_string(), room("2", "", 0, 0, [("w", "5")]));
        map.rooms.insert("3".to_string(), room("3", "", 0, 0, []));
        map.rooms
            .insert("5".to_string(), room("5", "", 0, 0, [("n", "7")]));
        map.rooms.insert("7".to_string(), room("7", "", 0, 0, []));

        let positions = visible_positions(&map, "1");

        assert!(!positions.contains_key("5"));
        assert!(!positions.contains_key("7"));
    }

    #[test]
    fn visible_positions_skip_hidden_exits() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms.insert("2".to_string(), room("2", "", 0, 0, []));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("e")
            .unwrap()
            .flags
            .insert(ExitFlag::Hide);

        let positions = visible_positions(&map, "1");

        assert!(!positions.contains_key("2"));
    }

    #[test]
    fn visible_positions_stop_after_fog_rooms() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms
            .insert("2".to_string(), room("2", "", 0, 0, [("e", "3")]));
        map.rooms.insert("3".to_string(), room("3", "", 0, 0, []));
        map.rooms.get_mut("2").unwrap().flags.insert(RoomFlag::Fog);

        let positions = visible_positions(&map, "1");

        assert!(positions.contains_key("2"));
        assert!(!positions.contains_key("3"));
    }

    #[test]
    fn visible_positions_allow_void_rooms_to_tunnel_through_overlap() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Center", 0, 0, [("e", "2"), ("w", "3")]),
        );
        map.rooms
            .insert("2".to_string(), room("2", "", 0, 0, [("w", "5")]));
        map.rooms
            .insert("3".to_string(), room("3", "", 0, 0, [("e", "6")]));
        map.rooms
            .insert("5".to_string(), room("5", "", 0, 0, [("w", "7")]));
        map.rooms.insert("6".to_string(), room("6", "", 0, 0, []));
        map.rooms.insert("7".to_string(), room("7", "", 0, 0, []));
        map.rooms.get_mut("5").unwrap().flags.insert(RoomFlag::Void);

        let positions = visible_positions(&map, "1");

        assert_eq!(positions.get("5"), Some(&(0, 0)));
        assert_eq!(positions.get("7"), Some(&(-2, 0)));
        assert!(!positions.contains_key("6"));
    }

    #[test]
    fn visible_positions_follow_msdp_exit_directions_not_stale_coordinates() {
        let mut map = MapState::default();
        map.current_room = Some("6945".to_string());
        map.rooms.insert(
            "6945".to_string(),
            room("6945", "A Shadowy Forest", 50, 50, [("s", "6948")]),
        );
        map.rooms
            .insert("6948".to_string(), room("6948", "Above A Gully", 1, 1, []));

        assert_eq!(visible_positions(&map, "6945").get("6948"), Some(&(0, 1)));
    }

    #[test]
    fn visible_positions_hide_rooms_on_other_z_layers() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room(
                "1",
                "Lower",
                0,
                0,
                [("n", "2"), ("e", "3"), ("w", "4"), ("u", "5")],
            ),
        );
        map.rooms
            .insert("2".to_string(), room("2", "North", 0, 0, []));
        map.rooms
            .insert("3".to_string(), room("3", "East", 0, 0, []));
        map.rooms
            .insert("4".to_string(), room("4", "West", 0, 0, []));
        map.rooms.insert(
            "5".to_string(),
            Room {
                z: 1,
                ..room("5", "Upper", 0, 0, [("s", "6"), ("d", "1")])
            },
        );
        map.rooms.insert(
            "6".to_string(),
            Room {
                z: 1,
                ..room("6", "Upper South", 0, 0, [])
            },
        );

        let positions = visible_positions(&map, "1");

        assert!(positions.contains_key("1"));
        assert!(positions.contains_key("2"));
        assert!(positions.contains_key("3"));
        assert!(positions.contains_key("4"));
        assert!(!positions.contains_key("5"));
        assert!(!positions.contains_key("6"));
    }

    #[test]
    fn map_lines_render_up_and_down_exit_indicators() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Tower Landing", 0, 0, [("u", "2"), ("d", "3")]),
        );
        map.rooms
            .insert("2".to_string(), room("2", "Upper Landing", 0, 0, []));
        map.rooms
            .insert("3".to_string(), room("3", "Lower Landing", 0, 0, []));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(10), Some('X'));
        assert_eq!(lines[4].chars().nth(11), Some('↑'));
        assert_eq!(lines[4].chars().nth(12), Some('↓'));
    }

    #[test]
    fn visible_positions_show_connected_rooms_on_current_z_after_moving_up() {
        let mut map = MapState::default();
        map.current_room = Some("5".to_string());
        map.rooms.insert(
            "1".to_string(),
            room(
                "1",
                "Lower",
                0,
                0,
                [("n", "2"), ("e", "3"), ("w", "4"), ("u", "5")],
            ),
        );
        map.rooms
            .insert("2".to_string(), room("2", "North", 0, 0, []));
        map.rooms
            .insert("3".to_string(), room("3", "East", 0, 0, []));
        map.rooms
            .insert("4".to_string(), room("4", "West", 0, 0, []));
        map.rooms.insert(
            "5".to_string(),
            Room {
                z: 1,
                ..room("5", "Upper", 0, 0, [("s", "6"), ("d", "1")])
            },
        );
        map.rooms.insert(
            "6".to_string(),
            Room {
                z: 1,
                ..room("6", "Upper South", 0, 0, [])
            },
        );

        let positions = visible_positions(&map, "5");

        assert!(positions.contains_key("5"));
        assert_eq!(positions.get("6"), Some(&(0, 1)));
        assert!(!positions.contains_key("1"));
        assert!(!positions.contains_key("2"));
        assert!(!positions.contains_key("3"));
        assert!(!positions.contains_key("4"));
    }

    #[test]
    fn map_lines_render_tintin_terrain_symbols() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            room("1", "Center", 0, 0, [("e", "2"), ("s", "3")]),
        );
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("2", "Forest", 0, 0, [])
            },
        );
        map.rooms.insert(
            "3".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("3", "Road", 0, 0, [])
            },
        );

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(10), Some('X'));
        assert_eq!(lines[4].chars().nth(13), Some('♣'));
        assert_eq!(lines[6].chars().nth(10), Some('∘'));
    }

    #[test]
    fn road_and_city_rooms_render_route_line_symbols() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room(
                    "1",
                    "Center Road",
                    0,
                    0,
                    [("n", "2"), ("e", "3"), ("s", "4"), ("w", "5")],
                )
            },
        );
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("2", "North Road", 0, 0, [])
            },
        );
        map.rooms.insert(
            "3".to_string(),
            Room {
                terrain: "City".to_string(),
                ..room("3", "East City", 0, 0, [])
            },
        );
        map.rooms.insert(
            "4".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("4", "South Road", 0, 0, [])
            },
        );
        map.rooms.insert(
            "5".to_string(),
            Room {
                terrain: "City".to_string(),
                ..room("5", "West City", 0, 0, [])
            },
        );

        assert_eq!(MapMarkers::new(&map).route_symbol("1"), '╋');
    }

    #[test]
    fn road_and_city_links_render_as_thick_route_lines() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("1", "Road", 0, 0, [("e", "2")])
            },
        );
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "City".to_string(),
                ..room("2", "City", 0, 0, [])
            },
        );

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(10), Some('X'));
        assert_eq!(lines[4].chars().nth(11), Some('━'));
        assert_eq!(lines[4].chars().nth(12), Some('━'));
        assert_eq!(lines[4].chars().nth(13), Some('╸'));
    }

    #[test]
    fn trigger_doors_render_on_non_route_links() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("e")
            .unwrap()
            .door = Some(DoorState::Trigger);
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("2", "Forest", 0, 0, [])
            },
        );

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(11), Some('╬'));
        assert_eq!(lines[4].chars().nth(13), Some('♣'));
    }

    #[test]
    fn teleport_exit_flags_render_teleport_marker() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("e")
            .unwrap()
            .flags
            .insert(ExitFlag::Teleport);
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("2", "Forest", 0, 0, [])
            },
        );

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(11), Some('◇'));
    }

    #[test]
    fn door_marker_survives_reverse_link_redraw() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("w", "2")]));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("w")
            .unwrap()
            .door = Some(DoorState::Closed);
        map.rooms
            .insert("2".to_string(), room("2", "West", 0, 0, [("e", "1")]));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(9), Some('╬'));
    }

    #[test]
    fn shared_door_display_prefers_current_room_directional_data() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("w", "2")]));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("w")
            .unwrap()
            .door = Some(DoorState::Closed);
        map.rooms
            .insert("2".to_string(), room("2", "West", 0, 0, [("e", "1")]));
        map.rooms
            .get_mut("2")
            .unwrap()
            .exits
            .get_mut("e")
            .unwrap()
            .door = Some(DoorState::Trigger);

        let lines = map_lines(Rect::new(0, 0, 21, 9), &map, &theme(), &map_config());
        let door_span = &lines[4].spans[9];

        assert_eq!(door_span.content.as_ref(), "╬");
        assert_eq!(door_span.style.fg, Some(Color::Rgb(214, 173, 85)));
    }

    #[test]
    fn closed_doors_render_on_vertical_links() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("s", "2")]));
        map.rooms
            .get_mut("1")
            .unwrap()
            .exits
            .get_mut("s")
            .unwrap()
            .door = Some(DoorState::Closed);
        map.rooms
            .insert("2".to_string(), room("2", "South", 0, 0, []));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(10), Some('X'));
        assert_eq!(lines[5].chars().nth(10), Some('╬'));
        assert_eq!(lines[6].chars().nth(10), Some('∘'));
    }

    #[test]
    fn nearby_map_omits_links_and_butts_route_symbols_together() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("1", "Center", 0, 0, [("e", "2"), ("w", "4"), ("u", "3")])
            },
        );
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("2", "Road", 0, 0, [])
            },
        );
        map.rooms.insert(
            "3".to_string(),
            Room {
                z: 1,
                ..room("3", "Up", 0, 0, [])
            },
        );
        map.rooms.insert(
            "4".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room("4", "Road", 0, 0, [])
            },
        );

        let lines = plain_lines(nearby_map_lines(
            Rect::new(0, 0, 11, 5),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[2].chars().nth(4), Some('╺'));
        assert_eq!(lines[2].chars().nth(5), Some('X'));
        assert_eq!(lines[2].chars().nth(6), Some('↑'));
        assert!(!lines.join("\n").contains('─'));
        assert!(!lines.join("\n").contains('━'));
    }

    #[test]
    fn door_markers_use_state_specific_colors() {
        let mut config = map_config();
        config.doors.trigger_color = "#ff00ff".to_string();

        let marker = door_marker(&config, Some(DoorState::Trigger), &theme()).unwrap();

        assert_eq!(marker.0, "╬");
        assert_eq!(marker.1, Color::Rgb(255, 0, 255));
    }

    #[test]
    fn incoming_route_index_preserves_direction_and_layer_rules() {
        let mut map = MapState::default();
        map.rooms
            .insert("target".into(), room("target", "", 0, 0, []));
        for (id, direction, terrain, z) in [
            ("north", "south", "Road", 0),
            ("east", "w", "City", 0),
            ("forest", "n", "Forest", 0),
            ("upper", "e", "Road", 1),
            ("vertical", "u", "Road", 0),
        ] {
            map.rooms.insert(
                id.into(),
                Room {
                    terrain: terrain.into(),
                    z,
                    ..room(id, "", 0, 0, [(direction, "target")])
                },
            );
        }
        map.rooms.insert(
            "self".into(),
            Room {
                terrain: "Road".into(),
                ..room("self", "", 0, 0, [("n", "self"), ("e", "missing")])
            },
        );
        let markers = MapMarkers::new(&map);
        assert!(markers.incoming_routes.get().is_none());
        assert_eq!(markers.route_symbol("target"), '┗');
        assert_eq!(markers.incoming_routes().get("target"), Some(&3));
        assert!(!markers.incoming_routes().contains_key("self"));
        assert!(!markers.incoming_routes().contains_key("missing"));
        assert_eq!(markers.route_symbol("self"), '╹');
        assert_eq!(markers.route_symbol("missing"), '∘');
    }

    #[test]
    fn route_line_symbols_ignore_non_route_and_other_z_rooms() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Road".to_string(),
                ..room(
                    "1",
                    "Road",
                    0,
                    0,
                    [("n", "2"), ("e", "3"), ("s", "4"), ("u", "5")],
                )
            },
        );
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("2", "Forest", 0, 0, [])
            },
        );
        map.rooms.insert(
            "3".to_string(),
            Room {
                terrain: "Road".to_string(),
                z: 1,
                ..room("3", "Upper Road", 0, 0, [])
            },
        );
        map.rooms.insert(
            "4".to_string(),
            Room {
                terrain: "City".to_string(),
                ..room("4", "South City", 0, 0, [])
            },
        );
        map.rooms.insert(
            "5".to_string(),
            Room {
                terrain: "Road".to_string(),
                z: 1,
                ..room("5", "Up Road", 0, 0, [])
            },
        );

        assert_eq!(MapMarkers::new(&map).route_symbol("1"), '╻');
    }

    #[test]
    fn map_lines_render_configured_terrain_field() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("1", "Center", 0, 0, [])
            },
        );
        let mut config = map_config();
        let forest = config.terrain.get_mut("Forest").unwrap();
        forest.symbol = "*".to_string();
        forest.density = "dense".to_string();
        forest.spread = "narrow".to_string();
        forest.fade.clear();

        let lines = plain_lines(map_lines(Rect::new(0, 0, 21, 9), &map, &theme(), &config));

        let terrain_count = lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|ch| *ch == '*')
            .count();
        assert!(terrain_count > 0);
        assert_eq!(lines[4].chars().nth(10), Some('X'));
    }

    #[test]
    fn map_lines_do_not_render_terrain_field_when_density_is_empty() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("1", "Center", 0, 0, [])
            },
        );
        let mut config = map_config();
        let forest = config.terrain.get_mut("Forest").unwrap();
        forest.symbol = "*".to_string();
        forest.density.clear();
        forest.spread = "narrow".to_string();

        let lines = plain_lines(map_lines(Rect::new(0, 0, 21, 9), &map, &theme(), &config));

        let terrain_count = lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|ch| *ch == '*')
            .count();
        assert_eq!(terrain_count, 0);
        assert_eq!(lines[4].chars().nth(10), Some('X'));
    }

    #[test]
    fn map_lines_render_double_width_terrain_symbols() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms.insert(
            "1".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("1", "Center", 0, 0, [])
            },
        );
        let mut config = map_config();
        let forest = config.terrain.get_mut("Forest").unwrap();
        forest.symbol = "%%".to_string();
        forest.density = "dense".to_string();
        forest.spread = "narrow".to_string();
        forest.double = true;

        let lines = plain_lines(map_lines(Rect::new(0, 0, 21, 9), &map, &theme(), &config));

        assert!(lines.iter().any(|line| line.contains("%%")));
    }

    #[test]
    fn map_lines_render_stub_symbol_for_unstyled_rooms() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms
            .insert("2".to_string(), room("2", "Stub", 0, 0, []));

        let lines = plain_lines(map_lines(
            Rect::new(0, 0, 21, 9),
            &map,
            &theme(),
            &map_config(),
        ));

        assert_eq!(lines[4].chars().nth(13), Some('∘'));
    }

    #[test]
    fn map_lines_use_configured_symbols() {
        let mut map = MapState::default();
        map.current_room = Some("1".to_string());
        map.rooms
            .insert("1".to_string(), room("1", "Center", 0, 0, [("e", "2")]));
        map.rooms.insert(
            "2".to_string(),
            Room {
                terrain: "Forest".to_string(),
                ..room("2", "Forest", 0, 0, [])
            },
        );
        let mut config = map_config();
        config.current_room_symbol = "@".to_string();
        config.terrain.get_mut("Forest").unwrap().symbol = "F".to_string();

        let lines = plain_lines(map_lines(Rect::new(0, 0, 21, 9), &map, &theme(), &config));

        assert_eq!(lines[4].chars().nth(10), Some('@'));
        assert_eq!(lines[4].chars().nth(13), Some('F'));
    }

    fn room<const N: usize>(
        id: &str,
        name: &str,
        x: i32,
        y: i32,
        exits: [(&str, &str); N],
    ) -> Room {
        Room {
            id: id.to_string(),
            name: name.to_string(),
            x,
            y,
            exits: exits
                .into_iter()
                .map(|(direction, target)| {
                    (
                        direction.to_string(),
                        Exit {
                            direction: direction.to_string(),
                            to: Some(target.to_string()),
                            ..Exit::default()
                        },
                    )
                })
                .collect(),
            ..Room::default()
        }
    }

    fn plain_lines(lines: Vec<Line<'_>>) -> Vec<String> {
        lines
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect()
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
            ..Theme::from_config(&crate::config::ThemeConfig::default())
        }
    }

    fn map_config() -> MapRenderConfig {
        MapRenderConfig::default()
    }
}
