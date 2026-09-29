use std::time::{Duration, Instant};

use crate::config::{AnimationConfig, WeatherConfig};

use super::{
    scheduler::{AnimationDefinition, AnimationScheduler},
    weather::WeatherKind,
};

const EFFECT: &str = "weather";
const LOW_PERFORMANCE_FPS: u64 = 2;
pub const SCENE_PHASES: usize = 240;
const LIGHTNING_DURATION: Duration = Duration::from_millis(500);
const LIGHTNING_MIN_INTERVAL_MS: u64 = 1_000;
const LIGHTNING_MAX_INTERVAL_MS: u64 = 5_000;

/// Owns weather timing; widgets only consume the resulting frame index.
#[derive(Debug, Default)]
pub struct WeatherPlayback {
    active: Option<(WeatherKind, u64)>,
    lightning: LightningPlayback,
}

#[derive(Debug)]
struct LightningPlayback {
    random_state: u64,
    started: Option<Instant>,
    next: Option<Instant>,
    visible: bool,
    bolts: u8,
}

impl Default for LightningPlayback {
    fn default() -> Self {
        Self {
            random_state: 0x726f_7473_7374_6f72,
            started: None,
            next: None,
            visible: false,
            bolts: 0,
        }
    }
}

impl LightningPlayback {
    fn random(&mut self) -> u64 {
        // Deterministic xorshift for visual variation, not cryptography.
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;
        self.random_state
    }

    fn interval(&mut self) -> Duration {
        let range = LIGHTNING_MAX_INTERVAL_MS - LIGHTNING_MIN_INTERVAL_MS + 1;
        Duration::from_millis(LIGHTNING_MIN_INTERVAL_MS + self.random() % range)
    }

    fn update(&mut self, storm: bool, moving: bool, now: Instant) {
        if !storm || !moving {
            self.started = None;
            self.next = None;
            self.visible = storm;
            self.bolts = if storm { 2 } else { 0 };
            return;
        }
        match self.next {
            None => {
                self.bolts = (self.random() % 3 + 1) as u8;
                self.started = Some(now);
                self.next = Some(now + self.interval());
            }
            Some(next) if now >= next => {
                if now.duration_since(next) < LIGHTNING_DURATION {
                    self.bolts = (self.random() % 3 + 1) as u8;
                    self.started = Some(next);
                    self.next = Some(next + self.interval());
                } else {
                    // Skip missed flashes after a pause without unbounded catch-up.
                    self.started = None;
                    self.next = Some(now + self.interval());
                }
            }
            Some(_) => {}
        }
        self.visible = self
            .started
            .is_some_and(|start| now.saturating_duration_since(start) < LIGHTNING_DURATION);
    }
}

impl WeatherPlayback {
    pub fn lightning_bolts(&self) -> u8 {
        if self.lightning.visible {
            self.lightning.bolts
        } else {
            0
        }
    }

    pub fn update(
        &mut self,
        kind: WeatherKind,
        weather: &WeatherConfig,
        animation: &AnimationConfig,
        scheduler: &mut AnimationScheduler,
        now: Instant,
    ) -> usize {
        let moving = weather.enabled
            && weather.show_info_marker
            && animation.enabled
            && !animation.reduced_motion
            && !matches!(kind, WeatherKind::Indoor | WeatherKind::Unknown);
        self.lightning.update(
            weather.enabled && weather.show_info_marker && kind == WeatherKind::Storm,
            moving,
            now,
        );
        let fps = if animation.low_performance {
            animation.weather_fps.min(LOW_PERFORMANCE_FPS)
        } else {
            animation.weather_fps
        };
        let desired = moving.then_some((kind, fps));
        if desired != self.active {
            scheduler.cancel(EFFECT);
            if let Some((_, fps)) = desired {
                scheduler.start(
                    AnimationDefinition {
                        name: EFFECT.into(),
                        frames: SCENE_PHASES,
                        frame_rate_fps: fps,
                        duration: Duration::ZERO,
                        looping: true,
                    },
                    now,
                );
            }
            self.active = desired;
        }
        scheduler.frame(EFFECT, now).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightning_intervals_are_seeded_varied_and_bounded() {
        let mut first = LightningPlayback::default();
        let mut second = LightningPlayback::default();
        let intervals: Vec<_> = (0..1_000).map(|_| first.interval()).collect();
        assert_eq!(
            intervals,
            (0..1_000).map(|_| second.interval()).collect::<Vec<_>>()
        );
        for interval in &intervals {
            assert!((1_000..=5_000).contains(&interval.as_millis()));
        }
        for bucket in 1..5 {
            assert!(
                intervals
                    .iter()
                    .any(|interval| interval.as_millis() / 1_000 == bucket)
            );
        }
        assert!(intervals.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn lightning_uses_elapsed_time_and_skips_missed_flashes() {
        let mut lightning = LightningPlayback::default();
        let now = Instant::now();
        lightning.update(true, true, now);
        assert!(lightning.visible);
        let next = lightning.next.unwrap();
        lightning.update(true, true, now + Duration::from_millis(499));
        assert!(lightning.visible);
        assert_eq!(lightning.next, Some(next));
        lightning.update(true, true, now + LIGHTNING_DURATION);
        assert!(!lightning.visible);
        lightning.update(true, true, next);
        assert!(lightning.visible);
        let later = now + Duration::from_secs(86_400);
        lightning.update(true, true, later);
        assert!(!lightning.visible);
        let next = lightning.next.unwrap();
        assert!(
            (Duration::from_secs(1)..=Duration::from_secs(5)).contains(&next.duration_since(later))
        );
        lightning.update(true, true, next);
        assert!(lightning.visible);
    }

    #[test]
    fn strikes_choose_one_to_three_bolts_deterministically() {
        let mut lightning = LightningPlayback::default();
        let mut repeated = LightningPlayback::default();
        let mut now = Instant::now();
        let mut counts = [false; 3];
        for _ in 0..100 {
            lightning.update(true, true, now);
            repeated.update(true, true, now);
            assert!(lightning.visible);
            assert_eq!(lightning.bolts, repeated.bolts);
            assert!((1..=3).contains(&lightning.bolts));
            counts[usize::from(lightning.bolts - 1)] = true;
            now = lightning.next.unwrap();
        }
        assert_eq!(counts, [true; 3]);
    }

    #[test]
    fn lightning_rate_is_independent_of_scene_fps_and_low_performance() {
        let weather = WeatherConfig::default();
        let mut animation = AnimationConfig {
            weather_fps: 1,
            ..AnimationConfig::default()
        };
        let mut scheduler = AnimationScheduler::new(&animation);
        let mut playback = WeatherPlayback::default();
        let now = Instant::now();
        playback.update(
            WeatherKind::Storm,
            &weather,
            &animation,
            &mut scheduler,
            now,
        );
        assert!((1..=3).contains(&playback.lightning_bolts()));
        let next = playback.lightning.next;
        animation.low_performance = true;
        animation.weather_fps = 30;
        playback.update(
            WeatherKind::Storm,
            &weather,
            &animation,
            &mut scheduler,
            now + LIGHTNING_DURATION,
        );
        assert_eq!(playback.lightning_bolts(), 0);
        assert_eq!(playback.lightning.next, next);
    }

    #[test]
    fn lightning_toggles_and_weather_changes_clear_timers() {
        let mut weather = WeatherConfig::default();
        let mut animation = AnimationConfig::default();
        let mut scheduler = AnimationScheduler::new(&animation);
        let mut playback = WeatherPlayback::default();
        let now = Instant::now();
        for (enabled, reduced_motion) in [(false, false), (true, true)] {
            animation.enabled = enabled;
            animation.reduced_motion = reduced_motion;
            playback.update(
                WeatherKind::Storm,
                &weather,
                &animation,
                &mut scheduler,
                now,
            );
            assert_eq!(playback.lightning_bolts(), 2);
            assert!(playback.lightning.next.is_none());
        }
        animation.enabled = true;
        animation.reduced_motion = false;
        for (enabled, marker) in [(false, true), (true, false), (true, true)] {
            weather.enabled = enabled;
            weather.show_info_marker = marker;
            playback.update(
                WeatherKind::Storm,
                &weather,
                &animation,
                &mut scheduler,
                now,
            );
            assert_eq!(playback.lightning_bolts() > 0, enabled && marker);
        }
        for kind in [WeatherKind::Rain, WeatherKind::Indoor, WeatherKind::Unknown] {
            playback.update(kind, &weather, &animation, &mut scheduler, now);
            assert_eq!(playback.lightning_bolts(), 0);
            assert!(playback.lightning.next.is_none());
            playback.update(
                WeatherKind::Storm,
                &weather,
                &animation,
                &mut scheduler,
                now,
            );
            assert!((1..=3).contains(&playback.lightning_bolts()));
        }
    }

    #[test]
    fn elapsed_time_drives_frames_and_weather_changes_restart() {
        let animation = AnimationConfig::default();
        let weather = WeatherConfig::default();
        let mut scheduler = AnimationScheduler::new(&animation);
        let mut playback = WeatherPlayback::default();
        let now = Instant::now();
        assert_eq!(
            playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now),
            0
        );
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_millis(499)
            ),
            0
        );
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_millis(500)
            ),
            1
        );
        assert_eq!(
            playback.update(
                WeatherKind::Snow,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_millis(550)
            ),
            0
        );
    }

    #[test]
    fn toggles_and_rate_changes_cancel_or_restart() {
        let mut animation = AnimationConfig {
            weather_fps: 10,
            ..AnimationConfig::default()
        };
        let mut weather = WeatherConfig::default();
        let mut scheduler = AnimationScheduler::new(&animation);
        let mut playback = WeatherPlayback::default();
        let now = Instant::now();
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        animation.enabled = false;
        scheduler.configure(&animation);
        assert_eq!(
            playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now),
            0
        );
        assert!(scheduler.frame(EFFECT, now).is_none());
        animation.enabled = true;
        scheduler.configure(&animation);
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        weather.show_info_marker = false;
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        assert!(scheduler.frame(EFFECT, now).is_none());
        weather.show_info_marker = true;
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        animation.weather_fps = 2;
        let later = now + Duration::from_secs(1);
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                later
            ),
            0
        );
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                later + Duration::from_millis(500)
            ),
            1
        );
        playback.update(
            WeatherKind::Unknown,
            &weather,
            &animation,
            &mut scheduler,
            later,
        );
        assert!(scheduler.frame(EFFECT, now).is_none());
    }

    #[test]
    fn settings_stop_restart_and_slow_the_loop() {
        let mut animation = AnimationConfig {
            weather_fps: 10,
            ..AnimationConfig::default()
        };
        let mut weather = WeatherConfig::default();
        let mut scheduler = AnimationScheduler::new(&animation);
        let mut playback = WeatherPlayback::default();
        let now = Instant::now();
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        animation.reduced_motion = true;
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_secs(1)
            ),
            0
        );
        assert!(scheduler.frame(EFFECT, now).is_none());
        animation.reduced_motion = false;
        animation.low_performance = true;
        playback.update(WeatherKind::Rain, &weather, &animation, &mut scheduler, now);
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_millis(100)
            ),
            0
        );
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_millis(500)
            ),
            1
        );
        weather.enabled = false;
        assert_eq!(
            playback.update(
                WeatherKind::Rain,
                &weather,
                &animation,
                &mut scheduler,
                now + Duration::from_secs(1)
            ),
            0
        );
        assert!(scheduler.frame(EFFECT, now).is_none());
    }
}
