//! Rhythm judgements.
//!
//! Contains abstracted input events and how to work with them.

use bevy::prelude::*;

use super::{
    input::{KeyEvent, KeyEventType},
    note::{Lane, Note, NoteType, Slider},
    BeatmapInstance, Rhythm, RhythmExt,
};

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

/// Triggers a judgement on a key press or key release.
pub fn create_judgements(
    beatmaps: Query<&BeatmapInstance>,
    mut lanes: Query<(&mut Lane, &Parent)>,
    notes: Query<(Entity, &Note)>,
    mut key_events: EventReader<KeyEvent>,
    mut judgement_event_tx: EventWriter<JudgementEvent>,
    rhythm: Res<Time<Rhythm>>,
) {
    for key in key_events.read() {
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
            // notes and sliderbegins only want up events
            if (matches!(next_note.kind(), NoteType::Note | NoteType::SliderBegin)
                && matches!(key.kind, KeyEventType::Down))
                || (matches!(next_note.kind(), NoteType::SliderEnd)
                    && matches!(key.kind, KeyEventType::Up))
            {
                judgement_event_tx.send(JudgementEvent {
                    note: note_entity,
                    offset: Some(diff),
                });

                // advance note if it was hit
                lane.advance_note();
            }
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

/// Sets the down flag on the slider on inputs.
pub fn set_slider_down(
    lanes: Query<&Lane>,
    mut sliders: Query<&mut Slider, With<Note>>,
    mut key_events: EventReader<KeyEvent>,
) {
    for key in key_events.read() {
        let Ok(lane) = lanes.get(key.lane) else {
            continue;
        };

        let Some(next_note) = lane.next_note() else {
            continue;
        };

        // if the next note is a slider...
        let Ok(mut slider) = sliders.get_mut(next_note) else {
            continue;
        };

        // ...key events will contribute whether the slider is down or not
        slider.set_down(matches!(key.kind, KeyEventType::Down));
    }
}
