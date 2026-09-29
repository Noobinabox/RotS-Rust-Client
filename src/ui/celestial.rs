//! Read-only sky geometry and theme-relative lighting for clear or cloudy weather.

use ratatui::style::{Color, Modifier, Style};

use crate::animation::daylight::CelestialPosition;

use super::theme::Theme;

pub(super) fn moon_shape(width: u16, height: u16) -> &'static [&'static str] {
    if width >= 7 && height >= 5 {
        &[" .---. ", "/ o   \\", "|   o |", "\\ .   /", " '---' "]
    } else if width >= 5 && height >= 3 {
        &[" .-. ", "(o .)", " `-' "]
    } else {
        &["o"]
    }
}

pub(super) struct CelestialScene {
    pub x: i32,
    pub y: i32,
    pub moon: bool,
    pub compact: bool,
    pub moon_shape: &'static [&'static str],
    pub style: Style,
}

impl CelestialScene {
    /// RGB themes blend into the background; palettes use a stable spatial
    /// reveal and dimming because terminal palette colors have no alpha channel.
    pub fn faded_style(&self, opacity: u8, x: i32, y: i32, theme: &Theme) -> Option<Style> {
        if opacity == 0 {
            return None;
        }
        let mut style = self.style;
        if let (Color::Rgb(..), Some(color @ Color::Rgb(..))) = (theme.background, style.fg) {
            style = style.fg(blend(theme.background, color, f32::from(opacity) / 255.0));
        } else {
            // Relative to the body, not the frame: reversing the fade removes
            // cells in reverse order without random flicker.
            let threshold = ((x - self.x) * 73 + (y - self.y) * 109).rem_euclid(255);
            if i32::from(opacity) <= threshold {
                return None;
            }
        }
        if opacity < 170 {
            style = style.add_modifier(Modifier::DIM);
        }
        Some(style)
    }

    pub fn new(position: CelestialPosition, width: u16, height: u16, theme: &Theme) -> Self {
        let progress = position.progress.clamp(0.0, 1.0);
        let elevation = 4.0 * progress * (1.0 - progress);
        let compact = width < 7 || height < 3;
        let moon_shape = moon_shape(width, height);
        let (shape_width, shape_height) = if position.moon {
            (moon_shape[0].len() as u16, moon_shape.len() as u16)
        } else if compact {
            (1, 1)
        } else {
            (7, 3)
        };
        // North-up: east is right, west is left.
        let x = (f32::from(width.saturating_sub(shape_width)) * (1.0 - progress)).round() as i32;
        let y = (f32::from(height.saturating_sub(shape_height)) * (1.0 - elevation)).round() as i32;
        let color = if position.moon {
            blend(theme.muted, theme.foreground, elevation)
        } else if elevation < 0.5 {
            blend(theme.danger, theme.warning, elevation * 2.0)
        } else {
            blend(theme.warning, theme.foreground, (elevation - 0.5) * 2.0)
        };
        let mut style = Style::default().fg(color);
        if elevation < 0.35 {
            style = style.add_modifier(Modifier::DIM);
        }
        Self {
            x,
            y,
            moon: position.moon,
            compact,
            moon_shape,
            style,
        }
    }
}

// Interpolate only RGB themes. Named/indexed colors retain terminal palette
// semantics instead of silently forcing true color on limited terminals.
fn blend(low: Color, high: Color, amount: f32) -> Color {
    match (low, high) {
        (Color::Rgb(r, g, b), Color::Rgb(rr, gg, bb)) => {
            let channel = |a: u8, z: u8| {
                (f32::from(a) + (f32::from(z) - f32::from(a)) * amount).round() as u8
            };
            Color::Rgb(channel(r, rr), channel(g, gg), channel(b, bb))
        }
        _ if amount < 0.5 => low,
        _ => high,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;

    #[test]
    fn fades_are_transparent_at_zero_and_restore_exact_style() {
        let mut theme = Theme::from_config(&ThemeConfig::default());
        for moon in [false, true] {
            theme.background = Color::Rgb(0, 0, 0);
            theme.foreground = Color::Rgb(200, 200, 200);
            let scene = CelestialScene::new(
                CelestialPosition {
                    moon,
                    progress: 0.5,
                },
                40,
                9,
                &theme,
            );
            assert_eq!(scene.faded_style(0, scene.x, scene.y, &theme), None);
            assert_eq!(
                scene.faded_style(255, scene.x, scene.y, &theme),
                Some(scene.style)
            );
            let half = scene.faded_style(128, scene.x, scene.y, &theme).unwrap();
            assert_eq!(half.fg, Some(Color::Rgb(100, 100, 100)));
            assert!(half.add_modifier.contains(Modifier::DIM));
        }
    }

    #[test]
    fn palette_fade_reveals_cells_monotonically_without_rgb_conversion() {
        let mut theme = Theme::from_config(&ThemeConfig::default());
        theme.background = Color::Reset;
        theme.foreground = Color::White;
        for moon in [false, true] {
            let scene = CelestialScene::new(
                CelestialPosition {
                    moon,
                    progress: 0.5,
                },
                40,
                9,
                &theme,
            );
            for x in scene.x..scene.x + 7 {
                for y in scene.y..scene.y + 3 {
                    let mut visible = false;
                    for opacity in 0..=255 {
                        let style = scene.faded_style(opacity, x, y, &theme);
                        assert!(!visible || style.is_some());
                        if let Some(style) = style {
                            assert_eq!(style.fg, Some(Color::White));
                            visible = true;
                        }
                    }
                    assert!(visible);
                }
            }
        }
    }

    #[test]
    fn both_bodies_rise_from_east_right_and_set_west_left() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for moon in [false, true] {
            let make =
                |progress| CelestialScene::new(CelestialPosition { moon, progress }, 40, 9, &theme);
            let (rise, peak, set) = (make(0.0), make(0.5), make(1.0));
            assert!(rise.x > peak.x && peak.x > set.x);
            assert_eq!(set.x, 0);
            assert!(rise.y > peak.y && set.y > peak.y);
            assert_eq!(rise.y, set.y);
            assert_eq!(rise.style, set.style);
            assert_eq!(peak.style.fg, Some(theme.foreground));
            assert!(rise.style.add_modifier.contains(Modifier::DIM));
            assert!(!peak.style.add_modifier.contains(Modifier::DIM));
        }
    }

    #[test]
    fn rgb_blends_but_palette_colors_remain_palette_colors() {
        assert_eq!(
            blend(Color::Rgb(200, 40, 0), Color::Rgb(240, 160, 80), 0.5),
            Color::Rgb(220, 100, 40)
        );
        assert_eq!(blend(Color::Red, Color::Indexed(208), 0.25), Color::Red);
        assert_eq!(
            blend(Color::Red, Color::Indexed(208), 0.75),
            Color::Indexed(208)
        );
    }

    #[test]
    fn tiny_geometry_stays_within_available_cells() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for width in 1..10 {
            for height in 1..6 {
                for moon in [false, true] {
                    for step in 0..=24 {
                        let scene = CelestialScene::new(
                            CelestialPosition {
                                moon,
                                progress: step as f32 / 24.0,
                            },
                            width,
                            height,
                            &theme,
                        );
                        assert!((0..i32::from(width)).contains(&scene.x));
                        assert!((0..i32::from(height)).contains(&scene.y));
                    }
                }
            }
        }
    }
}
