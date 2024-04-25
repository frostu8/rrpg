//! Buddy Holly (weezer).

#![feature(div_duration)]
#![allow(clippy::type_complexity)]

pub mod audio;
pub mod effect;
pub mod rhythm;
pub mod state;

use bevy::app::{PluginGroup, PluginGroupBuilder};
pub use state::GameState;

/// All game plugins.
pub struct RrpgPlugins;

impl PluginGroup for RrpgPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(state::GameStatePlugin)
            .add(audio::AudioPlugin)
            .add(rhythm::RhythmPlugin)
            .add(effect::EffectPlugin)
    }
}
