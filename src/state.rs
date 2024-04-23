//! Global game state structures and loading systems.

use bevy::prelude::*;
use bevy_asset_loader::prelude::*;

/// Plugin to add game state.
///
/// This also adds systems responsible for loading. Make sure you install
/// this plugin if anything!
pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>().add_loading_state(
            LoadingState::new(GameState::LoadingBattle).continue_to_state(GameState::InBattle),
        );
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
    ///
    /// Will transition to [`GameState::InBattle`] after this is concluded.
    LoadingBattle,
    /// In battle.
    InBattle,
}
