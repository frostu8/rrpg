//! Input mapping, events and timetamps.

use std::time::Duration;

use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    prelude::*,
};

use super::{note::Lane, Rhythm, RhythmExt, RhythmSystem};

/// Keyboard input plugin.
pub struct KeyboardInputPlugin;

impl Plugin for KeyboardInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            create_key_events_keyboard.in_set(RhythmSystem::Input),
        );
    }
}

/// Maps keyboard inputs to lanes.
#[derive(Clone, Component, Debug, Default)]
pub struct LaneInputKeyboard {
    key_code: Option<KeyCode>,
}

impl LaneInputKeyboard {
    /// Creates a new `LaneInputKeyboard`.
    pub fn new(key_code: KeyCode) -> LaneInputKeyboard {
        LaneInputKeyboard {
            key_code: Some(key_code),
        }
    }
}

/// For when a key is down on a lane.
///
/// # Timestamps
/// Timestamps returned by this event are based off the rhythm clock's
/// [`RhythmExt::position`] and are adjusted for delay.
#[derive(Clone, Debug, Event)]
pub struct KeyEvent {
    /// The timestamp of the event.
    pub timestamp: Duration,
    /// The lane this event is for.
    pub lane: Entity,
    /// The kind of input event.
    pub kind: KeyEventType,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyEventType {
    /// The key changed to down on this input.
    #[default]
    Down,
    /// The key changed to up on this input.
    Up,
}

/// Creates input events from mapped keyboard inputs.
pub fn create_key_events_keyboard(
    lanes: Query<(Entity, &LaneInputKeyboard), With<Lane>>,
    rhythm: Res<Time<Rhythm>>,
    mut key_event_tx: EventWriter<KeyEvent>,
    mut key_events: EventReader<KeyboardInput>,
) {
    for input in key_events.read() {
        // find input for `key_code`
        let lane = lanes
            .iter()
            .find(|(_, ik)| ik.key_code == Some(input.key_code));

        if let Some((lane, _)) = lane {
            let kind = match input.state {
                ButtonState::Pressed => Some(KeyEventType::Down),
                ButtonState::Released => Some(KeyEventType::Up),
            };

            if let Some(kind) = kind {
                // send input event
                key_event_tx.send(KeyEvent {
                    timestamp: rhythm.position(),
                    kind,
                    lane,
                });
            }
        }
    }
}
