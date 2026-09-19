use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use crate::config::AnimationConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationDefinition {
    pub name: String,
    pub frames: usize,
    pub frame_rate_fps: u64,
    pub duration: Duration,
    pub looping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationSnapshot {
    pub name: String,
    pub frame: usize,
    pub complete: bool,
    pub paused: bool,
}

#[derive(Debug)]
pub struct AnimationScheduler {
    enabled: bool,
    reduced_motion: bool,
    low_performance: bool,
    effects: BTreeMap<String, RunningAnimation>,
}

#[derive(Debug, Clone)]
struct RunningAnimation {
    definition: AnimationDefinition,
    started_at: Instant,
    paused_at: Option<Instant>,
    total_paused: Duration,
    complete: bool,
}

impl AnimationScheduler {
    pub fn new(config: &AnimationConfig) -> Self {
        Self {
            enabled: config.enabled,
            reduced_motion: config.reduced_motion,
            low_performance: config.low_performance,
            effects: BTreeMap::new(),
        }
    }

    pub fn configure(&mut self, config: &AnimationConfig) {
        self.enabled = config.enabled;
        self.reduced_motion = config.reduced_motion;
        self.low_performance = config.low_performance;
    }

    pub fn start(&mut self, definition: AnimationDefinition, now: Instant) {
        if !self.enabled || definition.frames == 0 {
            return;
        }
        self.effects.insert(
            definition.name.clone(),
            RunningAnimation {
                definition,
                started_at: now,
                paused_at: None,
                total_paused: Duration::ZERO,
                complete: false,
            },
        );
    }

    pub fn start_event_effect(&mut self, event_name: &str, now: Instant) {
        if !self.enabled || self.reduced_motion {
            return;
        }
        self.start(
            AnimationDefinition {
                name: format!("event:{event_name}"),
                frames: if self.low_performance { 2 } else { 4 },
                frame_rate_fps: if self.low_performance { 4 } else { 12 },
                duration: Duration::from_millis(450),
                looping: false,
            },
            now,
        );
    }

    pub fn pause(&mut self, name: &str, now: Instant) {
        if let Some(effect) = self.effects.get_mut(name)
            && effect.paused_at.is_none()
        {
            effect.paused_at = Some(now);
        }
    }

    pub fn resume(&mut self, name: &str, now: Instant) {
        if let Some(effect) = self.effects.get_mut(name)
            && let Some(paused_at) = effect.paused_at.take()
        {
            effect.total_paused += now.saturating_duration_since(paused_at);
        }
    }

    pub fn cancel(&mut self, name: &str) {
        self.effects.remove(name);
    }

    pub fn tick(&mut self, now: Instant) {
        for effect in self.effects.values_mut() {
            effect.update_complete(now);
        }
        self.effects
            .retain(|_, effect| effect.definition.looping || !effect.complete);
    }

    pub fn snapshots(&self, now: Instant) -> Vec<AnimationSnapshot> {
        self.effects
            .values()
            .map(|effect| effect.snapshot(now))
            .collect()
    }
}

impl RunningAnimation {
    fn snapshot(&self, now: Instant) -> AnimationSnapshot {
        let elapsed = self.elapsed(now);
        let frame_duration = frame_duration(self.definition.frame_rate_fps);
        let frame = if self.definition.frames <= 1 {
            0
        } else if self.complete && !self.definition.looping {
            self.definition.frames - 1
        } else {
            ((elapsed.as_millis() / frame_duration.as_millis().max(1)) as usize)
                % self.definition.frames
        };
        AnimationSnapshot {
            name: self.definition.name.clone(),
            frame,
            complete: self.complete,
            paused: self.paused_at.is_some(),
        }
    }

    fn update_complete(&mut self, now: Instant) {
        if !self.definition.looping && self.elapsed(now) >= self.definition.duration {
            self.complete = true;
        }
    }

    fn elapsed(&self, now: Instant) -> Duration {
        let effective_now = self.paused_at.unwrap_or(now);
        effective_now
            .saturating_duration_since(self.started_at)
            .saturating_sub(self.total_paused)
    }
}

fn frame_duration(frame_rate_fps: u64) -> Duration {
    if frame_rate_fps == 0 {
        return Duration::from_secs(1);
    }
    Duration::from_millis((1000 / frame_rate_fps).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(looping: bool) -> AnimationDefinition {
        AnimationDefinition {
            name: "pulse".to_string(),
            frames: 4,
            frame_rate_fps: 10,
            duration: Duration::from_millis(350),
            looping,
        }
    }

    #[test]
    fn selects_frames_from_elapsed_time() {
        let now = Instant::now();
        let mut scheduler = AnimationScheduler::new(&AnimationConfig::default());

        scheduler.start(definition(true), now);

        assert_eq!(scheduler.snapshots(now)[0].frame, 0);
        assert_eq!(
            scheduler.snapshots(now + Duration::from_millis(250))[0].frame,
            2
        );
    }

    #[test]
    fn removes_completed_one_shot_effects_on_tick() {
        let now = Instant::now();
        let mut scheduler = AnimationScheduler::new(&AnimationConfig::default());

        scheduler.start(definition(false), now);
        scheduler.tick(now + Duration::from_millis(400));

        assert!(
            scheduler
                .snapshots(now + Duration::from_millis(400))
                .is_empty()
        );
    }

    #[test]
    fn pause_and_resume_exclude_paused_time() {
        let now = Instant::now();
        let mut scheduler = AnimationScheduler::new(&AnimationConfig::default());

        scheduler.start(definition(true), now);
        scheduler.pause("pulse", now + Duration::from_millis(100));
        scheduler.resume("pulse", now + Duration::from_millis(300));

        assert_eq!(
            scheduler.snapshots(now + Duration::from_millis(350))[0].frame,
            1
        );
    }

    #[test]
    fn reduced_motion_skips_event_effects() {
        let mut scheduler = AnimationScheduler::new(&AnimationConfig {
            reduced_motion: true,
            ..AnimationConfig::default()
        });

        scheduler.start_event_effect("Enemy", Instant::now());

        assert!(scheduler.snapshots(Instant::now()).is_empty());
    }
}
