use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{config::GaugeConfig, ui::theme::Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeValue {
    pub current: Option<i64>,
    pub max: Option<i64>,
}

impl GaugeValue {
    pub fn ratio(self) -> f64 {
        let (Some(current), Some(max)) = (self.current, self.max) else {
            return 0.0;
        };
        if max <= 0 {
            return 0.0;
        }
        (current.max(0) as f64 / max as f64).clamp(0.0, 1.0)
    }

    pub fn percent(self) -> u16 {
        (self.ratio() * 100.0).round() as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeKind {
    Health,
    Mana,
    Movement,
    Tnl,
}

pub fn render_gauge(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    kind: GaugeKind,
    value: GaugeValue,
    _theme: &Theme,
    config: &GaugeConfig,
) {
    let color = kind.color();
    let text = gauge_text_for_width(kind, value, config, area.width as usize);

    Paragraph::new(text)
        .style(Style::new().fg(color))
        .render(area, buf);
}

pub fn compact_line(kind: GaugeKind, value: GaugeValue, _theme: &Theme) -> Vec<Span<'static>> {
    let percent = value.percent();
    vec![Span::styled(
        format!("{} {percent}%", kind.short_label()),
        Style::new().fg(kind.color()),
    )]
}

pub fn compact_gauge(
    kind: GaugeKind,
    value: GaugeValue,
    config: &GaugeConfig,
) -> Vec<Span<'static>> {
    let percent = value.percent();
    let width = config.width.clamp(3, 8);
    vec![
        Span::styled(
            kind.short_label().to_string(),
            Style::new().fg(kind.color()),
        ),
        Span::raw(" "),
        Span::styled(
            bar_with_width(percent, config, width),
            Style::new().fg(kind.color()),
        ),
        Span::raw(" "),
        Span::styled(format!("{percent:>2}%"), Style::new().fg(Color::White)),
    ]
}

pub fn status_line<'a>(parts: Vec<Vec<Span<'a>>>) -> Line<'a> {
    let mut spans = Vec::with_capacity(parts.len().saturating_mul(2).saturating_sub(1));
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.extend(part);
    }
    Line::from(spans).bold()
}

impl GaugeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Health => "Health",
            Self::Mana => "Mana",
            Self::Movement => "Movement",
            Self::Tnl => "TNL",
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            Self::Health => "HP",
            Self::Mana => "MP",
            Self::Movement => "MV",
            Self::Tnl => "TNL",
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::Health => Color::Red,
            Self::Mana => Color::Blue,
            Self::Movement => Color::Green,
            Self::Tnl => Color::Yellow,
        }
    }
}

fn gauge_text_for_width(
    kind: GaugeKind,
    value: GaugeValue,
    config: &GaugeConfig,
    available_width: usize,
) -> String {
    let label = kind.label();
    let suffix = gauge_suffix(kind, value);
    let bar_width = expanded_bar_width(available_width, suffix.len(), config);
    let bar = bar_with_width(value.percent(), config, bar_width);
    format!("{label:<8} {bar} {suffix}")
}

fn gauge_suffix(kind: GaugeKind, value: GaugeValue) -> String {
    if kind == GaugeKind::Tnl {
        return match value.current {
            Some(current) => format_tnl_amount(current),
            None => "--".to_string(),
        };
    }
    match (value.current, value.max) {
        (Some(current), Some(max)) if max > 0 => format!("{current:>3} / {max:>3}"),
        (Some(current), _) => format!("{current:>3}"),
        _ => "--".to_string(),
    }
}

fn expanded_bar_width(available_width: usize, suffix_width: usize, config: &GaugeConfig) -> u16 {
    const LABEL_WIDTH: usize = 8;
    const SPACING_WIDTH: usize = 2;
    let fixed_width = LABEL_WIDTH
        .saturating_add(SPACING_WIDTH)
        .saturating_add(suffix_width);
    available_width
        .checked_sub(fixed_width)
        .filter(|width| *width > 0)
        .map(|width| width.min(u16::MAX as usize) as u16)
        .unwrap_or_else(|| config.width.max(1))
}

fn format_tnl_amount(value: i64) -> String {
    let value = value.max(0);
    if value >= 10_000 {
        format!("{}K", value / 1_000)
    } else if value > 1_000 {
        let whole = value / 1_000;
        let decimal = (value % 1_000) / 100;
        format!("{whole}.{decimal}K")
    } else {
        value.to_string()
    }
}

fn bar_with_width(percent: u16, config: &GaugeConfig, width: u16) -> String {
    let width = usize::from(width.max(1));
    let filled = (usize::from(percent) * width + 50) / 100;
    let (filled_char, empty_char) = if config.unicode {
        ('█', '░')
    } else {
        ('#', '-')
    };
    std::iter::repeat_n(filled_char, filled)
        .chain(std::iter::repeat_n(
            empty_char,
            width.saturating_sub(filled),
        ))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_handles_boundaries() {
        assert_eq!(
            GaugeValue {
                current: Some(50),
                max: Some(100)
            }
            .percent(),
            50
        );
        assert_eq!(
            GaugeValue {
                current: Some(10),
                max: Some(0)
            }
            .percent(),
            0
        );
        assert_eq!(
            GaugeValue {
                current: Some(150),
                max: Some(100)
            }
            .percent(),
            100
        );
        assert_eq!(
            GaugeValue {
                current: Some(-1),
                max: Some(100)
            }
            .percent(),
            0
        );
    }

    #[test]
    fn gauge_kinds_use_fixed_resource_colors() {
        assert_eq!(GaugeKind::Health.color(), Color::Red);
        assert_eq!(GaugeKind::Mana.color(), Color::Blue);
        assert_eq!(GaugeKind::Movement.color(), Color::Green);
    }

    #[test]
    fn compact_gauge_renders_bar_then_fixed_width_percent() {
        let spans = compact_gauge(
            GaugeKind::Health,
            GaugeValue {
                current: Some(50),
                max: Some(100),
            },
            &GaugeConfig {
                unicode: false,
                width: 10,
            },
        );
        let text = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(text, "HP ####---- 50%");
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(spans[2].style.fg, Some(Color::Red));
        assert_eq!(spans[4].content.as_ref(), "50%");
        assert_eq!(spans[4].style.fg, Some(Color::White));
    }

    #[test]
    fn compact_gauge_pads_single_digit_percent() {
        let spans = compact_gauge(
            GaugeKind::Movement,
            GaugeValue {
                current: Some(5),
                max: Some(100),
            },
            &GaugeConfig {
                unicode: false,
                width: 6,
            },
        );
        let text = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(text, "MV ------  5%");
    }

    #[test]
    fn character_gauge_values_have_minimum_three_digit_columns() {
        let value = GaugeValue {
            current: Some(5),
            max: Some(90),
        };
        let config = GaugeConfig {
            unicode: false,
            width: 4,
        };
        let text = gauge_text_for_width(GaugeKind::Health, value, &config, 0);

        assert_eq!(text, "Health   ----   5 /  90");
    }

    #[test]
    fn tnl_gauge_displays_remaining_amount_compactly() {
        let config = GaugeConfig {
            unicode: false,
            width: 4,
        };

        assert_eq!(
            gauge_text_for_width(
                GaugeKind::Tnl,
                GaugeValue {
                    current: Some(10_000),
                    max: Some(20_000),
                },
                &config,
                0,
            ),
            "TNL      ##-- 10K"
        );
        assert_eq!(
            gauge_text_for_width(
                GaugeKind::Tnl,
                GaugeValue {
                    current: Some(9_250),
                    max: Some(20_000),
                },
                &config,
                0,
            ),
            "TNL      ##-- 9.2K"
        );
        assert_eq!(
            gauge_text_for_width(
                GaugeKind::Tnl,
                GaugeValue {
                    current: Some(999),
                    max: Some(20_000),
                },
                &config,
                0,
            ),
            "TNL      ---- 999"
        );
    }

    #[test]
    fn full_gauge_expands_bar_to_available_width() {
        let config = GaugeConfig {
            unicode: false,
            width: 4,
        };
        let text = gauge_text_for_width(
            GaugeKind::Health,
            GaugeValue {
                current: Some(50),
                max: Some(100),
            },
            &config,
            30,
        );

        assert_eq!(text, "Health   ######-----  50 / 100");
        assert_eq!(text.chars().count(), 30);
    }

    #[test]
    fn full_tnl_gauge_expands_bar_to_available_width() {
        let config = GaugeConfig {
            unicode: false,
            width: 4,
        };
        let text = gauge_text_for_width(
            GaugeKind::Tnl,
            GaugeValue {
                current: Some(9_250),
                max: Some(20_000),
            },
            &config,
            30,
        );

        assert_eq!(text, "TNL      #######--------- 9.2K");
        assert_eq!(text.chars().count(), 30);
    }
}
