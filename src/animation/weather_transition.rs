//! Shared, elapsed-time presentation transitions independent of server weather state.

use std::time::{Duration, Instant};

use crate::config::{AnimationConfig, WeatherConfig};

use super::weather::WeatherKind;

const TRANSITION_DURATION: Duration = Duration::from_millis(1500);
const SKY_TRANSITION_DURATION: Duration = Duration::from_millis(500);
const LAYER_COUNT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum WeatherLayer {
    Sky,
    Clouds,
    Rain,
    Snow,
    Blizzard,
    Fog,
    Wind,
    Ash,
    Dust,
}

impl WeatherLayer {
    pub const ALL: [Self; LAYER_COUNT] = [
        Self::Sky,
        Self::Clouds,
        Self::Rain,
        Self::Snow,
        Self::Blizzard,
        Self::Fog,
        Self::Wind,
        Self::Ash,
        Self::Dust,
    ];
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WeatherBlend {
    pub opacity: [u8; LAYER_COUNT],
}

impl WeatherBlend {
    /// Lightning is intentionally absent: authoritative storm state controls it immediately.
    pub fn settled(kind: WeatherKind) -> Self {
        use WeatherLayer as Layer;
        let layers: &[WeatherLayer] = match kind {
            WeatherKind::Clear => &[Layer::Sky],
            WeatherKind::Cloudy => &[Layer::Sky, Layer::Clouds],
            WeatherKind::Rain | WeatherKind::Storm => &[Layer::Clouds, Layer::Rain],
            WeatherKind::Snow => &[Layer::Clouds, Layer::Snow],
            WeatherKind::Blizzard => &[Layer::Clouds, Layer::Blizzard],
            WeatherKind::Fog => &[Layer::Fog],
            WeatherKind::Wind => &[Layer::Wind],
            WeatherKind::Ash => &[Layer::Ash],
            WeatherKind::Dust => &[Layer::Dust],
            WeatherKind::Indoor | WeatherKind::Unknown => &[],
        };
        let mut blend = Self::default();
        for &layer in layers {
            blend.opacity[layer as usize] = u8::MAX;
        }
        blend
    }

    pub fn opacity(self, layer: WeatherLayer) -> u8 {
        self.opacity[layer as usize]
    }
}

/// Fixed-size layer weights allow interruption without stacking outgoing scenes.
#[derive(Debug, Default)]
pub struct WeatherTransition {
    from: WeatherBlend,
    target: Option<WeatherBlend>,
    started_at: Option<Instant>,
}

impl WeatherTransition {
    pub fn update(
        &mut self,
        kind: WeatherKind,
        weather: &WeatherConfig,
        animation: &AnimationConfig,
        now: Instant,
    ) -> WeatherBlend {
        let visible = weather.enabled && weather.show_info_marker;
        let desired = if visible {
            WeatherBlend::settled(kind)
        } else {
            WeatherBlend::default()
        };
        if self.target.is_none()
            || !visible
            || matches!(kind, WeatherKind::Indoor | WeatherKind::Unknown)
            || !animation.enabled
            || animation.reduced_motion
        {
            self.from = desired;
            self.target = Some(desired);
            self.started_at = None;
            return desired;
        }

        let current = self.current(now);
        if self.target != Some(desired) {
            self.from = current;
            self.target = Some(desired);
            self.started_at = (current != desired).then_some(now);
        }
        current
    }

    fn current(&mut self, now: Instant) -> WeatherBlend {
        let target = self.target.unwrap_or_default();
        let Some(started_at) = self.started_at else {
            return target;
        };
        let elapsed = now.saturating_duration_since(started_at);
        if elapsed >= TRANSITION_DURATION {
            self.started_at = None;
            return target;
        }
        let mut blend = WeatherBlend::default();
        for layer in WeatherLayer::ALL {
            let duration = if layer == WeatherLayer::Sky {
                SKY_TRANSITION_DURATION
            } else {
                TRANSITION_DURATION
            }
            .as_nanos();
            let elapsed = elapsed.as_nanos().min(duration);
            let index = layer as usize;
            let start = u128::from(self.from.opacity[index]);
            let end = u128::from(target.opacity[index]);
            // Unsigned weighted interpolation rounds consistently in either direction.
            blend.opacity[index] =
                ((start * (duration - elapsed) + end * elapsed) / duration) as u8;
        }
        blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [WeatherKind; 12] = [
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
        WeatherKind::Indoor,
        WeatherKind::Unknown,
    ];

    fn update(playback: &mut WeatherTransition, kind: WeatherKind, now: Instant) -> WeatherBlend {
        playback.update(
            kind,
            &WeatherConfig::default(),
            &AnimationConfig::default(),
            now,
        )
    }

    #[test]
    fn every_weather_pair_reaches_target_with_shared_layers_unchanged() {
        let start = Instant::now();
        for from in KINDS {
            for to in KINDS {
                let mut playback = WeatherTransition::default();
                let original = WeatherBlend::settled(from);
                let target = WeatherBlend::settled(to);
                assert_eq!(update(&mut playback, from, start), original);
                let first = update(&mut playback, to, start);
                if matches!(to, WeatherKind::Indoor | WeatherKind::Unknown) {
                    assert_eq!(first, WeatherBlend::default());
                    continue;
                }
                assert_eq!(first, original);
                let middle = update(&mut playback, to, start + TRANSITION_DURATION / 2);
                for layer in WeatherLayer::ALL {
                    assert_eq!(
                        middle.opacity(layer),
                        if layer == WeatherLayer::Sky {
                            target.opacity(layer)
                        } else {
                            ((u16::from(original.opacity(layer))
                                + u16::from(target.opacity(layer)))
                                / 2) as u8
                        },
                        "{from:?} -> {to:?}: {layer:?}"
                    );
                }
                assert_eq!(
                    update(&mut playback, to, start + TRANSITION_DURATION),
                    target
                );
                assert_eq!(
                    update(&mut playback, to, start + Duration::from_secs(500)),
                    target
                );
            }
        }
    }

    #[test]
    fn sky_fades_out_of_clear_and_cloudy_into_other_outdoor_weather() {
        let start = Instant::now();
        for from in [WeatherKind::Clear, WeatherKind::Cloudy] {
            for to in KINDS.into_iter().skip(2).take(8) {
                let mut playback = WeatherTransition::default();
                update(&mut playback, from, start);
                assert_eq!(
                    update(&mut playback, to, start).opacity(WeatherLayer::Sky),
                    255
                );
                assert_eq!(
                    update(&mut playback, to, start + SKY_TRANSITION_DURATION / 2)
                        .opacity(WeatherLayer::Sky),
                    127
                );
                assert_eq!(
                    update(&mut playback, to, start + SKY_TRANSITION_DURATION)
                        .opacity(WeatherLayer::Sky),
                    0
                );
                assert_eq!(WeatherBlend::settled(to).opacity(WeatherLayer::Sky), 0);
                let back = start + TRANSITION_DURATION * 2;
                assert_eq!(
                    update(&mut playback, from, back).opacity(WeatherLayer::Sky),
                    0
                );
                assert_eq!(
                    update(&mut playback, from, back + SKY_TRANSITION_DURATION)
                        .opacity(WeatherLayer::Sky),
                    255
                );
            }
        }
    }

    #[test]
    fn sky_finishes_before_particles_and_reverses_without_jumping() {
        let start = Instant::now();
        let mut playback = WeatherTransition::default();
        update(&mut playback, WeatherKind::Clear, start);
        update(&mut playback, WeatherKind::Rain, start);
        let early = update(
            &mut playback,
            WeatherKind::Rain,
            start + Duration::from_millis(250),
        );
        assert_eq!(early.opacity(WeatherLayer::Sky), 127);
        assert_eq!(early.opacity(WeatherLayer::Rain), 42);
        let half_second = update(
            &mut playback,
            WeatherKind::Rain,
            start + SKY_TRANSITION_DURATION,
        );
        assert_eq!(half_second.opacity(WeatherLayer::Sky), 0);
        assert_eq!(half_second.opacity(WeatherLayer::Rain), 85);
        assert_eq!(
            update(
                &mut playback,
                WeatherKind::Clear,
                start + SKY_TRANSITION_DURATION
            ),
            half_second
        );
        let returning = update(
            &mut playback,
            WeatherKind::Clear,
            start + SKY_TRANSITION_DURATION + Duration::from_millis(250),
        );
        assert_eq!(returning.opacity(WeatherLayer::Sky), 127);
        assert!((0..85).contains(&returning.opacity(WeatherLayer::Rain)));
    }

    #[test]
    fn interrupted_transition_restarts_from_current_weights_without_jump() {
        let start = Instant::now();
        let mut playback = WeatherTransition::default();
        update(&mut playback, WeatherKind::Clear, start);
        update(&mut playback, WeatherKind::Rain, start);
        let middle = update(
            &mut playback,
            WeatherKind::Rain,
            start + TRANSITION_DURATION / 2,
        );
        assert_eq!(middle.opacity(WeatherLayer::Rain), 127);
        assert_eq!(
            update(
                &mut playback,
                WeatherKind::Clear,
                start + TRANSITION_DURATION / 2
            ),
            middle
        );
        let reversed = update(
            &mut playback,
            WeatherKind::Clear,
            start + TRANSITION_DURATION,
        );
        assert_eq!(reversed.opacity(WeatherLayer::Rain), 63);
        assert_eq!(reversed.opacity(WeatherLayer::Sky), 255);
        assert_eq!(
            update(
                &mut playback,
                WeatherKind::Clear,
                start + TRANSITION_DURATION * 2
            ),
            WeatherBlend::settled(WeatherKind::Clear)
        );
    }

    #[test]
    fn weather_with_same_layers_does_not_restart_deadline() {
        let start = Instant::now();
        let mut playback = WeatherTransition::default();
        update(&mut playback, WeatherKind::Clear, start);
        update(&mut playback, WeatherKind::Rain, start);
        update(
            &mut playback,
            WeatherKind::Storm,
            start + Duration::from_millis(1000),
        );
        assert_eq!(
            update(
                &mut playback,
                WeatherKind::Storm,
                start + TRANSITION_DURATION
            ),
            WeatherBlend::settled(WeatherKind::Storm)
        );
    }

    #[test]
    fn accessibility_and_visibility_changes_snap_immediately() {
        let start = Instant::now();
        for option in 0..4 {
            let mut playback = WeatherTransition::default();
            update(&mut playback, WeatherKind::Rain, start);
            update(&mut playback, WeatherKind::Snow, start);
            let mut weather = WeatherConfig::default();
            let mut animation = AnimationConfig::default();
            match option {
                0 => weather.enabled = false,
                1 => weather.show_info_marker = false,
                2 => animation.enabled = false,
                _ => animation.reduced_motion = true,
            }
            let expected = if option < 2 {
                WeatherBlend::default()
            } else {
                WeatherBlend::settled(WeatherKind::Snow)
            };
            assert_eq!(
                playback.update(
                    WeatherKind::Snow,
                    &weather,
                    &animation,
                    start + Duration::from_millis(100)
                ),
                expected
            );
            assert!(playback.started_at.is_none());
        }
    }

    #[test]
    fn low_frame_rate_does_not_extend_transition_and_earlier_time_is_safe() {
        let start = Instant::now();
        let mut playback = WeatherTransition::default();
        update(&mut playback, WeatherKind::Clear, start);
        let changed = start + Duration::from_secs(1);
        update(&mut playback, WeatherKind::Snow, changed);
        assert_eq!(
            update(&mut playback, WeatherKind::Snow, start),
            WeatherBlend::settled(WeatherKind::Clear)
        );
        let animation = AnimationConfig {
            low_performance: true,
            weather_fps: 1,
            ..AnimationConfig::default()
        };
        assert_eq!(
            playback.update(
                WeatherKind::Snow,
                &WeatherConfig::default(),
                &animation,
                changed + Duration::from_secs(30)
            ),
            WeatherBlend::settled(WeatherKind::Snow)
        );
    }
}
