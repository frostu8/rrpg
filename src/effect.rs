//! 2D sprite effects.
//!
//! Contains components for basic animations.

use std::time::Duration;

use bevy::prelude::*;

/// Effect plugin.
pub struct EffectPlugin;

impl Plugin for EffectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, animate_sprite_sheets);
    }
}

/// Animation frames.
#[derive(Clone, Component, Debug)]
pub struct AnimationFrames {
    first: usize,
    last: usize,
}

impl AnimationFrames {
    /// Creates a new `AnimationFrames`.
    ///
    /// # Panics
    /// Panics when `first` > `last`.
    pub fn new(first: usize, last: usize) -> AnimationFrames {
        assert!(first <= last);

        AnimationFrames { first, last }
    }
}

/// The actual animation timer.
#[derive(Clone, Component, Debug)]
pub struct AnimationTimer {
    timer: Timer,
    despawn: bool,
}

impl AnimationTimer {
    /// Creates a new `AnimationTimer` that despawns the entity when finished.
    ///
    /// Useful for simple particles and effects.
    pub fn despawn_after(duration: Duration) -> AnimationTimer {
        AnimationTimer {
            timer: Timer::new(duration, TimerMode::Once),
            despawn: true,
        }
    }
}

/// Animates entities with [`AnimationFrames`] and [`AnimationTimer`].
pub fn animate_sprite_sheets(
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &AnimationFrames,
        &mut AnimationTimer,
        &mut TextureAtlas,
    )>,
    mut commands: Commands,
) {
    for (entity, frames, mut timer, mut atlas) in query.iter_mut() {
        let AnimationFrames { first, last } = *frames;

        // increment timer
        timer.timer.tick(time.delta());

        let frames = last - first;

        let new_frame = timer.timer.fraction() * frames as f32;

        // change atlas
        atlas.index = new_frame as usize + first;

        if timer.timer.just_finished() && timer.despawn {
            // despawn entity
            commands.entity(entity).despawn_recursive();
        }
    }
}
