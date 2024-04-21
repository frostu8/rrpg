//! Global game state structures.

use bevy::prelude::*;

/// Plugin to add game state.
pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>();
    }
}

/// Global game state.
///
/// See [module level documentation][`super`] for more info.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, States)]
pub enum GameState {
    /// Default game state on startup.
    #[default]
    Splash,
    /// Asset loading.
    Loading,
    /// In battle.
    InBattle,
}
