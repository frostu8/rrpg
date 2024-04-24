//! Rhythm judgements.
//!
//! Contains abstracted input events and how to work with them.

use std::time::Duration;

use bevy::{
    input::{keyboard::KeyboardInput, ButtonState},
    prelude::*,
};

use super::{
    note::{Lane, Note},
    BeatmapInstance, Rhythm, RhythmExt,
};

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

/// An event that is created for judgements.
#[derive(Clone, Debug, Event)]
pub struct JudgementEvent {
    /// The note that this judgement is for.
    pub note: Entity,
    /// The offset in seconds. A positive offset means the note press was too
    /// early, and a negative means the press was too late.
    ///
    /// If the note was missed, this is `None`.
    pub offset: Option<f32>,
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

/// Triggers a judgement on a key press.
pub fn create_judgements(
    beatmaps: Query<&BeatmapInstance>,
    mut lanes: Query<(&mut Lane, &Parent)>,
    notes: Query<(Entity, &Note)>,
    mut key_events: EventReader<KeyEvent>,
    mut judgement_event_tx: EventWriter<JudgementEvent>,
    rhythm: Res<Time<Rhythm>>,
) {
    for key in key_events.read() {
        // filter downs for now
        if key.kind != KeyEventType::Down {
            continue;
        };

        // find lane associated
        let Ok((mut lane, parent)) = lanes.get_mut(key.lane) else {
            continue;
        };

        // get beatmap
        let Ok(beatmap) = beatmaps.get(parent.get()) else {
            continue;
        };

        // get the next note in the lane
        let Some((note_entity, next_note)) = lane.next_note().and_then(|n| notes.get(n).ok())
        else {
            continue;
        };

        // compare timing
        // NOTE: the order we read these inputs should be in time order!
        // this does not work well if they aren't.
        let note_position = rhythm.beat_position(next_note.beat());
        let input_position = key.timestamp;

        let diff = note_position.as_secs_f32() - input_position.as_secs_f32();

        let window_max = beatmap.note_window.as_secs_f32();

        if diff.abs() <= window_max.abs() {
            judgement_event_tx.send(JudgementEvent {
                note: note_entity,
                offset: Some(diff),
            });

            // advance note if it was hit
            lane.advance_note();
        }

        // do not count input otherwise;
        // dropped notes will get picked up by `create_dropped_judgements`
    }
}

/// Creates any "Missed" judgements for dropped notes.
///
/// This runs after the [`create_judgements`] system.
pub fn create_dropped_judgements(
    beatmaps: Query<&BeatmapInstance>,
    mut lanes: Query<(&mut Lane, &Parent)>,
    notes: Query<&Note>,
    mut judgement_event_tx: EventWriter<JudgementEvent>,
    rhythm: Res<Time<Rhythm>>,
) {
    for (mut lane, lane_parent) in lanes.iter_mut() {
        // get beatmap
        let Ok(beatmap) = beatmaps.get(lane_parent.get()) else {
            continue;
        };

        let mut last_note_idx = 0;

        for (i, note_entity, _) in lane
            .all_next_notes()
            .enumerate()
            .filter_map(|(i, ne)| notes.get(ne).map(|n| (i, ne, n)).ok())
            .take_while(|(_, _, n)| {
                let note_position = rhythm.beat_position(n.beat());
                let current_position = rhythm.position();

                if let Some(offset) = current_position.checked_sub(note_position) {
                    // offset cannot be greater than window
                    offset > beatmap.note_window
                } else {
                    false
                }
            })
        {
            // send missed to all of these
            judgement_event_tx.send(JudgementEvent {
                note: note_entity,
                offset: None,
            });

            // update last note
            last_note_idx = i + 1;
        }

        // skip all missed notes
        lane.skip_notes(last_note_idx);
    }
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
                _ => None,
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
