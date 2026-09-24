use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, Borders, Padding},
};

use crate::{
    config::{PanelBorderStyle, PanelOptions},
    ui::theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelId {
    Info,
    Social,
    Map,
    NearbyMap,
    Opponent,
    Group,
    Character,
    Output,
}

#[derive(Debug)]
struct CachedPanel {
    buffer: Buffer,
    options: PanelOptions,
    theme: Theme,
    rendered_at: Instant,
    visited: bool,
}

/// UI-only cache; never owns or delays application state updates.
#[derive(Debug, Default)]
pub struct PanelCache(RefCell<BTreeMap<PanelInstance, CachedPanel>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PanelInstance {
    id: PanelId,
    geometry: (u16, u16, u16, u16),
}

impl PanelCache {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    pub fn begin_frame(&self) {
        for entry in self.0.borrow_mut().values_mut() {
            entry.visited = false;
        }
    }

    pub fn end_frame(&self) {
        self.0.borrow_mut().retain(|_, entry| entry.visited);
    }
}

pub struct PanelRenderer<'a> {
    pub theme: &'a Theme,
    pub cache: Option<&'a PanelCache>,
}

impl PanelRenderer<'_> {
    pub fn render(
        &self,
        id: PanelId,
        options: &PanelOptions,
        area: Rect,
        buffer: &mut Buffer,
        draw: impl FnOnce(&mut Buffer, &Theme),
    ) {
        self.render_at((id, options, area), buffer, Instant::now(), draw);
    }

    fn render_at(
        &self,
        (id, options, area): (PanelId, &PanelOptions, Rect),
        buffer: &mut Buffer,
        now: Instant,
        draw: impl FnOnce(&mut Buffer, &Theme),
    ) {
        let theme = self.theme.for_panel(options);
        let Some(cache) = self.cache.filter(|_| options.refresh_ms > 0) else {
            draw(buffer, &theme);
            return;
        };
        let mut entries = cache.0.borrow_mut();
        let instance = PanelInstance {
            id,
            geometry: (area.x, area.y, area.width, area.height),
        };
        let reusable = entries.get(&instance).is_some_and(|entry| {
            entry.buffer.area == area
                && entry.options == *options
                && entry.theme == theme
                && now.saturating_duration_since(entry.rendered_at)
                    < Duration::from_millis(options.refresh_ms)
        });
        if !reusable {
            let mut content = Buffer::empty(area);
            draw(&mut content, &theme);
            entries.insert(
                instance,
                CachedPanel {
                    buffer: content,
                    options: options.clone(),
                    theme,
                    rendered_at: now,
                    visited: true,
                },
            );
        }
        if let Some(entry) = entries.get_mut(&instance) {
            entry.visited = true;
            for y in area.y..area.bottom() {
                for x in area.x..area.right() {
                    buffer[(x, y)] = entry.buffer[(x, y)].clone();
                }
            }
        }
    }
}

pub fn panel<'a>(title: &'a str, theme: &Theme) -> Block<'a> {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::new().fg(theme.foreground).bg(theme.background))
        .border_style(Style::new().fg(theme.border))
        .title_style(Style::new().fg(theme.title));
    match theme.border_style {
        PanelBorderStyle::Plain => block.border_type(BorderType::Plain),
        PanelBorderStyle::Rounded => block.border_type(BorderType::Rounded),
        PanelBorderStyle::Double => block.border_type(BorderType::Double),
        PanelBorderStyle::Thick => block.border_type(BorderType::Thick),
        // Preserve the same inner geometry for scrolling, hit testing and cursors.
        // The title itself reserves the top row, so only pad the other sides.
        PanelBorderStyle::None => block
            .borders(Borders::NONE)
            .padding(Padding::new(1, 1, 0, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PanelConfig, ThemeConfig};
    use ratatui::{style::Color, widgets::Widget};
    use std::cell::Cell;

    #[test]
    fn border_styles_keep_geometry_and_apply_local_colors() {
        let base = Theme::from_config(&ThemeConfig::default());
        let mut options = PanelConfig::default().output;
        options.theme.insert("border".into(), "red".into());
        options.theme.insert("background".into(), "blue".into());
        let area = Rect::new(0, 0, 12, 5);
        for (border, corner) in [
            (PanelBorderStyle::Plain, "┌"),
            (PanelBorderStyle::Rounded, "╭"),
            (PanelBorderStyle::Double, "╔"),
            (PanelBorderStyle::Thick, "┏"),
            (PanelBorderStyle::None, " "),
        ] {
            options.border_style = border;
            let theme = base.for_panel(&options);
            let block = panel("Title", &theme);
            assert_eq!(block.inner(area), Rect::new(1, 1, 10, 3));
            let mut buf = Buffer::empty(area);
            block.render(area, &mut buf);
            // Borderless titles start in the top-left cell; check the bottom corner.
            if border != PanelBorderStyle::None {
                assert_eq!(buf[(0, 0)].symbol(), corner);
            } else {
                assert_eq!(buf[(0, 4)].symbol(), " ");
            }
            assert_eq!(buf[(1, 1)].bg, Color::Blue);
            assert_eq!(theme.border, Color::Red);
            assert_eq!(theme.accent, base.accent);
        }
    }

    #[test]
    fn duplicate_panel_roles_keep_independent_cached_content() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let cache = PanelCache::default();
        let renderer = PanelRenderer {
            theme: &theme,
            cache: Some(&cache),
        };
        let mut options = PanelConfig::default().info;
        options.refresh_ms = 100;
        let now = Instant::now();
        let count = Cell::new(0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        for millis in [0, 50] {
            cache.begin_frame();
            for (x, symbol) in [(0, "L"), (15, "R")] {
                renderer.render_at(
                    (PanelId::Info, &options, Rect::new(x, 0, 10, 5)),
                    &mut buf,
                    now + Duration::from_millis(millis),
                    |buf, _| {
                        count.set(count.get() + 1);
                        buf[(x, 0)].set_symbol(symbol);
                    },
                );
            }
            cache.end_frame();
        }
        assert_eq!(count.get(), 2);
        assert_eq!(cache.0.borrow().len(), 2);
        assert_eq!(buf[(0, 0)].symbol(), "L");
        assert_eq!(buf[(15, 0)].symbol(), "R");
    }

    #[test]
    fn refresh_deadline_and_invalidations_are_deterministic() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let cache = PanelCache::default();
        let renderer = PanelRenderer {
            theme: &theme,
            cache: Some(&cache),
        };
        let mut options = PanelConfig::default().map;
        options.refresh_ms = 100;
        let area = Rect::new(2, 3, 5, 4);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 20));
        let count = Cell::new(0);
        let now = Instant::now();
        let draw = |buf: &mut Buffer, _: &Theme| {
            count.set(count.get() + 1);
            buf[(2, 3)].set_symbol("X");
        };
        renderer.render_at((PanelId::Map, &options, area), &mut buf, now, draw);
        buf[(2, 3)].set_symbol(" ");
        renderer.render_at(
            (PanelId::Map, &options, area),
            &mut buf,
            now + Duration::from_millis(99),
            draw,
        );
        assert_eq!(count.get(), 1);
        assert_eq!(buf[(2, 3)].symbol(), "X");
        renderer.render_at(
            (PanelId::Map, &options, area),
            &mut buf,
            now + Duration::from_millis(100),
            draw,
        );
        assert_eq!(count.get(), 2);
        options.theme.insert("foreground".into(), "red".into());
        renderer.render_at(
            (PanelId::Map, &options, area),
            &mut buf,
            now + Duration::from_millis(101),
            draw,
        );
        assert_eq!(count.get(), 3);
        renderer.render_at(
            (PanelId::Map, &options, Rect::new(2, 3, 6, 4)),
            &mut buf,
            now + Duration::from_millis(102),
            draw,
        );
        assert_eq!(count.get(), 4);
        cache.begin_frame();
        cache.end_frame(); // Hidden panels must not return stale when made visible again.
        renderer.render_at(
            (PanelId::Map, &options, area),
            &mut buf,
            now + Duration::from_millis(103),
            draw,
        );
        assert_eq!(count.get(), 5);
        cache.clear();
        renderer.render_at(
            (PanelId::Map, &options, area),
            &mut buf,
            now + Duration::from_millis(104),
            draw,
        );
        assert_eq!(count.get(), 6);
        options.refresh_ms = 0;
        for _ in 0..2 {
            renderer.render_at((PanelId::Map, &options, area), &mut buf, now, draw);
        }
        assert_eq!(count.get(), 8);
    }
}
