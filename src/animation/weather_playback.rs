use std::time::{Duration, Instant};

use crate::config::{AnimationConfig, WeatherConfig};

use super::{
    scheduler::{AnimationDefinition, AnimationScheduler},
    weather::WeatherKind,
};

const EFFECT: &str = "weather";
const LOW_PERFORMANCE_FPS: u64 = 2;
pub const SCENE_PHASES: usize = 240;

/// Owns weather timing; widgets only consume the resulting frame index.
#[derive(Debug, Default)]
pub struct WeatherPlayback {
    active: Option<(WeatherKind, u64)>,
}

impl WeatherPlayback {
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
