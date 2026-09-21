use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::{
    DisplayBreakpointsConfig, FullHdLayoutConfig, LargeLayoutConfig, LayoutConfig, PaneRole,
    SidebarPosition, StackedLayoutConfig,
};

pub const COMMAND_HEIGHT: u16 = 3;
pub const MIN_CENTER_WIDTH: u16 = 20;
pub const MIN_OUTPUT_HEIGHT: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutMinimums {
    pub map_width: u16,
    pub map_height: u16,
    pub output_width: u16,
    pub output_height: u16,
    pub input_height: u16,
}

impl Default for LayoutMinimums {
    fn default() -> Self {
        Self {
            map_width: MIN_CENTER_WIDTH,
            map_height: 5,
            output_width: MIN_CENTER_WIDTH,
            output_height: MIN_OUTPUT_HEIGHT,
            input_height: COMMAND_HEIGHT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Mobile,
    Tablet,
    FullHd,
    Ultrawide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Left,
    Right,
    MapOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignedPane {
    pub area: Rect,
    pub role: PaneRole,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutOverrides {
    pub left_width: Option<u16>,
    pub right_width: Option<u16>,
    pub stacked_map_height: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedLayout {
    pub mode: UiMode,
    pub map: Rect,
    pub output: Rect,
    pub input: Rect,
    pub compact_status: Option<Rect>,
    pub top: Option<AssignedPane>,
    pub left: Option<AssignedPane>,
    pub right: Option<AssignedPane>,
    pub left_divider: Option<Rect>,
    pub right_divider: Option<Rect>,
    pub map_output_divider: Option<Rect>,
}

pub fn mode_for(area: Rect, breakpoints: &DisplayBreakpointsConfig) -> UiMode {
    if area.width < breakpoints.mobile_width || area.height < breakpoints.mobile_height {
        UiMode::Mobile
    } else if area.width < breakpoints.tablet_width || area.height < breakpoints.tablet_height {
        UiMode::Tablet
    } else if area.width < breakpoints.ultrawide_width || area.height < breakpoints.ultrawide_height
    {
        UiMode::FullHd
    } else {
        UiMode::Ultrawide
    }
}

pub fn resolve_layout(
    area: Rect,
    config: &LayoutConfig,
    minimums: LayoutMinimums,
    overrides: LayoutOverrides,
) -> ResolvedLayout {
    let mode = mode_for(area, &config.breakpoints);
    let resolved = match mode {
        UiMode::Mobile => resolve_stacked(area, mode, minimums, &config.mobile, overrides),
        UiMode::Tablet => resolve_stacked(area, mode, minimums, &config.tablet, overrides),
        UiMode::FullHd => resolve_full_hd(area, &config.full_hd, minimums, overrides),
        UiMode::Ultrawide => resolve_large(area, mode, &config.ultrawide, minimums, overrides),
    };
    if (resolved.map.width == 0 || resolved.map.height == 0) && area.width > 0 && area.height > 0 {
        resolve_stacked(
            area,
            mode,
            minimums,
            &StackedLayoutConfig::default(),
            overrides,
        )
    } else {
        resolved
    }
}

fn resolve_stacked(
    area: Rect,
    mode: UiMode,
    minimums: LayoutMinimums,
    config: &StackedLayoutConfig,
    overrides: LayoutOverrides,
) -> ResolvedLayout {
    let input_height = minimums.input_height.min(area.height);
    let content_height = area.height.saturating_sub(input_height);
    let status_height = u16::from(config.show_status && content_height >= 15);
    let available = content_height.saturating_sub(status_height);
    let reserved_output = minimums.output_height.min(available.saturating_sub(1));
    let map_height = if available == 0 {
        0
    } else {
        overrides
            .stacked_map_height
            .unwrap_or(config.map_height.max(minimums.map_height))
            .max(minimums.map_height)
            .min(available.saturating_sub(reserved_output))
            .max(1)
    };
    let output_height = available.saturating_sub(map_height);

    let map = Rect::new(area.x, area.y, area.width, map_height);
    let output = Rect::new(
        area.x,
        area.y.saturating_add(map_height),
        area.width,
        output_height,
    );
    let compact_status = (status_height > 0).then_some(Rect::new(
        area.x,
        output.bottom(),
        area.width,
        status_height,
    ));
    let input = Rect::new(
        area.x,
        area.bottom().saturating_sub(input_height),
        area.width,
        input_height,
    );

    ResolvedLayout {
        mode,
        map,
        output,
        input,
        compact_status,
        top: None,
        left: None,
        right: None,
        left_divider: None,
        right_divider: None,
        map_output_divider: (map_height > 0 && output_height > 0).then_some(Rect::new(
            area.x,
            output.y.saturating_sub(1),
            area.width,
            1,
        )),
    }
}

fn resolve_full_hd(
    area: Rect,
    config: &FullHdLayoutConfig,
    minimums: LayoutMinimums,
    overrides: LayoutOverrides,
) -> ResolvedLayout {
    let requested_width = match config.sidebar_position {
        SidebarPosition::Left => overrides.left_width.unwrap_or(config.sidebar_width),
        SidebarPosition::Right => overrides.right_width.unwrap_or(config.sidebar_width),
    };
    let sidebar_width = requested_width
        .max(minimums.map_width)
        .min(area.width.saturating_sub(minimums.output_width));
    let center_width = area.width.saturating_sub(sidebar_width);
    let (sidebar, center) = match config.sidebar_position {
        SidebarPosition::Left => (
            Rect::new(area.x, area.y, sidebar_width, area.height),
            Rect::new(
                area.x.saturating_add(sidebar_width),
                area.y,
                center_width,
                area.height,
            ),
        ),
        SidebarPosition::Right => (
            Rect::new(
                area.right().saturating_sub(sidebar_width),
                area.y,
                sidebar_width,
                area.height,
            ),
            Rect::new(area.x, area.y, center_width, area.height),
        ),
    };
    let center_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(minimums.input_height.min(center.height)),
        ])
        .split(center);
    let pane = AssignedPane {
        area: sidebar,
        role: PaneRole::ClassicSidebar,
    };

    ResolvedLayout {
        mode: UiMode::FullHd,
        map: sidebar,
        output: center_rows[0],
        input: center_rows[1],
        compact_status: None,
        top: None,
        left: (config.sidebar_position == SidebarPosition::Left).then_some(pane),
        right: (config.sidebar_position == SidebarPosition::Right).then_some(pane),
        left_divider: (config.sidebar_position == SidebarPosition::Left).then_some(Rect::new(
            sidebar.right().saturating_sub(1),
            area.y,
            1,
            area.height,
        )),
        right_divider: (config.sidebar_position == SidebarPosition::Right).then_some(Rect::new(
            sidebar.x,
            area.y,
            1,
            area.height,
        )),
        map_output_divider: None,
    }
}

fn resolve_large(
    area: Rect,
    mode: UiMode,
    config: &LargeLayoutConfig,
    minimums: LayoutMinimums,
    overrides: LayoutOverrides,
) -> ResolvedLayout {
    let top_height = if config.top == PaneRole::None {
        0
    } else {
        config.top_height.min(
            area.height
                .saturating_sub(minimums.input_height + minimums.output_height),
        )
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(top_height), Constraint::Min(0)])
        .split(area);
    let top_area = rows[0];
    let body = rows[1];

    let requested_left = if config.left == PaneRole::None {
        0
    } else {
        overrides
            .left_width
            .unwrap_or(config.left_width)
            .max(if config.left == PaneRole::Map {
                minimums.map_width
            } else {
                0
            })
    };
    let requested_right = if config.right == PaneRole::None {
        0
    } else {
        overrides
            .right_width
            .unwrap_or(config.right_width)
            .max(if config.right == PaneRole::Map {
                minimums.map_width
            } else {
                0
            })
    };
    let (left_width, right_width) = clamp_side_widths(
        area.width,
        requested_left,
        requested_right,
        minimums.output_width,
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Min(0),
            Constraint::Length(right_width),
        ])
        .split(body);
    let center_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(minimums.input_height.min(body.height)),
        ])
        .split(columns[1]);

    let top = (config.top != PaneRole::None).then_some(AssignedPane {
        area: top_area,
        role: config.top,
    });
    let left = (config.left != PaneRole::None).then_some(AssignedPane {
        area: columns[0],
        role: config.left,
    });
    let right = (config.right != PaneRole::None).then_some(AssignedPane {
        area: columns[2],
        role: config.right,
    });
    let map = [top, left, right]
        .into_iter()
        .flatten()
        .find(|pane| pane.role == PaneRole::Map)
        .map_or(Rect::default(), |pane| pane.area);

    ResolvedLayout {
        mode,
        map,
        output: center_rows[0],
        input: center_rows[1],
        compact_status: None,
        top,
        left,
        right,
        left_divider: left
            .map(|pane| Rect::new(pane.area.right().saturating_sub(1), body.y, 1, body.height)),
        right_divider: right.map(|pane| Rect::new(pane.area.x, body.y, 1, body.height)),
        map_output_divider: None,
    }
}

fn clamp_side_widths(
    total_width: u16,
    requested_left: u16,
    requested_right: u16,
    minimum_center_width: u16,
) -> (u16, u16) {
    let max_sides = total_width.saturating_sub(minimum_center_width);
    match (requested_left > 0, requested_right > 0) {
        (false, false) => (0, 0),
        (true, false) => (requested_left.min(max_sides), 0),
        (false, true) => (0, requested_right.min(max_sides)),
        (true, true) => {
            let side_minimum = MIN_CENTER_WIDTH.min(max_sides / 2);
            let left = requested_left.clamp(side_minimum, max_sides - side_minimum);
            let right = requested_right.clamp(side_minimum, max_sides - left);
            (left, right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LayoutConfig {
        LayoutConfig::default()
    }

    #[test]
    fn selects_responsive_modes_at_boundaries() {
        let breakpoints = DisplayBreakpointsConfig::default();
        assert_eq!(
            mode_for(Rect::new(0, 0, 79, 50), &breakpoints),
            UiMode::Mobile
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 200, 23), &breakpoints),
            UiMode::Mobile
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 80, 24), &breakpoints),
            UiMode::Tablet
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 120, 29), &breakpoints),
            UiMode::Tablet
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 120, 30), &breakpoints),
            UiMode::FullHd
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 200, 44), &breakpoints),
            UiMode::FullHd
        );
        assert_eq!(
            mode_for(Rect::new(0, 0, 200, 45), &breakpoints),
            UiMode::Ultrawide
        );
    }

    #[test]
    fn mobile_and_tablet_stack_required_regions() {
        for area in [Rect::new(0, 0, 70, 24), Rect::new(0, 0, 100, 30)] {
            let layout = resolve_layout(
                area,
                &config(),
                LayoutMinimums::default(),
                LayoutOverrides {
                    left_width: None,
                    right_width: None,
                    stacked_map_height: None,
                },
            );
            assert!(layout.map.height > 0);
            assert!(layout.output.height > 0);
            assert_eq!(layout.input.height, COMMAND_HEIGHT);
            assert_eq!(layout.map.bottom(), layout.output.y);
            assert_eq!(layout.input.bottom(), area.bottom());
        }
    }

    #[test]
    fn large_map_minimum_does_not_starve_mobile_output() {
        let layout = resolve_layout(
            Rect::new(0, 0, 70, 24),
            &config(),
            LayoutMinimums {
                map_height: 20,
                ..LayoutMinimums::default()
            },
            LayoutOverrides::default(),
        );

        assert!(layout.map.height >= 5);
        assert!(layout.output.height >= MIN_OUTPUT_HEIGHT);
    }

    #[test]
    fn oversized_side_requests_preserve_both_panes_and_center() {
        let mut config = config();
        config.ultrawide.left_width = u16::MAX;
        config.ultrawide.right_width = u16::MAX;
        let layout = resolve_layout(
            Rect::new(0, 0, 220, 50),
            &config,
            LayoutMinimums::default(),
            LayoutOverrides::default(),
        );

        assert!(layout.left.expect("left pane").area.width >= MIN_CENTER_WIDTH);
        assert!(layout.right.expect("right pane").area.width >= MIN_CENTER_WIDTH);
        assert!(layout.output.width >= MIN_CENTER_WIDTH);
    }

    #[test]
    fn full_hd_uses_configured_single_sidebar() {
        let mut config = config();
        config.full_hd.sidebar_position = SidebarPosition::Left;
        config.full_hd.sidebar_width = 40;
        let layout = resolve_layout(
            Rect::new(0, 0, 160, 40),
            &config,
            LayoutMinimums::default(),
            LayoutOverrides {
                left_width: None,
                right_width: None,
                stacked_map_height: None,
            },
        );

        assert_eq!(layout.mode, UiMode::FullHd);
        assert!(layout.top.is_none());
        assert_eq!(
            layout.left.expect("left pane").role,
            PaneRole::ClassicSidebar
        );
        assert!(layout.right.is_none());
        assert_eq!(layout.map.width, 40);
        assert_eq!(layout.output.width, 120);
        assert_eq!(layout.output.bottom(), layout.input.y);
    }

    #[test]
    fn large_slots_can_move_the_map() {
        let mut config = config();
        config.ultrawide.top = PaneRole::Map;
        config.ultrawide.left = PaneRole::Details;
        config.ultrawide.right = PaneRole::Info;
        let layout = resolve_layout(
            Rect::new(0, 0, 220, 50),
            &config,
            LayoutMinimums::default(),
            LayoutOverrides {
                left_width: Some(30),
                right_width: Some(30),
                stacked_map_height: None,
            },
        );

        assert_eq!(layout.map, layout.top.expect("top pane").area);
        assert_eq!(layout.left.expect("left pane").role, PaneRole::Details);
        assert_eq!(layout.right.expect("right pane").role, PaneRole::Info);
    }

    #[test]
    fn invalid_large_layout_falls_back_to_required_stack() {
        let mut config = config();
        config.ultrawide.right = PaneRole::Details;
        let layout = resolve_layout(
            Rect::new(0, 0, 220, 50),
            &config,
            LayoutMinimums::default(),
            LayoutOverrides::default(),
        );

        assert!(layout.map.height > 0);
        assert!(layout.output.height >= MIN_OUTPUT_HEIGHT);
        assert_eq!(layout.input.height, COMMAND_HEIGHT);
        assert!(layout.top.is_none());
        assert!(layout.left.is_none());
        assert!(layout.right.is_none());
    }

    #[test]
    fn tiny_stacked_layout_keeps_map_output_and_command_regions() {
        let layout = resolve_layout(
            Rect::new(0, 0, 40, 6),
            &config(),
            LayoutMinimums::default(),
            LayoutOverrides::default(),
        );

        assert_eq!(layout.map.height, 1);
        assert_eq!(layout.output.height, 2);
        assert_eq!(layout.input.height, 3);
    }

    #[test]
    fn configured_panel_minimums_constrain_full_hd_geometry() {
        let layout = resolve_layout(
            Rect::new(0, 0, 160, 40),
            &config(),
            LayoutMinimums {
                map_width: 50,
                output_width: 80,
                input_height: 5,
                ..LayoutMinimums::default()
            },
            LayoutOverrides::default(),
        );

        assert_eq!(layout.right.expect("right sidebar").area.width, 50);
        assert_eq!(layout.output.width, 110);
        assert_eq!(layout.input.height, 5);
    }
}
