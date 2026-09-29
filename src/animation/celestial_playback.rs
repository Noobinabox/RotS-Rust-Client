//! Short presentation fades; the server remains authoritative for sky position.

use std::time::{Duration, Instant};

use crate::config::{AnimationConfig, WeatherConfig};

use super::{
    daylight::{CelestialPosition, SkyClock},
    scheduler::{AnimationDefinition, AnimationScheduler},
    weather::WeatherKind,
};

const EFFECT: &str = "celestial-fade";
const FADE_DURATION: Duration = Duration::from_secs(2);

#[derive(Debug)]
struct Fade {
    started_at: Instant,
    outgoing: bool,
    start_opacity: u8,
    fps: u64,
}

/// Keeps outgoing bodies alive briefly without retaining stale weather or clocks.
#[derive(Debug, Default)]
pub struct CelestialPlayback {
    displayed: Option<CelestialPosition>,
    opacity: u8,
    fade: Option<Fade>,
}

impl CelestialPlayback {
    pub fn update(
        &mut self,
        target: SkyClock,
        kind: WeatherKind,
        settings: (&WeatherConfig, &AnimationConfig),
        scheduler: &mut AnimationScheduler,
        now: Instant,
    ) -> SkyClock {
        let (weather, animation) = settings;
        let visible = weather.enabled
            && weather.show_info_marker
            && matches!(kind, WeatherKind::Clear | WeatherKind::Cloudy)
            && target.celestial.is_some();
        if !visible {
            self.displayed = None;
            self.opacity = 255;
            self.fade = None;
            scheduler.cancel(EFFECT);
            return SkyClock {
                celestial: None,
                ..target
            };
        }
        if !animation.enabled || animation.reduced_motion {
            self.displayed = target.celestial;
            self.opacity = 255;
            self.fade = None;
            scheduler.cancel(EFFECT);
            return SkyClock {
                opacity: 255,
                ..target
            };
        }
        let fps = if animation.low_performance {
            animation.weather_fps.min(2)
        } else {
            animation.weather_fps
        }
        .clamp(1, 1000);

        // Rate changes preserve the original deadline rather than restarting a fade.
        if let Some(fade) = &mut self.fade
            && fade.fps != fps
        {
            fade.fps = fps;
            start_effect(scheduler, fade);
        }
        self.advance(target.celestial, fps, scheduler, now);
        let outgoing = self.fade.as_ref().is_some_and(|fade| fade.outgoing);
        if !outgoing {
            match (self.displayed, target.celestial) {
                (None, Some(position)) => {
                    self.displayed = Some(position);
                    self.opacity = 0;
                    self.begin(false, fps, scheduler, now);
                }
                (Some(current), Some(next)) if current.moon != next.moon => {
                    self.begin(true, fps, scheduler, now);
                }
                _ => self.displayed = target.celestial,
            }
        }
        SkyClock {
            daylight: target.daylight,
            celestial: self.displayed,
            opacity: self.opacity,
        }
    }

    fn begin(
        &mut self,
        outgoing: bool,
        fps: u64,
        scheduler: &mut AnimationScheduler,
        now: Instant,
    ) {
        let fade = Fade {
            started_at: now,
            outgoing,
            start_opacity: self.opacity,
            fps,
        };
        start_effect(scheduler, &fade);
        self.fade = Some(fade);
    }

    fn advance(
        &mut self,
        target: Option<CelestialPosition>,
        fps: u64,
        scheduler: &mut AnimationScheduler,
        now: Instant,
    ) {
        // At most two legs: late timer ticks can finish the entire transition.
        for _ in 0..2 {
            let Some(fade) = &self.fade else { return };
            if now.saturating_duration_since(fade.started_at) >= FADE_DURATION {
                let outgoing = fade.outgoing;
                let boundary = fade.started_at + FADE_DURATION;
                self.fade = None;
                scheduler.cancel(EFFECT);
                if outgoing {
                    self.displayed = target;
                    self.opacity = 0;
                    self.begin(false, fps, scheduler, boundary);
                    continue;
                }
                self.opacity = 255;
                return;
            }
            let frame = scheduler.frame(EFFECT, now).unwrap_or(0) as u64;
            let elapsed_ms = frame.saturating_mul(1000 / fade.fps);
            let progress = elapsed_ms.min(FADE_DURATION.as_millis() as u64);
            let duration = FADE_DURATION.as_millis() as u64;
            self.opacity = if fade.outgoing {
                (u64::from(fade.start_opacity) * (duration - progress) / duration) as u8
            } else {
                (255 * progress / duration) as u8
            };
            return;
        }
    }
}

fn start_effect(scheduler: &mut AnimationScheduler, fade: &Fade) {
    scheduler.start(
        AnimationDefinition {
            name: EFFECT.into(),
            frames: (FADE_DURATION.as_millis() as u64 / (1000 / fade.fps) + 1) as usize,
            frame_rate_fps: fade.fps,
            duration: FADE_DURATION,
            looping: false,
        },
        fade.started_at,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        playback: CelestialPlayback,
        scheduler: AnimationScheduler,
        animation: AnimationConfig,
        weather: WeatherConfig,
        start: Instant,
    }

    impl Harness {
        fn new() -> Self {
            let animation = AnimationConfig::default();
            Self {
                playback: CelestialPlayback::default(),
                scheduler: AnimationScheduler::new(&animation),
                animation,
                weather: WeatherConfig::default(),
                start: Instant::now(),
            }
        }

        fn at(&mut self, millis: u64, time: Option<&str>, kind: WeatherKind) -> SkyClock {
            let now = self.start + Duration::from_millis(millis);
            self.scheduler.configure(&self.animation);
            self.scheduler.tick(now);
            self.playback.update(
                SkyClock::from_world_time(time),
                kind,
                (&self.weather, &self.animation),
                &mut self.scheduler,
                now,
            )
        }

        fn clear(&mut self, millis: u64, time: &str) -> SkyClock {
            self.at(millis, Some(time), WeatherKind::Clear)
        }
    }

    #[test]
    fn first_appearance_fades_and_repeated_clock_packets_do_not_restart() {
        for clock in ["6:00 AM", "7:00 PM"] {
            let mut test = Harness::new();
            assert_eq!(test.clear(0, clock).opacity, 0);
            assert_eq!(test.clear(499, clock).opacity, 0);
            assert_eq!(test.clear(1000, clock).opacity, 127);
            assert_eq!(test.clear(2000, clock).opacity, 255);
            assert_eq!(test.clear(3000, clock).opacity, 255);
            assert!(test.scheduler.frame(EFFECT, test.start).is_none());
        }
    }

    #[test]
    fn both_body_changes_fade_out_then_in_using_target_daylight() {
        for (before, after) in [("6:00 PM", "7:00 PM"), ("5:00 AM", "6:00 AM")] {
            let mut test = Harness::new();
            let old = test.clear(0, before).celestial;
            test.clear(2000, before);
            assert_eq!(test.clear(3000, after).celestial, old);
            let middle = test.clear(4000, after);
            assert_eq!(middle.celestial, old);
            assert_eq!(middle.opacity, 127);
            assert_eq!(
                middle.daylight,
                SkyClock::from_world_time(Some(after)).daylight
            );
            let boundary = test.clear(5000, after);
            assert_eq!(boundary.opacity, 0);
            assert_eq!(
                boundary.celestial,
                SkyClock::from_world_time(Some(after)).celestial
            );
            assert_eq!(test.clear(6000, after).opacity, 127);
            assert_eq!(test.clear(7000, after).opacity, 255);
        }
    }

    #[test]
    fn same_body_position_updates_keep_fade_deadline_and_late_ticks_complete() {
        let mut test = Harness::new();
        test.clear(0, "6:00 AM");
        let advanced = test.clear(1000, "8:00 AM");
        assert_eq!(advanced.opacity, 127);
        assert_eq!(
            advanced.celestial,
            SkyClock::from_world_time(Some("8:00 AM")).celestial
        );
        assert_eq!(test.clear(2000, "8:00 AM").opacity, 255);
        test.clear(3000, "8:00 PM");
        let late = test.clear(10000, "8:00 PM");
        assert_eq!(late.opacity, 255);
        assert!(late.celestial.unwrap().moon);
    }

    #[test]
    fn rapid_targets_do_not_restart_outgoing_leg() {
        let mut test = Harness::new();
        test.clear(0, "6:00 AM");
        test.clear(2000, "6:00 AM");
        test.clear(3000, "7:00 PM");
        assert_eq!(test.clear(4000, "7:00 AM").opacity, 127);
        assert_eq!(test.clear(4500, "8:00 PM").opacity, 63);
        assert_eq!(test.clear(7000, "8:00 PM").opacity, 255);
        test.clear(8000, "7:00 AM");
        test.clear(10000, "7:00 AM");
        assert_eq!(test.clear(11000, "8:00 PM").opacity, 127);
        assert_eq!(test.clear(15000, "8:00 PM").opacity, 255);
    }

    #[test]
    fn clouds_preserve_celestial_fades_and_day_night_transitions() {
        let mut test = Harness::new();
        assert_eq!(test.at(0, Some("6:00 AM"), WeatherKind::Cloudy).opacity, 0);
        assert_eq!(test.clear(1000, "6:00 AM").opacity, 127);
        assert_eq!(
            test.at(2000, Some("6:00 AM"), WeatherKind::Cloudy).opacity,
            255
        );
        let outgoing = test.at(3000, Some("7:00 PM"), WeatherKind::Cloudy);
        assert!(!outgoing.celestial.unwrap().moon);
        let incoming = test.at(5000, Some("7:00 PM"), WeatherKind::Cloudy);
        assert!(incoming.celestial.unwrap().moon);
        assert_eq!(incoming.opacity, 0);
        assert_eq!(
            test.at(7000, Some("7:00 PM"), WeatherKind::Cloudy).opacity,
            255
        );
    }

    #[test]
    fn hidden_weather_invalid_time_and_visibility_toggles_clear_old_bodies() {
        for kind in [WeatherKind::Rain, WeatherKind::Indoor, WeatherKind::Unknown] {
            let mut test = Harness::new();
            test.clear(0, "6:00 AM");
            assert!(test.at(1000, Some("7:00 PM"), kind).celestial.is_none());
            let resumed = test.clear(2000, "7:00 PM");
            assert_eq!(resumed.opacity, 0);
            assert!(resumed.celestial.unwrap().moon);
            assert!(test.at(3000, None, WeatherKind::Clear).celestial.is_none());
            assert_eq!(test.clear(4000, "6:00 AM").opacity, 0);
            test.weather.enabled = false;
            assert!(test.clear(4500, "6:00 AM").celestial.is_none());
            test.weather.enabled = true;
            test.weather.show_info_marker = false;
            assert!(test.clear(5000, "6:00 AM").celestial.is_none());
            assert!(test.scheduler.frame(EFFECT, test.start).is_none());
        }
    }

    #[test]
    fn accessibility_settings_finish_immediately_and_low_performance_limits_frames() {
        let mut test = Harness::new();
        test.animation.weather_fps = 10;
        test.animation.low_performance = true;
        test.clear(0, "6:00 AM");
        assert_eq!(test.clear(100, "6:00 AM").opacity, 0);
        assert_eq!(test.clear(500, "6:00 AM").opacity, 63);
        test.animation.low_performance = false;
        assert_eq!(test.clear(600, "6:00 AM").opacity, 76);
        test.animation.reduced_motion = true;
        assert_eq!(test.clear(700, "7:00 PM").opacity, 255);
        assert!(test.scheduler.frame(EFFECT, test.start).is_none());
        test.animation.reduced_motion = false;
        assert_eq!(test.clear(800, "7:00 PM").opacity, 255);
        test.animation.enabled = false;
        let day = test.clear(900, "6:00 AM");
        assert_eq!(day.opacity, 255);
        assert!(!day.celestial.unwrap().moon);
    }
}
