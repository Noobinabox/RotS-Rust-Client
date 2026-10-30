//! Bounded screen-space routing for connections that cannot follow the compass grid.

use super::*;

type Point = (i32, i32);

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_schematic_connection(
    cells: &mut [Vec<char>],
    styles: &mut BTreeMap<(usize, usize), Color>,
    map: &MapState,
    config: &MapRenderConfig,
    theme: &Theme,
    exit: &Exit,
    endpoints: (Point, Point),
    positions: &BTreeMap<String, Point>,
    geometry: (Point, Point),
    color: Color,
) {
    let (from, to) = endpoints;
    let width = cells.first().map_or(0, Vec::len);
    let height = cells.len();
    let (center, spacing) = geometry;
    let mut blocked = std::collections::BTreeSet::new();
    let markers = MapMarkers::new(map);
    for (id, p) in positions {
        let room_center = (
            center.0.saturating_add(p.0.saturating_mul(spacing.0)),
            center.1.saturating_add(p.1.saturating_mul(spacing.1)),
        );
        let is_current = map.current_room.as_ref() == Some(id);
        let (symbol, _) =
            room_marker_style(&markers, theme, config, id, is_current, MapView::Regular);
        let symbol_width = symbol.chars().count() as i32;
        let half_width = if !is_current
            && map
                .rooms
                .get(id)
                .is_some_and(|room| !room.symbol.is_empty())
        {
            symbol_width / 2
        } else {
            0
        };
        let indicators = map.rooms.get(id).map_or(0, |room| {
            i32::from(room.exits.contains_key("u")) + i32::from(room.exits.contains_key("d"))
        });
        let vnums = if map.show_vnums {
            map.rooms
                .get(id)
                .map_or(0, |room| room.id.chars().count() as i32)
        } else {
            0
        };
        for dx in -half_width..symbol_width - half_width + indicators.max(vnums) {
            let p = (room_center.0.saturating_add(dx), room_center.1);
            if p != from && p != to {
                blocked.insert(p);
            }
        }
    }
    let mut previous = BTreeMap::from([(from, from)]);
    let mut queue = VecDeque::from([from]);
    while let Some(p) = queue.pop_front() {
        if p == to {
            break;
        }
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let next = (p.0 + dx, p.1 + dy);
            if !inside(width, height, next.0, next.1)
                || blocked.contains(&next)
                || previous.contains_key(&next)
            {
                continue;
            }
            if next != to
                && next != from
                && styles.contains_key(&(next.0 as usize, next.1 as usize))
            {
                continue;
            }
            previous.insert(next, p);
            queue.push_back(next);
        }
    }
    let label = crate::map::normalize_direction(&exit.direction);
    if previous.contains_key(&to) {
        let mut path = vec![to];
        let mut p = to;
        while p != from {
            p = previous[&p];
            path.push(p);
        }
        path.reverse();
        // A label and arrow need distinct cells, clear of endpoint markers.
        let label_position = path
            .iter()
            .copied()
            .skip(2)
            .take(path.len().saturating_sub(4))
            .find(|p| {
                (0..label.len() as i32).all(|dx| {
                    let point = (p.0 + dx, p.1);
                    point != from
                        && point != to
                        && !blocked.contains(&point)
                        && inside(width, height, point.0, point.1)
                        && !styles.contains_key(&(point.0 as usize, point.1 as usize))
                })
            });
        if path.len() > label.len() + 4
            && let Some(label_position) = label_position
        {
            for segment in path.windows(3) {
                let a = segment[0];
                let b = segment[1];
                let c = segment[2];
                let glyph = if !map.unicode {
                    if a.0 == c.0 {
                        '|'
                    } else if a.1 == c.1 {
                        '-'
                    } else {
                        '+'
                    }
                } else if a.0 == c.0 {
                    '│'
                } else if a.1 == c.1 {
                    '─'
                } else {
                    match ((a.0 - b.0, a.1 - b.1), (c.0 - b.0, c.1 - b.1)) {
                        ((-1, 0), (0, 1)) | ((0, 1), (-1, 0)) => '┐',
                        ((1, 0), (0, 1)) | ((0, 1), (1, 0)) => '┌',
                        ((-1, 0), (0, -1)) | ((0, -1), (-1, 0)) => '┘',
                        _ => '└',
                    }
                };
                put_char(cells, styles, b.0, b.1, glyph, color);
            }
            let p = label_position;
            put_text(cells, styles, p.0, p.1, &label, color);
            let p = path[path.len() - 2];
            let delta = (to.0 - p.0, to.1 - p.1);
            let arrow = match (delta, map.unicode) {
                ((1, 0), true) => '▶',
                ((-1, 0), true) => '◀',
                ((0, 1), true) => '▼',
                (_, true) => '▲',
                ((1, 0), false) => '>',
                ((-1, 0), false) => '<',
                ((0, 1), false) => 'v',
                (_, false) => '^',
            };
            let source = positions
                .iter()
                .find(|(_, point)| {
                    (
                        center.0.saturating_add(point.0.saturating_mul(spacing.0)),
                        center.1.saturating_add(point.1.saturating_mul(spacing.1)),
                    ) == from
                })
                .map(|(id, _)| id.as_str());
            if config.one_way.show
                && source
                    .zip(exit.to.as_deref())
                    .is_some_and(|(source, target)| !has_reverse_exit(map, source, target, exit))
            {
                let arrow_color = config
                    .one_way
                    .color
                    .as_deref()
                    .and_then(parse_color)
                    .map(|color| theme.readable_map_color(color))
                    .unwrap_or(color);
                put_char(cells, styles, p.0, p.1, arrow, arrow_color);
            }
            if let Some((glyph, color)) =
                teleport_marker(config, exit, theme).or_else(|| door_marker(config, exit, theme))
                && let Some(p) = path
                    .iter()
                    .skip(1)
                    .take(path.len().saturating_sub(2))
                    .find(|p| {
                        p.1 != label_position.1
                            || p.0 < label_position.0
                            || p.0 >= label_position.0 + label.len() as i32
                    })
            {
                put_text(cells, styles, p.0, p.1, &glyph, color);
            }
            return;
        }
    }
    // A cramped or obstructed route stays explicit rather than crossing rooms.
    let target = exit.to.as_deref().unwrap_or("?");
    let marker = teleport_marker(config, exit, theme).or_else(|| door_marker(config, exit, theme));
    let prefix = marker.as_ref().map_or("", |(glyph, _)| glyph.as_str());
    let label = format!("{prefix}{label}→{target}");
    let label = if map.unicode {
        label
    } else {
        label.replace('→', ">")
    };
    for y in [from.1 + 1, from.1 - 1] {
        if label.chars().enumerate().all(|(i, _)| {
            let p = (from.0 + i as i32, y);
            !blocked.contains(&p)
                && p != from
                && p != to
                && inside(width, height, p.0, p.1)
                && !styles.contains_key(&(p.0 as usize, p.1 as usize))
        }) {
            put_text(cells, styles, from.0, y, &label, color);
            if let Some((glyph, color)) = &marker {
                put_text(cells, styles, from.0, y, glyph, *color);
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offscreen_destination_stub_preserves_door_marker_and_command() {
        let mut map = MapState::default();
        map.execute("create").unwrap();
        map.execute("link w 2").unwrap();
        map.execute("door w locked").unwrap();
        let exit = &map.rooms["1"].exits["w"];
        let mut cells = vec![vec![' '; 20]; 6];
        let mut styles = BTreeMap::new();
        let config = MapRenderConfig::default();
        let theme = Theme::from_named("catppuccin-mocha").unwrap();
        draw_schematic_connection(
            &mut cells,
            &mut styles,
            &map,
            &config,
            &theme,
            exit,
            ((2, 2), (25, 20)),
            &BTreeMap::from([("1".into(), (0, 0)), ("2".into(), (23, 18))]),
            ((2, 2), (1, 1)),
            theme.foreground,
        );
        let row: String = cells[3].iter().collect();
        assert!(row.contains("╬w→2"), "{row}");
    }
}
