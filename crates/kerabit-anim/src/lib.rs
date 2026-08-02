//! Clip animation playback on Kerabit transform hierarchies.
//!
//! M2: sample translation / rotation / scale channels by entity name and apply
//! them to a [`kerabit_world::World`]. glTF animation import is a minimal stretch
//! (optional helper that builds clips from authored keyframes).

use kerabit_math::{Quat, Vec3};
use kerabit_world::World;

/// Interpolation between keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// Linear blend (slerp for rotation).
    #[default]
    Linear,
    /// Hold previous key until the next.
    Step,
}

/// One timed sample of a vec3 property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3Key {
    pub time: f32,
    pub value: Vec3,
}

/// One timed sample of a quaternion property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuatKey {
    pub time: f32,
    pub value: Quat,
}

/// Animation channel targeting a named entity's local transform.
#[derive(Clone, Debug)]
pub struct AnimChannel {
    /// Entity name in the world (must match a spawned named entity).
    pub target: String,
    pub translation: Vec<Vec3Key>,
    pub rotation: Vec<QuatKey>,
    pub scale: Vec<Vec3Key>,
    pub interpolation: Interpolation,
}

impl AnimChannel {
    /// Channel targeting `target` with no keys yet.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            translation: Vec::new(),
            rotation: Vec::new(),
            scale: Vec::new(),
            interpolation: Interpolation::Linear,
        }
    }

    pub fn with_translation(mut self, keys: Vec<Vec3Key>) -> Self {
        self.translation = keys;
        self
    }

    pub fn with_rotation(mut self, keys: Vec<QuatKey>) -> Self {
        self.rotation = keys;
        self
    }

    pub fn with_scale(mut self, keys: Vec<Vec3Key>) -> Self {
        self.scale = keys;
        self
    }
}

/// A named animation clip (collection of channels).
#[derive(Clone, Debug, Default)]
pub struct AnimationClip {
    pub name: String,
    pub channels: Vec<AnimChannel>,
    /// Clip length in seconds (max key time). Computed if 0 when first sampled.
    duration: f32,
}

impl AnimationClip {
    /// Empty named clip.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channels: Vec::new(),
            duration: 0.0,
        }
    }

    /// Add a channel.
    pub fn add_channel(&mut self, channel: AnimChannel) {
        self.channels.push(channel);
        self.duration = 0.0;
    }

    /// Builder: push a channel.
    pub fn with_channel(mut self, channel: AnimChannel) -> Self {
        self.add_channel(channel);
        self
    }

    /// Duration in seconds (lazy-computed from key times).
    pub fn duration(&self) -> f32 {
        if self.duration > 0.0 {
            return self.duration;
        }
        let mut max_t = 0.0f32;
        for ch in &self.channels {
            for k in &ch.translation {
                max_t = max_t.max(k.time);
            }
            for k in &ch.rotation {
                max_t = max_t.max(k.time);
            }
            for k in &ch.scale {
                max_t = max_t.max(k.time);
            }
        }
        max_t
    }

    fn ensure_duration(&mut self) {
        if self.duration <= 0.0 {
            let mut max_t = 0.0f32;
            for ch in &self.channels {
                for k in &ch.translation {
                    max_t = max_t.max(k.time);
                }
                for k in &ch.rotation {
                    max_t = max_t.max(k.time);
                }
                for k in &ch.scale {
                    max_t = max_t.max(k.time);
                }
            }
            self.duration = if max_t > 0.0 { max_t } else { 1.0 };
        }
    }

    /// Sample this clip at `time` and write local TRS onto matching world entities.
    pub fn sample_into(&self, world: &mut World, time: f32) {
        for ch in &self.channels {
            let Some(entity) = world.get_mut(&ch.target) else {
                continue;
            };
            if let Some(t) = sample_vec3(&ch.translation, time, ch.interpolation) {
                entity.transform.set_translation(t);
            }
            if let Some(r) = sample_quat(&ch.rotation, time, ch.interpolation) {
                entity.transform.set_rotation(r);
            }
            if let Some(s) = sample_vec3(&ch.scale, time, ch.interpolation) {
                entity.transform.set_scale(s);
            }
        }
    }
}

/// Plays an [`AnimationClip`], advancing time and writing into a [`World`].
#[derive(Clone, Debug)]
pub struct AnimationPlayer {
    clip: AnimationClip,
    time: f32,
    /// When true, wraps time by clip duration.
    pub looping: bool,
    /// Playback rate multiplier (1.0 = realtime).
    pub speed: f32,
    playing: bool,
}

impl AnimationPlayer {
    /// Create a player for `clip`, starting at t=0 (paused until [`Self::play`]).
    pub fn new(mut clip: AnimationClip) -> Self {
        clip.ensure_duration();
        Self {
            clip,
            time: 0.0,
            looping: true,
            speed: 1.0,
            playing: false,
        }
    }

    /// Start or resume playback.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause playback (keeps current time).
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stop and rewind to t=0.
    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
    }

    #[inline]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    #[inline]
    pub fn time(&self) -> f32 {
        self.time
    }

    #[inline]
    pub fn clip(&self) -> &AnimationClip {
        &self.clip
    }

    /// Seek to `time` (wrapped if looping).
    pub fn seek(&mut self, time: f32) {
        let dur = self.clip.duration();
        self.time = if self.looping && dur > 0.0 {
            time.rem_euclid(dur)
        } else {
            time.clamp(0.0, dur)
        };
    }

    /// Advance by `dt * speed` and sample into `world`.
    pub fn update(&mut self, world: &mut World, dt: f32) {
        if self.playing {
            self.time += dt.max(0.0) * self.speed;
            let dur = self.clip.duration();
            if self.looping && dur > 0.0 {
                self.time = self.time.rem_euclid(dur);
            } else if self.time >= dur {
                self.time = dur;
                self.playing = false;
            }
        }
        self.clip.sample_into(world, self.time);
    }
}

/// Minimal helper: build a two-key translation ping-pong clip for demos / tests.
///
/// Not a full glTF importer — authors pass explicit endpoints. Stretch goal for
/// real glTF anim import can replace this later without changing the player API.
pub fn translation_clip(
    name: impl Into<String>,
    target: impl Into<String>,
    from: Vec3,
    to: Vec3,
    duration: f32,
) -> AnimationClip {
    let duration = duration.max(1e-3);
    AnimationClip::new(name).with_channel(
        AnimChannel::new(target).with_translation(vec![
            Vec3Key {
                time: 0.0,
                value: from,
            },
            Vec3Key {
                time: duration,
                value: to,
            },
        ]),
    )
}

fn sample_vec3(keys: &[Vec3Key], time: f32, mode: Interpolation) -> Option<Vec3> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return Some(keys[0].value);
    }
    if time >= keys[keys.len() - 1].time {
        return Some(keys[keys.len() - 1].value);
    }
    for w in keys.windows(2) {
        let a = &w[0];
        let b = &w[1];
        if time >= a.time && time <= b.time {
            return Some(match mode {
                Interpolation::Step => a.value,
                Interpolation::Linear => {
                    let span = (b.time - a.time).max(1e-8);
                    let t = (time - a.time) / span;
                    a.value.lerp(b.value, t)
                }
            });
        }
    }
    Some(keys[keys.len() - 1].value)
}

fn sample_quat(keys: &[QuatKey], time: f32, mode: Interpolation) -> Option<Quat> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return Some(keys[0].value);
    }
    if time >= keys[keys.len() - 1].time {
        return Some(keys[keys.len() - 1].value);
    }
    for w in keys.windows(2) {
        let a = &w[0];
        let b = &w[1];
        if time >= a.time && time <= b.time {
            return Some(match mode {
                Interpolation::Step => a.value,
                Interpolation::Linear => {
                    let span = (b.time - a.time).max(1e-8);
                    let t = (time - a.time) / span;
                    a.value.slerp(b.value, t)
                }
            });
        }
    }
    Some(keys[keys.len() - 1].value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;
    use kerabit_world::Transform;
    use std::f32::consts::FRAC_PI_2;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn samples_translation_lerp() {
        let clip = translation_clip("bob", "cube", vec3(0.0, 0.0, 0.0), vec3(2.0, 0.0, 0.0), 2.0);
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        clip.sample_into(&mut world, 1.0);
        let t = world.get("cube").unwrap().transform.translation();
        assert!(approx(t.x, 1.0));
    }

    #[test]
    fn player_loops_and_applies() {
        let clip = translation_clip("bob", "cube", vec3(0.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), 1.0);
        let mut player = AnimationPlayer::new(clip);
        player.looping = true;
        player.play();
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        player.update(&mut world, 0.5);
        assert!(approx(
            world.get("cube").unwrap().transform.translation().y,
            1.0
        ));
        player.update(&mut world, 0.75);
        // Wrapped past end toward start of next loop.
        assert!(player.time() < 1.0);
    }

    #[test]
    fn rotation_channel_slerps() {
        let clip = AnimationClip::new("spin").with_channel(AnimChannel::new("cube").with_rotation(
            vec![
                QuatKey {
                    time: 0.0,
                    value: Quat::IDENTITY,
                },
                QuatKey {
                    time: 1.0,
                    value: Quat::from_rotation_y(FRAC_PI_2),
                },
            ],
        ));
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        clip.sample_into(&mut world, 0.5);
        let q = world.get("cube").unwrap().transform.rotation();
        let expected = Quat::IDENTITY.slerp(Quat::from_rotation_y(FRAC_PI_2), 0.5);
        assert!((q.dot(expected)).abs() > 0.99);
    }
}
