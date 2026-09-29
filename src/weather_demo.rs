//! Standalone, offline preview. No application, persistence, scripts, or network.
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use mud_client::{
    animation::{
        celestial_playback::CelestialPlayback, daylight::SkyClock, scheduler::AnimationScheduler,
        weather::WeatherKind, weather_playback::WeatherPlayback,
    },
    config::{AnimationConfig, ThemeConfig, WeatherConfig},
    error::Result,
    terminal::TerminalGuard,
    ui::{theme::Theme, weather::render_weather},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    widgets::{Block, Borders, Paragraph, Wrap},
};

const FRAME_INTERVAL: Duration = Duration::from_millis(100);
const HOUR_INTERVAL: Duration = Duration::from_secs(6);
const WEATHER: [(WeatherKind, &str); 10] = [
    (WeatherKind::Clear, "Clear (sun/moon)"),
    (WeatherKind::Cloudy, "Cloudy"),
    (WeatherKind::Rain, "Rain"),
    (WeatherKind::Storm, "Storm/lightning"),
    (WeatherKind::Snow, "Snow"),
    (WeatherKind::Blizzard, "Blizzard"),
    (WeatherKind::Fog, "Fog"),
    (WeatherKind::Wind, "Wind"),
    (WeatherKind::Ash, "Ash"),
    (WeatherKind::Dust, "Dust"),
];

struct DemoState {
    weather: usize,
    hour: u8,
    paused: bool,
    elapsed: Duration,
    hour_elapsed: Duration,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            weather: 0,
            hour: 5,
            paused: false,
            elapsed: Duration::ZERO,
            hour_elapsed: Duration::ZERO,
        }
    }
}

impl DemoState {
    fn advance(&mut self, delta: Duration) {
        if self.paused {
            return;
        }
        self.elapsed += delta;
        self.hour_elapsed += delta;
        let hours = self.hour_elapsed.as_nanos() / HOUR_INTERVAL.as_nanos();
        self.hour = (u128::from(self.hour) + hours).rem_euclid(24) as u8;
        self.hour_elapsed =
            Duration::from_nanos((self.hour_elapsed.as_nanos() % HOUR_INTERVAL.as_nanos()) as u64);
    }

    /// Return true only for an explicit exit key; key releases do nothing.
    fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.code == KeyCode::Esc
            || key.code == KeyCode::Char('q')
            || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'C')))
        {
            return true;
        }
        match key.code {
            KeyCode::Left => self.weather = (self.weather + WEATHER.len() - 1) % WEATHER.len(),
            KeyCode::Right => self.weather = (self.weather + 1) % WEATHER.len(),
            KeyCode::Up | KeyCode::Down => {
                self.hour = (self.hour + if key.code == KeyCode::Up { 1 } else { 23 }) % 24;
                self.hour_elapsed = Duration::ZERO;
            }
            KeyCode::Char(' ') if key.kind == KeyEventKind::Press => self.paused = !self.paused,
            _ => {}
        }
        false
    }

    fn clock_label(&self) -> String {
        let hour = match self.hour % 12 {
            0 => 12,
            hour => hour,
        };
        format!("{hour}:00 {}", if self.hour < 12 { "AM" } else { "PM" })
    }
}

pub fn run() -> Result<()> {
    let animation = AnimationConfig::default();
    let weather = WeatherConfig::default();
    let theme = Theme::from_config(&ThemeConfig::default());
    let mut scheduler = AnimationScheduler::new(&animation);
    let mut weather_playback = WeatherPlayback::default();
    let mut celestial_playback = CelestialPlayback::default();
    let mut state = DemoState::default();
    let origin = Instant::now();
    let mut previous = origin;
    let (_guard, mut terminal) = TerminalGuard::enter(false)?;
    loop {
        let now = Instant::now();
        state.advance(now.saturating_duration_since(previous));
        previous = now;
        // A virtual monotonic clock freezes every animation during pause.
        let animation_now = origin + state.elapsed;
        let kind = WEATHER[state.weather].0;
        let phase =
            weather_playback.update(kind, &weather, &animation, &mut scheduler, animation_now);
        let sky = celestial_playback.update(
            SkyClock::from_world_time(Some(&state.clock_label())),
            kind,
            (&weather, &animation),
            &mut scheduler,
            animation_now,
        );
        terminal.draw(|frame| {
            render(
                frame,
                &state,
                phase,
                sky,
                &theme,
                weather_playback.lightning_bolts(),
            )
        })?;
        if event::poll(FRAME_INTERVAL)?
            && let Event::Key(key) = event::read()?
            && state.key(key)
        {
            return Ok(());
        }
    }
}

fn render(
    frame: &mut Frame<'_>,
    state: &DemoState,
    phase: usize,
    sky: SkyClock,
    theme: &Theme,
    lightning_bolts: u8,
) {
    let [header, world, controls] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.foreground)),
        frame.area(),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "OFFLINE WEATHER DEMO | {} | {} | {}",
            WEATHER[state.weather].1,
            state.clock_label(),
            if state.paused {
                "paused"
            } else {
                "6 seconds / game hour"
            },
        ))
        .wrap(Wrap { trim: false }),
        header,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" World — West ← East (north up) ");
    let inner = block.inner(world);
    frame.render_widget(block, world);
    render_weather(
        inner,
        frame.buffer_mut(),
        WEATHER[state.weather].0,
        phase,
        theme,
        sky,
        lightning_bolts,
    );
    frame.render_widget(Paragraph::new(
        "Left/Right: weather | Up/Down: hour | Space: pause/resume | q/Esc/Ctrl-C: quit\nClear/cloudy skies show sun/moon. No connection; no configuration loaded or saved."
    ).wrap(Wrap { trim: false }), controls);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn weather_and_hour_controls_wrap_both_directions() {
        let mut state = DemoState::default();
        state.key(press(KeyCode::Left));
        assert_eq!(WEATHER[state.weather].0, WeatherKind::Dust);
        for (kind, _) in WEATHER {
            state.key(press(KeyCode::Right));
            assert_eq!(WEATHER[state.weather].0, kind);
        }
        state.hour = 23;
        state.key(press(KeyCode::Up));
        assert_eq!(state.hour, 0);
        state.key(press(KeyCode::Down));
        assert_eq!(state.hour, 23);
    }

    #[test]
    fn elapsed_clock_pause_and_manual_selection_are_deterministic() {
        let mut state = DemoState::default();
        state.advance(HOUR_INTERVAL - Duration::from_millis(1));
        assert_eq!(state.hour, 5);
        state.advance(Duration::from_millis(1));
        assert_eq!(state.hour, 6);
        state.key(press(KeyCode::Char(' ')));
        state.advance(Duration::from_secs(600));
        assert_eq!(state.hour, 6);
        assert_eq!(state.elapsed, HOUR_INTERVAL);
        state.key(press(KeyCode::Up));
        assert_eq!(state.hour, 7);
        state.key(press(KeyCode::Char(' ')));
        state.advance(HOUR_INTERVAL * 24);
        assert_eq!(state.hour, 7);
        state.hour = 0;
        assert_eq!(state.clock_label(), "12:00 AM");
        state.hour = 12;
        assert_eq!(state.clock_label(), "12:00 PM");
    }

    #[test]
    fn releases_and_repeat_pause_are_ignored_and_exit_keys_work() {
        let mut state = DemoState::default();
        let mut key = press(KeyCode::Char(' '));
        key.kind = KeyEventKind::Repeat;
        state.key(key);
        assert!(!state.paused);
        key = press(KeyCode::Char('q'));
        key.kind = KeyEventKind::Release;
        assert!(!state.key(key));
        for key in [
            press(KeyCode::Char('q')),
            press(KeyCode::Esc),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            assert!(state.key(key));
        }
    }

    #[test]
    fn every_weather_renders_at_tiny_and_normal_sizes() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for (width, height) in [(1, 1), (5, 3), (30, 8), (100, 30)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            for weather in 0..WEATHER.len() {
                let state = DemoState {
                    weather,
                    ..DemoState::default()
                };
                terminal
                    .draw(|frame| {
                        render(
                            frame,
                            &state,
                            15,
                            SkyClock::from_world_time(Some("12:00 PM")),
                            &theme,
                            3,
                        )
                    })
                    .unwrap();
            }
        }
    }
}
