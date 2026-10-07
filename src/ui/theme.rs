use ratatui::{layout::Alignment, style::Color};

use crate::config::{PanelAlignment, PanelBorderStyle, PanelOptions, ThemeAppearance, ThemeConfig};

pub use crate::color::parse_color;

// Snow stays neutral even when general theme roles use warm or saturated colors.
pub const SNOW_WHITE: Color = Color::White;
pub const SNOW_SOFT_WHITE: Color = Color::Gray;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub appearance: ThemeAppearance,
    pub background: Color,
    pub border_style: PanelBorderStyle,
    pub alignment: Alignment,
    pub foreground: Color,
    pub border: Color,
    pub title: Color,
    pub accent: Color,
    pub rain: Color,
    pub lightning: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub muted: Color,
    pub player: Color,
    pub enemy: Color,
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = Self {
            appearance: config.appearance,
            background: parse_color(&config.background).unwrap_or(Color::Reset),
            border_style: PanelBorderStyle::Plain,
            alignment: Alignment::Left,
            foreground: parse_color(&config.foreground).unwrap_or(Color::Gray),
            border: parse_color(&config.border).unwrap_or(Color::DarkGray),
            title: parse_color(&config.title).unwrap_or(Color::Yellow),
            accent: parse_color(&config.accent).unwrap_or(Color::Cyan),
            rain: parse_color(&config.rain).unwrap_or(Color::Cyan),
            lightning: parse_color(&config.lightning).unwrap_or(Color::LightYellow),
            success: parse_color(&config.success).unwrap_or(Color::Green),
            warning: parse_color(&config.warning).unwrap_or(Color::Yellow),
            danger: parse_color(&config.danger).unwrap_or(Color::Red),
            muted: parse_color(&config.muted).unwrap_or(Color::DarkGray),
            player: parse_color(&config.player).unwrap_or(Color::Cyan),
            enemy: parse_color(&config.enemy).unwrap_or(Color::Red),
        };
        if theme.appearance == ThemeAppearance::Light {
            theme.foreground = theme.ensure_contrast(theme.foreground, theme.background, 4.5);
            theme.border = theme.ensure_contrast(theme.border, theme.background, 3.0);
            theme.title = theme.ensure_contrast(theme.title, theme.background, 4.5);
            theme.accent = theme.ensure_contrast(theme.accent, theme.background, 4.5);
            theme.rain = theme.ensure_contrast(theme.rain, theme.background, 3.0);
            theme.lightning = theme.ensure_contrast(theme.lightning, theme.background, 4.5);
            theme.success = theme.ensure_contrast(theme.success, theme.background, 4.5);
            theme.warning = theme.ensure_contrast(theme.warning, theme.background, 4.5);
            theme.danger = theme.ensure_contrast(theme.danger, theme.background, 4.5);
            theme.muted = theme.ensure_contrast(theme.muted, theme.background, 4.5);
            theme.player = theme.ensure_contrast(theme.player, theme.background, 4.5);
            theme.enemy = theme.ensure_contrast(theme.enemy, theme.background, 4.5);
        }
        theme
    }

    pub fn from_named(name: &str) -> Option<Self> {
        let config = named_theme_config(name)?;
        Some(Self::from_config(&config))
    }

    pub fn for_panel(&self, options: &PanelOptions) -> Self {
        let mut theme = *self;
        theme.border_style = options.border_style;
        theme.alignment = match options.alignment {
            PanelAlignment::Left => Alignment::Left,
            PanelAlignment::Center => Alignment::Center,
            PanelAlignment::Right => Alignment::Right,
        };
        for (key, value) in &options.theme {
            let target = match key.as_str() {
                "background" => &mut theme.background,
                "foreground" => &mut theme.foreground,
                "border" => &mut theme.border,
                "title" => &mut theme.title,
                "accent" => &mut theme.accent,
                "rain" => &mut theme.rain,
                "lightning" => &mut theme.lightning,
                "success" => &mut theme.success,
                "warning" => &mut theme.warning,
                "danger" => &mut theme.danger,
                "muted" => &mut theme.muted,
                "player" => &mut theme.player,
                "enemy" => &mut theme.enemy,
                _ => continue,
            };
            if let Some(color) = parse_color(value) {
                *target = color;
            }
        }
        theme
    }

    pub fn contrasting_foreground(&self, background: Color) -> Color {
        if contrast_ratio(Color::Black, background) >= contrast_ratio(Color::White, background) {
            Color::Black
        } else {
            Color::White
        }
    }

    pub fn readable_text_color(&self, color: Color, background: Color) -> Color {
        self.ensure_contrast(color, background, 4.5)
    }

    pub fn readable_map_color(&self, color: Color) -> Color {
        self.ensure_contrast(color, self.background, 3.0)
    }

    fn ensure_contrast(&self, color: Color, background: Color, minimum: f64) -> Color {
        if contrast_ratio(color, background) >= minimum {
            return color;
        }
        let Some((red, green, blue)) = color_rgb(color) else {
            return self.foreground;
        };
        let black_contrast = contrast_ratio(Color::Black, background);
        let white_contrast = contrast_ratio(Color::White, background);
        let target = if black_contrast >= white_contrast {
            0
        } else {
            255
        };
        for step in 1..=20 {
            let mix = |channel: u8| {
                let value = i32::from(channel) + (target - i32::from(channel)) * step / 20;
                value.clamp(0, 255) as u8
            };
            let candidate = Color::Rgb(mix(red), mix(green), mix(blue));
            if contrast_ratio(candidate, background) >= minimum {
                return candidate;
            }
        }
        if target == 0 {
            Color::Black
        } else {
            Color::White
        }
    }
}

fn contrast_ratio(first: Color, second: Color) -> f64 {
    let luminance = |color| {
        let Some((red, green, blue)) = color_rgb(color) else {
            return 0.0;
        };
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    };
    let first = luminance(first);
    let second = luminance(second);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

fn color_rgb(color: Color) -> Option<(u8, u8, u8)> {
    Some(match color {
        Color::Black => (0, 0, 0),
        Color::Red => (128, 0, 0),
        Color::Green => (0, 128, 0),
        Color::Yellow => (128, 128, 0),
        Color::Blue => (0, 0, 128),
        Color::Magenta => (128, 0, 128),
        Color::Cyan => (0, 128, 128),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (0, 0, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Indexed(index) if index < 16 => color_rgb(ansi_index_color(index))?,
        Color::Indexed(index) if index < 232 => {
            let value = index - 16;
            let component = |part: u8| if part == 0 { 0 } else { 55 + part * 40 };
            (
                component(value / 36),
                component((value % 36) / 6),
                component(value % 6),
            )
        }
        Color::Indexed(index) => {
            let value = 8 + (index - 232) * 10;
            (value, value, value)
        }
        Color::Reset => return None,
    })
}

fn ansi_index_color(index: u8) -> Color {
    [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ][usize::from(index)]
}

pub fn builtin_theme_names() -> &'static [&'static str] {
    crate::config::BUILTIN_THEME_NAMES
}

pub use crate::config::normalize_theme_name;

pub fn race_theme_name(race: &str) -> Option<&'static str> {
    match normalize_theme_name(race).as_str() {
        "wood-elf" | "woodelf" => Some("wood-elf"),
        "hobbit" => Some("hobbit"),
        "human" => Some("human"),
        "dwarf" => Some("dwarf"),
        "beorning" => Some("beorning"),
        "uruk-hai" | "urukhai" => Some("uruk-hai"),
        "common-orc" | "commonorc" | "orc" => Some("common-orc"),
        "olog-hai" | "ologhai" => Some("olog-hai"),
        "uruk-lhuth" | "uruklhuth" => Some("uruk-lhuth"),
        "haradrim" | "haradrims" => Some("haradrim"),
        _ => None,
    }
}

fn named_theme_config(name: &str) -> Option<ThemeConfig> {
    let mut config = ThemeConfig::default();
    let set = |config: &mut ThemeConfig, values: [&str; 13]| {
        config.background = values[0].into();
        config.foreground = values[1].into();
        config.border = values[2].into();
        config.title = values[3].into();
        config.accent = values[4].into();
        config.rain = values[5].into();
        config.lightning = values[6].into();
        config.success = values[7].into();
        config.warning = values[8].into();
        config.danger = values[9].into();
        config.muted = values[10].into();
        config.player = values[11].into();
        config.enemy = values[12].into();
    };
    match normalize_theme_name(name).as_str() {
        "tokyo-night" => set(
            &mut config,
            [
                "#1a1b26", "#c0caf5", "#3b4261", "#7aa2f7", "#bb9af7", "#61afef", "#ffff00",
                "#9ece6a", "#e0af68", "#f7768e", "#565f89", "#7dcfff", "#f7768e",
            ],
        ),
        "nord" => set(
            &mut config,
            [
                "#2e3440", "#d8dee9", "#4c566a", "#88c0d0", "#81a1c1", "#81a1c1", "#ebcb8b",
                "#a3be8c", "#ebcb8b", "#bf616a", "#616e88", "#8fbcbb", "#bf616a",
            ],
        ),
        "gruvbox" => set(
            &mut config,
            [
                "#282828", "#ebdbb2", "#504945", "#fabd2f", "#d79921", "#83a598", "#fabd2f",
                "#b8bb26", "#fabd2f", "#fb4934", "#928374", "#8ec07c", "#fb4934",
            ],
        ),
        "dracula" => set(
            &mut config,
            [
                "#282a36", "#f8f8f2", "#6272a4", "#bd93f9", "#ff79c6", "#8be9fd", "#f1fa8c",
                "#50fa7b", "#ffb86c", "#ff5555", "#6272a4", "#8be9fd", "#ff5555",
            ],
        ),
        // Official Everforest dark, medium background (the upstream default).
        // https://github.com/sainnhe/everforest/blob/master/autoload/everforest.vim
        "everforest-dark" => set(
            &mut config,
            [
                "#2d353b", "#d3c6aa", "#859289", "#a7c080", "#d699b6", "#7fbbb3", "#dbbc7f",
                "#a7c080", "#dbbc7f", "#e67e80", "#9da9a0", "#83c092", "#e67e80",
            ],
        ),
        // Official Kanagawa Wave: Sumi Ink background and Fuji White text.
        // https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua
        "kanagawa-wave" => set(
            &mut config,
            [
                "#1f1f28", "#dcd7ba", "#727169", "#7e9cd8", "#957fb8", "#7fb4ca", "#e6c384",
                "#98bb6c", "#e6c384", "#ff5d62", "#c8c093", "#7aa89f", "#e46876",
            ],
        ),
        // Official Rosé Pine Moon: Base, Text, Subtle, Foam, Iris, Pine,
        // Gold, Leaf, Gold, Love, Subtle, Foam, Love.
        // https://github.com/rose-pine/neovim/blob/main/lua/rose-pine/palette.lua
        "rose-pine-moon" => set(
            &mut config,
            [
                "#232136", "#e0def4", "#908caa", "#9ccfd8", "#c4a7e7", "#3e8fb0", "#f6c177",
                "#95b1ac", "#f6c177", "#eb6f92", "#908caa", "#9ccfd8", "#eb6f92",
            ],
        ),
        // Official everforest-light palette: https://github.com/sainnhe/everforest/blob/master/autoload/everforest.vim
        "everforest-light" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#fdf6e3", "#5c6a72", "#829181", "#3a94c5", "#df69ba", "#3a94c5", "#dfa000",
                    "#8da101", "#dfa000", "#f85552", "#829181", "#35a77c", "#f85552",
                ],
            );
        }
        // Official kanagawa-dragon palette: https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua
        "kanagawa-dragon" => set(
            &mut config,
            [
                "#181616", "#c5c9c5", "#7a8382", "#8ba4b0", "#8992a7", "#8ba4b0", "#c4b28a",
                "#87a987", "#c4b28a", "#c4746e", "#a6a69c", "#8ea4a2", "#c4746e",
            ],
        ),
        // Official kanagawa-lotus palette: https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua
        "kanagawa-lotus" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#f2ecbc", "#545464", "#716e61", "#4d699b", "#624c83", "#6693bf", "#de9800",
                    "#6f894e", "#de9800", "#c84053", "#716e61", "#597b75", "#c84053",
                ],
            );
        }
        // Official rose-pine palette: https://github.com/rose-pine/neovim/blob/main/lua/rose-pine/palette.lua
        "rose-pine" => set(
            &mut config,
            [
                "#191724", "#e0def4", "#908caa", "#9ccfd8", "#c4a7e7", "#31748f", "#f6c177",
                "#95b1ac", "#f6c177", "#eb6f92", "#908caa", "#9ccfd8", "#eb6f92",
            ],
        ),
        // Official tokyo-night-storm palette: https://github.com/folke/tokyonight.nvim/blob/main/extras/alacritty/tokyonight_storm.toml
        "tokyo-night-storm" => set(
            &mut config,
            [
                "#24283b", "#c0caf5", "#a9b1d6", "#7aa2f7", "#bb9af7", "#7aa2f7", "#e0af68",
                "#9ece6a", "#e0af68", "#f7768e", "#a9b1d6", "#7dcfff", "#f7768e",
            ],
        ),
        // Official tokyo-night-moon palette: https://github.com/folke/tokyonight.nvim/blob/main/extras/alacritty/tokyonight_moon.toml
        "tokyo-night-moon" => set(
            &mut config,
            [
                "#222436", "#c8d3f5", "#828bb8", "#82aaff", "#c099ff", "#82aaff", "#ffc777",
                "#c3e88d", "#ffc777", "#ff757f", "#828bb8", "#86e1fc", "#ff757f",
            ],
        ),
        // Official tokyo-night-day palette: https://github.com/folke/tokyonight.nvim/blob/main/extras/alacritty/tokyonight_day.toml
        "tokyo-night-day" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#e1e2e7", "#3760bf", "#6172b0", "#2e7de9", "#9854f1", "#2e7de9", "#8c6c3e",
                    "#587539", "#8c6c3e", "#f52a65", "#6172b0", "#007197", "#f52a65",
                ],
            );
        }
        // Official https://catppuccin.com/palette/ colors, in the role order above:
        // Base, Text, Overlay1, Blue, Mauve, Blue, Yellow, Green, Yellow, Red,
        // Subtext0, Teal, Red. Latte is intentionally a light theme.
        "catppuccin-latte" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#eff1f5", "#4c4f69", "#8c8fa1", "#1e66f5", "#8839ef", "#1e66f5", "#df8e1d",
                    "#40a02b", "#df8e1d", "#d20f39", "#6c6f85", "#179299", "#d20f39",
                ],
            );
        }
        "catppuccin-frappe" => set(
            &mut config,
            [
                "#303446", "#c6d0f5", "#838ba7", "#8caaee", "#ca9ee6", "#8caaee", "#e5c890",
                "#a6d189", "#e5c890", "#e78284", "#a5adce", "#81c8be", "#e78284",
            ],
        ),
        "catppuccin-macchiato" => set(
            &mut config,
            [
                "#24273a", "#cad3f5", "#8087a2", "#8aadf4", "#c6a0f6", "#8aadf4", "#eed49f",
                "#a6da95", "#eed49f", "#ed8796", "#a5adcb", "#8bd5ca", "#ed8796",
            ],
        ),
        "catppuccin-mocha" => set(
            &mut config,
            [
                "#1e1e2e", "#cdd6f4", "#7f849c", "#89b4fa", "#cba6f7", "#89b4fa", "#f9e2af",
                "#a6e3a1", "#f9e2af", "#f38ba8", "#a6adc8", "#94e2d5", "#f38ba8",
            ],
        ),
        // Official https://github.com/altercation/solarized palette. Darker
        // monotones retain readable UI text on Solarized's Base3 background.
        "solarized-light" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#fdf6e3", "#586e75", "#657b83", "#268bd2", "#6c71c4", "#268bd2", "#b58900",
                    "#859900", "#b58900", "#dc322f", "#657b83", "#2aa198", "#dc322f",
                ],
            );
        }
        // Official https://github.com/morhetz/gruvbox light palette using the
        // medium background and faded accents selected by Gruvbox light mode.
        "gruvbox-light" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#fbf1c7", "#3c3836", "#928374", "#076678", "#8f3f71", "#076678", "#b57614",
                    "#79740e", "#b57614", "#9d0006", "#665c54", "#427b58", "#9d0006",
                ],
            );
        }
        // Official https://rosepinetheme.com/palette/ Dawn role palette.
        "rose-pine-dawn" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#faf4ed", "#464261", "#797593", "#286983", "#907aa9", "#56949f", "#ea9d34",
                    "#286983", "#ea9d34", "#b4637a", "#797593", "#56949f", "#b4637a",
                ],
            );
        }
        // Official https://primer.style/product/primitives/colors/ light palette.
        "github-light" => {
            config.appearance = ThemeAppearance::Light;
            set(
                &mut config,
                [
                    "#ffffff", "#1f2328", "#d0d7de", "#0969da", "#8250df", "#0969da", "#9a6700",
                    "#1f883d", "#9a6700", "#cf222e", "#656d76", "#1a7f37", "#cf222e",
                ],
            );
        }
        // The Hobbit's Mirkwood and Thranduil's halls: blue-black forest,
        // autumn leaf gold, silver-green elegance, pale stone, and cold mist.
        "wood-elf" => set(
            &mut config,
            [
                "#08191a", "#eef0e5", "#78908a", "#e5c56b", "#a7c7b7", "#678fa5", "#f4dfa0",
                "#62b978", "#e2b85c", "#e57b64", "#a5b2ad", "#d9d6a2", "#c878a5",
            ],
        ),
        // Fellowship's Hobbiton: saturated hill and round-door greens, warm
        // plaster, party lanterns, polished wood, and golden hearthlight.
        "hobbit" => set(
            &mut config,
            [
                "#142414", "#f7e7bd", "#789452", "#e8e04c", "#3aa63a", "#78a9ad", "#ffe09a",
                "#82c85f", "#e7b957", "#dd7b60", "#a8ad88", "#d9ef8b", "#c77fa3",
            ],
        ),
        // Gondor in The Lord of the Rings: night-blue shadow, Minas Tirith
        // stone, White Tree silver, steel, royal blue, and restrained gold.
        "human" => set(
            &mut config,
            [
                "#0c1621", "#f1f0e8", "#71869c", "#f2f0dc", "#7ca8d4", "#6f9fbd", "#f0d990",
                "#83b38f", "#d9b45f", "#d67868", "#9aa7b3", "#dce8f0", "#c56591",
            ],
        ),
        // The Hobbit's Erebor: near-black carved stone, immense molten gold,
        // bronze machinery, emerald treasure, and warm ivory firelight.
        "dwarf" => set(
            &mut config,
            [
                "#0c1918", "#f3e5c2", "#9b7440", "#f0be4f", "#20a985", "#5f9998", "#f5ce68",
                "#55b47e", "#e0a83f", "#d87455", "#a69a82", "#f4d16f", "#c26086",
            ],
        ),
        // The Hobbit's Beorn: dark timber hall, black-brown bear fur, honey,
        // firelight, meadow green, rough leather, and warm cream.
        "beorning" => set(
            &mut config,
            [
                "#24150b", "#f3dfba", "#9b7044", "#e8dc49", "#d09b3e", "#72918d", "#f4d783",
                "#7fa44b", "#ebb34a", "#c96f4e", "#aa9077", "#d9b56b", "#c56791",
            ],
        ),
        // The game's Dol Guldur-based Uruk-Hai, grounded in The Hobbit's wet
        // black masonry, dead woodland, spectral green, iron, smoke, and rust.
        "uruk-hai" => set(
            &mut config,
            [
                "#061514", "#dae3d4", "#536f5b", "#a8c77a", "#329154", "#578b8c", "#cdd784",
                "#70ad65", "#ca9e4c", "#d2664f", "#87998d", "#96c77c", "#c65c8b",
            ],
        ),
        // The Lord of the Rings' Mordor orcs: ash, weathered bone, diseased
        // olive, scavenged iron, torch smoke, and jagged rust-red armor.
        "common-orc" => set(
            &mut config,
            [
                "#18150d", "#dfd3b3", "#766943", "#cdbb88", "#e85c3f", "#6d8580", "#d7c67d",
                "#87953e", "#c58a35", "#d84f6f", "#968d77", "#aeb052", "#e29355",
            ],
        ),
        // The game's troll/Uruk-Hai hybrids draw on Mordor's armored film
        // trolls: Morannon basalt, black iron, bronze, blood, and sick green.
        "olog-hai" => set(
            &mut config,
            [
                "#0d1115", "#dedbd1", "#69717a", "#f0eee5", "#9fa84f", "#617d8d", "#d0b96b",
                "#7f9b54", "#bb8640", "#d6533e", "#8d9295", "#acb0a5", "#ef5a42",
            ],
        ),
        // Custom dark casters grounded in Dol Guldur's film language: black
        // violet sorcery, necromancer smoke, venom green, and bloody magenta.
        "uruk-lhuth" => set(
            &mut config,
            [
                "#100b18", "#e8dff1", "#755785", "#da55e8", "#30b833", "#7567a1", "#b9df68",
                "#59b89a", "#d0a64d", "#d85a9a", "#9a8ba5", "#afe768", "#b86bd5",
            ],
        ),
        // The Lord of the Rings' Haradrim: deep sun-baked crimson, Mumakil
        // leather, black-red shadow, brass, desert gold, and turquoise patina.
        "haradrim" => set(
            &mut config,
            [
                "#25100d", "#f4dec0", "#9f6144", "#f0b84f", "#e6533e", "#71999a", "#ffd16d",
                "#91ad55", "#e4a53c", "#ee4566", "#ac8e7c", "#69a8a2", "#f08b57",
            ],
        ),
        _ => return None,
    }
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_palettes_meet_contrast_targets_and_have_distinct_identity_anchors() {
        fn luminance(color: Color) -> f64 {
            let Color::Rgb(r, g, b) = color else {
                panic!("race palettes must use explicit RGB colors");
            };
            let linear = |channel: u8| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        }

        let mut identities: Vec<(&str, [Color; 3])> = Vec::new();
        for name in builtin_theme_names()
            .iter()
            .filter(|name| race_theme_name(name).is_some())
        {
            let theme = Theme::from_named(name).unwrap();
            let background = luminance(theme.background);
            for (role, color, minimum) in [
                ("foreground", theme.foreground, 7.0),
                ("title", theme.title, 4.5),
                ("accent", theme.accent, 4.5),
                ("muted", theme.muted, 4.5),
                ("success", theme.success, 4.5),
                ("warning", theme.warning, 4.5),
                ("danger", theme.danger, 4.5),
                ("player", theme.player, 4.5),
                ("enemy", theme.enemy, 4.5),
                ("border", theme.border, 3.0),
                ("rain", theme.rain, 3.0),
                ("lightning", theme.lightning, 4.5),
            ] {
                let foreground = luminance(color);
                let contrast =
                    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                assert!(contrast >= minimum, "{name}.{role}: contrast {contrast:.2}");
            }
            let identity = [theme.background, theme.title, theme.accent];
            for (previous_name, previous) in &identities {
                let distance = identity
                    .iter()
                    .zip(previous)
                    .flat_map(|(left, right)| {
                        let left = color_rgb(*left).unwrap();
                        let right = color_rgb(*right).unwrap();
                        [
                            left.0.abs_diff(right.0),
                            left.1.abs_diff(right.1),
                            left.2.abs_diff(right.2),
                        ]
                    })
                    .map(|difference| f64::from(difference).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(
                    distance >= 65.0,
                    "{name} and {previous_name} identity anchors are too similar: {distance:.2}"
                );
            }
            identities.push((name, identity));
        }
        assert_eq!(identities.len(), 10);
    }

    #[test]
    fn race_palettes_keep_their_cinematic_identity_anchors() {
        let roles = [
            "background",
            "foreground",
            "border",
            "title",
            "accent",
            "rain",
            "lightning",
            "success",
            "warning",
            "danger",
            "muted",
            "player",
            "enemy",
        ];
        for (name, expected) in [
            (
                "wood-elf",
                [
                    "#08191a", "#eef0e5", "#78908a", "#e5c56b", "#a7c7b7", "#678fa5", "#f4dfa0",
                    "#62b978", "#e2b85c", "#e57b64", "#a5b2ad", "#d9d6a2", "#c878a5",
                ],
            ),
            (
                "hobbit",
                [
                    "#142414", "#f7e7bd", "#789452", "#e8e04c", "#3aa63a", "#78a9ad", "#ffe09a",
                    "#82c85f", "#e7b957", "#dd7b60", "#a8ad88", "#d9ef8b", "#c77fa3",
                ],
            ),
            (
                "human",
                [
                    "#0c1621", "#f1f0e8", "#71869c", "#f2f0dc", "#7ca8d4", "#6f9fbd", "#f0d990",
                    "#83b38f", "#d9b45f", "#d67868", "#9aa7b3", "#dce8f0", "#c56591",
                ],
            ),
            (
                "dwarf",
                [
                    "#0c1918", "#f3e5c2", "#9b7440", "#f0be4f", "#20a985", "#5f9998", "#f5ce68",
                    "#55b47e", "#e0a83f", "#d87455", "#a69a82", "#f4d16f", "#c26086",
                ],
            ),
            (
                "beorning",
                [
                    "#24150b", "#f3dfba", "#9b7044", "#e8dc49", "#d09b3e", "#72918d", "#f4d783",
                    "#7fa44b", "#ebb34a", "#c96f4e", "#aa9077", "#d9b56b", "#c56791",
                ],
            ),
            (
                "uruk-hai",
                [
                    "#061514", "#dae3d4", "#536f5b", "#a8c77a", "#329154", "#578b8c", "#cdd784",
                    "#70ad65", "#ca9e4c", "#d2664f", "#87998d", "#96c77c", "#c65c8b",
                ],
            ),
            (
                "common-orc",
                [
                    "#18150d", "#dfd3b3", "#766943", "#cdbb88", "#e85c3f", "#6d8580", "#d7c67d",
                    "#87953e", "#c58a35", "#d84f6f", "#968d77", "#aeb052", "#e29355",
                ],
            ),
            (
                "olog-hai",
                [
                    "#0d1115", "#dedbd1", "#69717a", "#f0eee5", "#9fa84f", "#617d8d", "#d0b96b",
                    "#7f9b54", "#bb8640", "#d6533e", "#8d9295", "#acb0a5", "#ef5a42",
                ],
            ),
            (
                "uruk-lhuth",
                [
                    "#100b18", "#e8dff1", "#755785", "#da55e8", "#30b833", "#7567a1", "#b9df68",
                    "#59b89a", "#d0a64d", "#d85a9a", "#9a8ba5", "#afe768", "#b86bd5",
                ],
            ),
            (
                "haradrim",
                [
                    "#25100d", "#f4dec0", "#9f6144", "#f0b84f", "#e6533e", "#71999a", "#ffd16d",
                    "#91ad55", "#e4a53c", "#ee4566", "#ac8e7c", "#69a8a2", "#f08b57",
                ],
            ),
        ] {
            let theme = Theme::from_named(name).unwrap();
            let actual = [
                theme.background,
                theme.foreground,
                theme.border,
                theme.title,
                theme.accent,
                theme.rain,
                theme.lightning,
                theme.success,
                theme.warning,
                theme.danger,
                theme.muted,
                theme.player,
                theme.enemy,
            ];
            for ((actual, expected), role) in actual.into_iter().zip(expected).zip(roles) {
                assert_eq!(Some(actual), parse_color(expected), "{name}.{role}");
            }
        }
    }

    #[test]
    fn race_combat_roles_remain_distinct() {
        let distance = |left: Color, right: Color| {
            let left = color_rgb(left).unwrap();
            let right = color_rgb(right).unwrap();
            [
                left.0.abs_diff(right.0),
                left.1.abs_diff(right.1),
                left.2.abs_diff(right.2),
            ]
            .into_iter()
            .map(|difference| f64::from(difference).powi(2))
            .sum::<f64>()
            .sqrt()
        };
        for name in builtin_theme_names()
            .iter()
            .filter(|name| race_theme_name(name).is_some())
        {
            let theme = Theme::from_named(name).unwrap();
            let roles = [
                ("accent", theme.accent),
                ("success", theme.success),
                ("warning", theme.warning),
                ("player", theme.player),
                ("danger", theme.danger),
                ("enemy", theme.enemy),
            ];
            for (index, (left_name, left)) in roles.iter().enumerate() {
                for (right_name, right) in &roles[index + 1..] {
                    assert!(
                        distance(*left, *right) >= 24.0,
                        "{name}.{left_name}/{right_name} are too similar"
                    );
                }
            }
        }
    }

    #[test]
    fn lightning_defaults_to_bright_yellow_and_supports_panel_overrides() {
        let theme = Theme::from_config(&ThemeConfig::default());
        assert_eq!(theme.lightning, Color::Rgb(255, 255, 0));
        let mut options = PanelOptions::default();
        options
            .theme
            .insert("lightning".into(), "lightyellow".into());
        assert_eq!(theme.for_panel(&options).lightning, Color::LightYellow);
        assert_eq!(theme.lightning, Color::Rgb(255, 255, 0));
    }

    #[test]
    fn rain_color_parses_supported_formats_and_defaults() {
        assert_eq!(
            Theme::from_config(&ThemeConfig::default()).rain,
            Color::Rgb(97, 175, 239)
        );
        for (value, expected) in [
            ("blue", Color::Blue),
            ("index:123", Color::Indexed(123)),
            ("#123456", Color::Rgb(18, 52, 86)),
        ] {
            let config = ThemeConfig {
                rain: value.into(),
                ..ThemeConfig::default()
            };
            assert_eq!(Theme::from_config(&config).rain, expected);
        }
    }

    #[test]
    fn rain_panel_override_is_local_and_invalid_override_is_ignored() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut options = PanelOptions::default();
        options.theme.insert("rain".into(), "blue".into());
        let panel = theme.for_panel(&options);
        assert_eq!(panel.rain, Color::Blue);
        assert_eq!(panel.accent, theme.accent);
        assert_eq!(theme.rain, Color::Rgb(97, 175, 239));
        options.theme.insert("rain".into(), "invalid".into());
        assert_eq!(theme.for_panel(&options).rain, theme.rain);
    }

    #[test]
    fn built_in_themes_and_race_aliases_resolve() {
        for name in builtin_theme_names() {
            assert!(Theme::from_named(name).is_some(), "missing theme {name}");
        }
        assert_eq!(race_theme_name("Wood Elf"), Some("wood-elf"));
        assert_eq!(race_theme_name("HARADRIMS"), Some("haradrim"));
        assert_eq!(normalize_theme_name("Uruk_Lhuth"), "uruk-lhuth");
        assert!(Theme::from_named("unknown").is_none());
    }

    #[test]
    fn built_in_themes_declare_light_or_dark_appearance() {
        for name in builtin_theme_names() {
            let expected = if [
                "catppuccin-latte",
                "solarized-light",
                "gruvbox-light",
                "rose-pine-dawn",
                "github-light",
                "everforest-light",
                "kanagawa-lotus",
                "tokyo-night-day",
            ]
            .contains(name)
            {
                ThemeAppearance::Light
            } else {
                ThemeAppearance::Dark
            };
            assert_eq!(
                Theme::from_named(name).unwrap().appearance,
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn new_dark_themes_preserve_upstream_colors_and_readable_roles() {
        for (name, expected) in [
            (
                "everforest-dark",
                [
                    "#2d353b", "#d3c6aa", "#859289", "#a7c080", "#d699b6", "#7fbbb3", "#dbbc7f",
                    "#a7c080", "#dbbc7f", "#e67e80", "#9da9a0", "#83c092", "#e67e80",
                ],
            ),
            (
                "kanagawa-wave",
                [
                    "#1f1f28", "#dcd7ba", "#727169", "#7e9cd8", "#957fb8", "#7fb4ca", "#e6c384",
                    "#98bb6c", "#e6c384", "#ff5d62", "#c8c093", "#7aa89f", "#e46876",
                ],
            ),
            (
                "rose-pine-moon",
                [
                    "#232136", "#e0def4", "#908caa", "#9ccfd8", "#c4a7e7", "#3e8fb0", "#f6c177",
                    "#95b1ac", "#f6c177", "#eb6f92", "#908caa", "#9ccfd8", "#eb6f92",
                ],
            ),
        ] {
            let theme = Theme::from_named(name).unwrap();
            let actual = [
                theme.background,
                theme.foreground,
                theme.border,
                theme.title,
                theme.accent,
                theme.rain,
                theme.lightning,
                theme.success,
                theme.warning,
                theme.danger,
                theme.muted,
                theme.player,
                theme.enemy,
            ];
            assert_eq!(
                actual,
                expected.map(|color| parse_color(color).unwrap()),
                "{name}"
            );
            assert_eq!(theme.appearance, ThemeAppearance::Dark);
            for (index, color) in actual.iter().enumerate().skip(1) {
                let minimum = if index == 2 || index == 5 { 3.0 } else { 4.5 };
                assert!(
                    contrast_ratio(*color, theme.background) >= minimum,
                    "{name} role {index}"
                );
            }
            assert_ne!(theme.success, theme.warning);
            assert_ne!(theme.warning, theme.danger);
            assert_ne!(theme.player, theme.enemy);
            assert_ne!(theme.accent, theme.danger);
        }
    }

    #[test]
    fn popular_light_themes_use_their_upstream_core_palettes() {
        for (name, background, foreground, title, accent) in [
            (
                "solarized-light",
                "#fdf6e3",
                "#586e75",
                "#268bd2",
                "#6c71c4",
            ),
            ("gruvbox-light", "#fbf1c7", "#3c3836", "#076678", "#8f3f71"),
            ("rose-pine-dawn", "#faf4ed", "#464261", "#286983", "#907aa9"),
            ("github-light", "#ffffff", "#1f2328", "#0969da", "#8250df"),
        ] {
            let config = named_theme_config(name).unwrap();
            assert_eq!(config.appearance, ThemeAppearance::Light, "{name}");
            assert_eq!(
                Some(Theme::from_config(&config).background),
                parse_color(background),
                "{name}"
            );
            assert_eq!(
                Some(parse_color(&config.foreground).unwrap()),
                parse_color(foreground),
                "{name}"
            );
            assert_eq!(
                Some(parse_color(&config.title).unwrap()),
                parse_color(title),
                "{name}"
            );
            assert_eq!(
                Some(parse_color(&config.accent).unwrap()),
                parse_color(accent),
                "{name}"
            );
        }
    }

    #[test]
    fn remaining_theme_variants_preserve_palettes_and_contrast() {
        for (name, appearance, expected) in [
            (
                "everforest-light",
                ThemeAppearance::Light,
                [
                    "#fdf6e3", "#5c6a72", "#829181", "#3a94c5", "#df69ba", "#3a94c5", "#dfa000",
                    "#8da101", "#dfa000", "#f85552", "#829181", "#35a77c", "#f85552",
                ],
            ),
            (
                "kanagawa-dragon",
                ThemeAppearance::Dark,
                [
                    "#181616", "#c5c9c5", "#7a8382", "#8ba4b0", "#8992a7", "#8ba4b0", "#c4b28a",
                    "#87a987", "#c4b28a", "#c4746e", "#a6a69c", "#8ea4a2", "#c4746e",
                ],
            ),
            (
                "kanagawa-lotus",
                ThemeAppearance::Light,
                [
                    "#f2ecbc", "#545464", "#716e61", "#4d699b", "#624c83", "#6693bf", "#de9800",
                    "#6f894e", "#de9800", "#c84053", "#716e61", "#597b75", "#c84053",
                ],
            ),
            (
                "rose-pine",
                ThemeAppearance::Dark,
                [
                    "#191724", "#e0def4", "#908caa", "#9ccfd8", "#c4a7e7", "#31748f", "#f6c177",
                    "#95b1ac", "#f6c177", "#eb6f92", "#908caa", "#9ccfd8", "#eb6f92",
                ],
            ),
            (
                "tokyo-night-storm",
                ThemeAppearance::Dark,
                [
                    "#24283b", "#c0caf5", "#a9b1d6", "#7aa2f7", "#bb9af7", "#7aa2f7", "#e0af68",
                    "#9ece6a", "#e0af68", "#f7768e", "#a9b1d6", "#7dcfff", "#f7768e",
                ],
            ),
            (
                "tokyo-night-moon",
                ThemeAppearance::Dark,
                [
                    "#222436", "#c8d3f5", "#828bb8", "#82aaff", "#c099ff", "#82aaff", "#ffc777",
                    "#c3e88d", "#ffc777", "#ff757f", "#828bb8", "#86e1fc", "#ff757f",
                ],
            ),
            (
                "tokyo-night-day",
                ThemeAppearance::Light,
                [
                    "#e1e2e7", "#3760bf", "#6172b0", "#2e7de9", "#9854f1", "#2e7de9", "#8c6c3e",
                    "#587539", "#8c6c3e", "#f52a65", "#6172b0", "#007197", "#f52a65",
                ],
            ),
        ] {
            let config = named_theme_config(name).unwrap();
            let raw = [
                &config.background,
                &config.foreground,
                &config.border,
                &config.title,
                &config.accent,
                &config.rain,
                &config.lightning,
                &config.success,
                &config.warning,
                &config.danger,
                &config.muted,
                &config.player,
                &config.enemy,
            ];
            assert_eq!(raw.map(String::as_str), expected, "{name}");
            let theme = Theme::from_named(name).unwrap();
            assert_eq!(theme.appearance, appearance, "{name}");
            assert_eq!(Some(theme.background), parse_color(expected[0]), "{name}");
            let colors = [
                theme.foreground,
                theme.border,
                theme.title,
                theme.accent,
                theme.rain,
                theme.lightning,
                theme.success,
                theme.warning,
                theme.danger,
                theme.muted,
                theme.player,
                theme.enemy,
            ];
            for (index, color) in colors.iter().enumerate() {
                let minimum = if index == 1 || index == 4 { 3.0 } else { 4.5 };
                assert!(
                    contrast_ratio(*color, theme.background) >= minimum,
                    "{name} role {index}"
                );
                if appearance == ThemeAppearance::Dark {
                    assert_eq!(
                        Some(*color),
                        parse_color(expected[index + 1]),
                        "{name} role {index}"
                    );
                }
            }
            assert_ne!(theme.success, theme.warning);
            assert_ne!(theme.warning, theme.danger);
            assert_ne!(theme.player, theme.enemy);
        }
    }

    #[test]
    fn all_builtin_light_theme_roles_are_contrast_safe() {
        for name in [
            "catppuccin-latte",
            "solarized-light",
            "gruvbox-light",
            "rose-pine-dawn",
            "everforest-light",
            "kanagawa-lotus",
            "tokyo-night-day",
        ] {
            let theme = Theme::from_named(name).unwrap();
            for (role, color, minimum) in [
                ("foreground", theme.foreground, 4.5),
                ("border", theme.border, 3.0),
                ("title", theme.title, 4.5),
                ("accent", theme.accent, 4.5),
                ("rain", theme.rain, 3.0),
                ("lightning", theme.lightning, 4.5),
                ("success", theme.success, 4.5),
                ("warning", theme.warning, 4.5),
                ("danger", theme.danger, 4.5),
                ("muted", theme.muted, 4.5),
                ("player", theme.player, 4.5),
                ("enemy", theme.enemy, 4.5),
            ] {
                assert!(
                    contrast_ratio(color, theme.background) >= minimum,
                    "{name}.{role}"
                );
            }
        }
    }

    #[test]
    fn custom_light_theme_semantic_foregrounds_are_adapted() {
        let config = ThemeConfig {
            appearance: ThemeAppearance::Light,
            background: "#ffffff".into(),
            foreground: "#ffffff".into(),
            border: "#ffffff".into(),
            title: "#ffffff".into(),
            accent: "#ffffff".into(),
            rain: "#ffffff".into(),
            lightning: "#ffffff".into(),
            success: "#ffffff".into(),
            warning: "#ffffff".into(),
            danger: "#ffffff".into(),
            muted: "#ffffff".into(),
            player: "#ffffff".into(),
            enemy: "#ffffff".into(),
        };
        let theme = Theme::from_config(&config);

        for (color, minimum) in [
            (theme.foreground, 4.5),
            (theme.border, 3.0),
            (theme.title, 4.5),
            (theme.accent, 4.5),
            (theme.rain, 3.0),
            (theme.lightning, 4.5),
            (theme.success, 4.5),
            (theme.warning, 4.5),
            (theme.danger, 4.5),
            (theme.muted, 4.5),
            (theme.player, 4.5),
            (theme.enemy, 4.5),
        ] {
            assert!(contrast_ratio(color, theme.background) >= minimum);
        }
    }

    #[test]
    fn actual_background_adapts_low_contrast_text_and_map_colors() {
        let latte = Theme::from_named("catppuccin-latte").unwrap();
        assert_eq!(
            latte.readable_text_color(Color::Black, latte.background),
            Color::Black
        );
        let white_text = latte.readable_text_color(Color::White, latte.background);
        let white_map = latte.readable_map_color(Color::White);
        assert_ne!(white_text, Color::White);
        assert_ne!(white_map, Color::White);
        assert!(contrast_ratio(white_text, latte.background) >= 4.5);
        assert!(contrast_ratio(white_map, latte.background) >= 3.0);

        let mocha = Theme::from_named("catppuccin-mocha").unwrap();
        let black_text = mocha.readable_text_color(Color::Black, mocha.background);
        assert_ne!(black_text, Color::Black);
        assert!(contrast_ratio(black_text, mocha.background) >= 4.5);

        for (theme, foreground, background) in [
            (&latte, Color::Black, Color::Black),
            (&latte, Color::Indexed(16), Color::Indexed(16)),
            (&latte, Color::Rgb(1, 2, 3), Color::Rgb(1, 2, 3)),
            (&mocha, Color::White, Color::White),
            (&mocha, Color::Indexed(231), Color::Indexed(231)),
            (&mocha, Color::Rgb(250, 251, 252), Color::Rgb(250, 251, 252)),
        ] {
            let adapted = theme.readable_text_color(foreground, background);
            assert!(contrast_ratio(adapted, background) >= 4.5);
        }
    }

    #[test]
    fn indexed_colors_and_selection_foregrounds_are_contrast_safe() {
        let latte = Theme::from_named("catppuccin-latte").unwrap();
        let adapted = latte.readable_text_color(Color::Indexed(231), latte.background);
        assert!(contrast_ratio(adapted, latte.background) >= 4.5);
        assert!(contrast_ratio(latte.contrasting_foreground(latte.accent), latte.accent) >= 4.5);
        assert!(contrast_ratio(latte.contrasting_foreground(latte.warning), latte.warning) >= 4.5);
    }

    #[test]
    fn custom_theme_appearance_deserializes_and_rejects_unknown_values() {
        let light: ThemeConfig = toml::from_str("appearance = 'light'").unwrap();
        assert_eq!(light.appearance, ThemeAppearance::Light);
        let defaulted: ThemeConfig = toml::from_str("").unwrap();
        assert_eq!(defaulted.appearance, ThemeAppearance::Dark);
        assert!(toml::from_str::<ThemeConfig>("appearance = 'sepia'").is_err());
    }
}
