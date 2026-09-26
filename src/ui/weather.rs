//! Deterministic weather in the unused space around World-pane text.
//!
//! The caller owns timing and supplies the inner content area after rendering
//! text. This module only decorates available cells; it never mutates app state.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use unicode_width::UnicodeWidthStr;

use crate::animation::{daylight::Daylight, weather::WeatherKind, weather_playback::SCENE_PHASES};

use super::theme::Theme;

/// Add a sparse, low-contrast background without overwriting existing content.
/// Each occupied row protects its entire text span, including internal spaces,
/// wide-glyph continuation cells, and one extra cell on either side.
pub fn render_weather(
    area: Rect,
    buf: &mut Buffer,
    kind: WeatherKind,
    phase: usize,
    theme: &Theme,
    daylight: Daylight,
) {
    if matches!(kind, WeatherKind::Indoor | WeatherKind::Unknown) {
        return;
    }
    let clipped = area.intersection(buf.area);
    if clipped.is_empty() {
        return;
    }
    let phase = (phase % SCENE_PHASES) as i32;
    let (muted, accent) = palette(kind, daylight, theme);
    for y in clipped.top()..clipped.bottom() {
        // Snapshot occupancy before adding weather to this row. Treat non-ASCII
        // whitespace as content too, rather than erasing intentional spacing.
        let protected = protected_span(buf, clipped, y);
        for x in clipped.left()..clipped.right() {
            if protected.is_some_and(|(left, right)| x >= left && x < right) {
                continue;
            }
            let local_x = i32::from(x) - i32::from(area.x);
            let local_y = i32::from(y) - i32::from(area.y);
            let glyph = if kind == WeatherKind::Clear && daylight == Daylight::Night {
                // A quiet crescent replaces sunshine; no lunar phase is inferred.
                silhouette(
                    &[" .)", "(  ", " `)"],
                    local_x - i32::from(area.width) * 3 / 4,
                    local_y - (i32::from(area.height) / 2 - 1).max(0),
                )
                .map(|symbol| (symbol, true))
            } else {
                weather_glyph(
                    kind,
                    local_x,
                    local_y,
                    i32::from(area.width),
                    i32::from(area.height),
                    phase,
                )
            };
            if let Some((symbol, highlighted)) = glyph {
                buf[(x, y)]
                    .set_symbol(symbol)
                    .set_style(if highlighted { accent } else { muted });
            }
        }
    }
}

fn palette(kind: WeatherKind, daylight: Daylight, theme: &Theme) -> (Style, Style) {
    // Reuse configurable theme roles without forcing RGB output on ANSI themes.
    let base = match kind {
        WeatherKind::Clear | WeatherKind::Dust => theme.warning,
        WeatherKind::Snow | WeatherKind::Blizzard => theme.foreground,
        WeatherKind::Rain | WeatherKind::Storm => theme.accent,
        _ => theme.muted,
    };
    let (body, highlight) = match daylight {
        Daylight::Dawn | Daylight::Dusk => (theme.warning, theme.warning),
        Daylight::Day => (
            base,
            if kind == WeatherKind::Clear {
                theme.warning
            } else {
                theme.foreground
            },
        ),
        Daylight::Night => (theme.muted, theme.accent),
        Daylight::Unknown => (theme.muted, theme.accent),
    };
    let mut body = Style::default().fg(body);
    let mut highlight = Style::default().fg(highlight);
    if daylight != Daylight::Day {
        body = body.add_modifier(Modifier::DIM);
        highlight = highlight.add_modifier(Modifier::DIM);
    }
    (body, highlight)
}

fn protected_span(buf: &Buffer, area: Rect, y: u16) -> Option<(u16, u16)> {
    let mut span: Option<(u16, u16)> = None;
    // A wide glyph may start immediately outside the clipped area while its
    // blank continuation cell lies inside it.
    for x in area.left().saturating_sub(1).max(buf.area.left())..area.right() {
        let symbol = buf[(x, y)].symbol();
        if symbol == " " || symbol.is_empty() {
            continue;
        }
        let width = u16::try_from(symbol.width()).unwrap_or(u16::MAX).max(1);
        let right = x.saturating_add(width).saturating_add(1);
        span = Some(match span {
            Some((left, previous_right)) => (left, previous_right.max(right)),
            None => (x.saturating_sub(1), right),
        });
    }
    span
}

/// All shapes use single-column ASCII glyphs. Arithmetic uses bounded local
/// coordinates and a bounded phase, so tiny and clipped panes share one path.
fn weather_glyph(
    kind: WeatherKind,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    phase: i32,
) -> Option<(&'static str, bool)> {
    match kind {
        WeatherKind::Clear => {
            let center = (width * 3 / 4).max(0);
            let row = if height == 1 {
                1
            } else {
                y - (height / 2 - 1).max(0)
            };
            let shape: &[&str] = if phase / 12 % 2 == 0 {
                &[" \\ | / ", " - O - ", " / | \\ "]
            } else {
                &["   |   ", " --O-- ", "   |   "]
            };
            silhouette(shape, x - center + 3, row).map(|symbol| (symbol, true))
        }
        WeatherKind::Cloudy => {
            let drift = phase / 12;
            let cloud_x = (x - drift).rem_euclid(23);
            let cloud_y = y.rem_euclid(6);
            silhouette(
                &["   .--.    ", " .(    ).  ", "(___.__)__)"],
                cloud_x,
                cloud_y,
            )
            .map(|symbol| (symbol, false))
        }
        WeatherKind::Rain => rain(x, y, phase).map(|symbol| (symbol, false)),
        WeatherKind::Storm => {
            // A small intermittent bolt, never a full-pane flash.
            if phase % 80 < 3 {
                let bolt_x = width * 2 / 3;
                let bolt_y = (height / 2 - 1).max(0);
                if let Some(symbol) = silhouette(&[" /", "/_", " /"], x - bolt_x, y - bolt_y) {
                    return Some((symbol, true));
                }
            }
            rain(x, y, phase).map(|symbol| (symbol, false))
        }
        WeatherKind::Snow | WeatherKind::Blizzard => {
            let dense = kind == WeatherKind::Blizzard;
            let step = if dense { phase } else { phase / 3 };
            let drift = if dense { step } else { step / 2 };
            let spacing = if dense { 13 } else { 31 };
            let seed = (x - drift) * 7 + (y - step) * 11;
            if seed.rem_euclid(spacing) == 0 {
                Some((
                    if (x + y + step).rem_euclid(3) == 0 {
                        "*"
                    } else {
                        "."
                    },
                    false,
                ))
            } else {
                None
            }
        }
        WeatherKind::Fog => {
            let moving_x = (x - phase / 16).rem_euclid(19);
            (y % 3 == 0 && moving_x < 5).then_some(("~", false))
        }
        WeatherKind::Wind => {
            let moving_x = (x - phase / 2 + y * 3).rem_euclid(19);
            match (y % 2, moving_x) {
                (0, 0..=2) => Some(("-", false)),
                (0, 3) => Some((">", false)),
                _ => None,
            }
        }
        WeatherKind::Ash => (((x + phase / 4) * 5 + (y - phase / 3) * 7).rem_euclid(29) == 0)
            .then_some(("'", false)),
        WeatherKind::Dust => {
            (((x - phase / 2) * 3 + y * 5).rem_euclid(23) == 0).then_some((":", false))
        }
        WeatherKind::Indoor | WeatherKind::Unknown => None,
    }
}

fn rain(x: i32, y: i32, phase: i32) -> Option<&'static str> {
    // Translate a sparse field down and left, rather than sampling randomness.
    (((x + phase / 2) * 3 + (y - phase) * 7).rem_euclid(23) == 0).then_some("/")
}

fn silhouette(shape: &[&str], x: i32, y: i32) -> Option<&'static str> {
    let row = shape.get(usize::try_from(y).ok()?)?;
    match row.as_bytes().get(usize::try_from(x).ok()?)? {
        b'.' => Some("."),
        b'-' => Some("-"),
        b'|' => Some("|"),
        b'/' => Some("/"),
        b'\\' => Some("\\"),
        b'O' => Some("O"),
        b'(' => Some("("),
        b')' => Some(")"),
        b'_' => Some("_"),
        b'`' => Some("`"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;

    fn theme() -> Theme {
        Theme::from_config(&ThemeConfig::default())
    }

    fn scene(kind: WeatherKind, phase: usize) -> Buffer {
        let area = Rect::new(0, 0, 40, 9);
        let mut buf = Buffer::empty(area);
        render_weather(area, &mut buf, kind, phase, &theme(), Daylight::Unknown);
        buf
    }

    #[test]
    fn every_scene_uses_daylight_and_preserves_theme_color_formats() {
        let mut theme = theme();
        theme.warning = ratatui::style::Color::Indexed(208);
        theme.muted = ratatui::style::Color::Rgb(30, 40, 60);
        for kind in [
            WeatherKind::Clear,
            WeatherKind::Cloudy,
            WeatherKind::Rain,
            WeatherKind::Storm,
            WeatherKind::Snow,
            WeatherKind::Blizzard,
            WeatherKind::Fog,
            WeatherKind::Wind,
            WeatherKind::Ash,
            WeatherKind::Dust,
        ] {
            let dawn = palette(kind, Daylight::Dawn, &theme);
            assert_eq!(dawn.0.fg, Some(theme.warning));
            assert_eq!(dawn, palette(kind, Daylight::Dusk, &theme));
            let night = palette(kind, Daylight::Night, &theme);
            assert_eq!(night.0.fg, Some(theme.muted));
            assert!(night.0.add_modifier.contains(Modifier::DIM));
            assert!(
                !palette(kind, Daylight::Day, &theme)
                    .0
                    .add_modifier
                    .contains(Modifier::DIM)
            );
            let area = Rect::new(0, 0, 40, 9);
            let mut day = Buffer::empty(area);
            let mut dark = Buffer::empty(area);
            render_weather(area, &mut day, kind, 0, &theme, Daylight::Day);
            render_weather(area, &mut dark, kind, 0, &theme, Daylight::Night);
            assert_ne!(day, dark, "{kind:?}");
            if kind == WeatherKind::Clear {
                assert!(day.content.iter().any(|cell| cell.symbol() == "O"));
                assert!(!dark.content.iter().any(|cell| cell.symbol() == "O"));
            }
        }
    }

    #[test]
    fn scenes_are_deterministic_and_wrap_without_time_or_randomness() {
        for kind in [
            WeatherKind::Clear,
            WeatherKind::Cloudy,
            WeatherKind::Rain,
            WeatherKind::Storm,
            WeatherKind::Snow,
            WeatherKind::Blizzard,
            WeatherKind::Fog,
            WeatherKind::Wind,
            WeatherKind::Ash,
            WeatherKind::Dust,
        ] {
            assert_eq!(scene(kind, 17), scene(kind, 17));
            assert_eq!(scene(kind, 17), scene(kind, 17 + SCENE_PHASES));
            assert_eq!(
                scene(kind, usize::MAX),
                scene(kind, usize::MAX % SCENE_PHASES)
            );
            assert_ne!(scene(kind, 0), scene(kind, 19), "{kind:?} should move");
        }
    }

    #[test]
    fn six_rots_conditions_produce_distinct_scenes() {
        let kinds = [
            WeatherKind::Clear,
            WeatherKind::Cloudy,
            WeatherKind::Rain,
            WeatherKind::Storm,
            WeatherKind::Snow,
            WeatherKind::Blizzard,
        ];
        for (index, kind) in kinds.iter().enumerate() {
            for other in &kinds[index + 1..] {
                assert_ne!(scene(*kind, 0), scene(*other, 0), "{kind:?} vs {other:?}");
            }
        }
    }

    #[test]
    fn sunshine_snapshot_uses_a_small_ascii_shape() {
        let area = Rect::new(0, 0, 12, 3);
        let mut buf = Buffer::empty(area);
        render_weather(
            area,
            &mut buf,
            WeatherKind::Clear,
            0,
            &theme(),
            Daylight::Unknown,
        );
        let rows: Vec<String> = (0..3)
            .map(|y| (0..12).map(|x| buf[(x, y)].symbol()).collect())
            .collect();
        assert_eq!(rows, ["       \\ | /", "       - O -", "       / | \\"]);
    }

    #[test]
    fn text_gaps_halo_unicode_and_borders_are_preserved() {
        let area = Rect::new(0, 0, 28, 5);
        let mut original = Buffer::empty(area);
        original.set_string(0, 0, "+--------------------------+", Style::default());
        original.set_string(5, 1, "HP  20  MP", Style::default());
        original.set_string(5, 2, "雪  e\u{301}", Style::default());
        original.set_string(5, 3, "\u{a0}", Style::default());
        original.set_string(0, 4, "+--------------------------+", Style::default());
        for kind in [
            WeatherKind::Cloudy,
            WeatherKind::Clear,
            WeatherKind::Storm,
            WeatherKind::Blizzard,
        ] {
            for phase in 0..SCENE_PHASES {
                let mut rendered = original.clone();
                render_weather(
                    area,
                    &mut rendered,
                    kind,
                    phase,
                    &theme(),
                    Daylight::Unknown,
                );
                for y in area.top()..area.bottom() {
                    let (left, right) = protected_span(&original, area, y).unwrap();
                    for x in left..right.min(area.right()) {
                        assert_eq!(
                            rendered[(x, y)],
                            original[(x, y)],
                            "{kind:?} {phase} ({x},{y})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn clipping_empty_and_single_cell_areas_do_not_touch_neighbors() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(6, 8, 1, 1),
            Rect::new(0, 0, 8, 9),
            Rect::new(30, 30, 9, 9),
            Rect::new(u16::MAX, u16::MAX, 1, 1),
        ] {
            let bounds = Rect::new(5, 7, 12, 4);
            let mut buf = Buffer::empty(bounds);
            let before = buf.clone();
            render_weather(
                area,
                &mut buf,
                WeatherKind::Blizzard,
                usize::MAX,
                &theme(),
                Daylight::Unknown,
            );
            let clipped = area.intersection(bounds);
            for y in bounds.top()..bounds.bottom() {
                for x in bounds.left()..bounds.right() {
                    if !clipped.contains((x, y).into()) {
                        assert_eq!(buf[(x, y)], before[(x, y)]);
                    }
                }
            }
        }
    }

    #[test]
    fn clipping_does_not_overwrite_a_wide_glyph_continuation() {
        let bounds = Rect::new(0, 0, 12, 2);
        let area = Rect::new(5, 0, 7, 2);
        let mut original = Buffer::empty(bounds);
        original.set_string(4, 0, "雪", Style::default());
        for phase in 0..SCENE_PHASES {
            let mut rendered = original.clone();
            render_weather(
                area,
                &mut rendered,
                WeatherKind::Blizzard,
                phase,
                &theme(),
                Daylight::Unknown,
            );
            for x in 4..7 {
                assert_eq!(rendered[(x, 0)], original[(x, 0)]);
            }
        }
    }

    #[test]
    fn particles_are_sparse_and_storm_bolt_remains_localized() {
        let count = |buf: &Buffer| {
            buf.content
                .iter()
                .filter(|cell| cell.symbol() != " ")
                .count()
        };
        let snow = count(&scene(WeatherKind::Snow, 0));
        let blizzard = count(&scene(WeatherKind::Blizzard, 0));
        assert!(blizzard > snow);
        assert!(blizzard < 40 * 9 / 8);
        for phase in 0..SCENE_PHASES {
            let rain = scene(WeatherKind::Rain, phase);
            let storm = scene(WeatherKind::Storm, phase);
            let changed = rain
                .content
                .iter()
                .zip(&storm.content)
                .filter(|(left, right)| left != right)
                .count();
            assert!(changed <= 4, "bolt must never flash a pane");
        }
    }

    #[test]
    fn indoor_and_unknown_are_noops_and_weather_uses_only_dim_theme_colors() {
        for kind in [WeatherKind::Indoor, WeatherKind::Unknown] {
            assert_eq!(scene(kind, 0), Buffer::empty(Rect::new(0, 0, 40, 9)));
        }
        let theme = theme();
        for kind in [
            WeatherKind::Clear,
            WeatherKind::Cloudy,
            WeatherKind::Rain,
            WeatherKind::Storm,
            WeatherKind::Snow,
            WeatherKind::Blizzard,
        ] {
            let buf = scene(kind, 0);
            for cell in &buf.content {
                if cell.symbol() != " " {
                    assert!(cell.modifier.contains(Modifier::DIM));
                    assert!(cell.fg == theme.muted || cell.fg == theme.accent);
                }
            }
        }
    }
}
