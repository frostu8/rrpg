//! Single note components.
//!
//! # Hierarchy
//! To form a beatmap, components are expected to be formed in a certain
//! hierarchy:
//! * Handle<Beatmap> -> Lane -> Note

use bevy::{prelude::*, utils::HashSet};

use super::{asset::BeatmapNote, Rhythm, RhythmExt};

/// A lane bundle.
#[derive(Bundle, Default)]
pub struct LaneBundle {
    pub global_transform: GlobalTransform,
    pub transform: Transform,
    pub visibility: Visibility,
    pub view_visibility: ViewVisibility,
    pub inherited_visibility: InheritedVisibility,
    pub lane: Lane,
}

/// A single lane.
///
/// Manages what note needs to be hit next. Notes can be added as children to
/// this entity, and the `Lane` will automatically manage them.
#[derive(Clone, Component, Debug, Default)]
pub struct Lane {
    number: u32,
    notes: Vec<Entity>,
    current_note: usize,
}

impl Lane {
    /// Creates a new `Lane`.
    pub fn new(number: u32) -> Lane {
        Lane {
            number,
            notes: Vec::with_capacity(512),
            current_note: 0,
        }
    }
}

/// An instantiated note.
///
/// Contains a copy of the note that it was created from.
#[derive(Clone, Component, Debug)]
pub struct Note {
    inner: BeatmapNote,
    scroll_axis: Vec3,
}

impl Default for Note {
    fn default() -> Note {
        Note {
            inner: BeatmapNote::default(),
            scroll_axis: Vec3::Y * 32.,
        }
    }
}

impl From<BeatmapNote> for Note {
    fn from(inner: BeatmapNote) -> Note {
        Note {
            inner,
            ..Default::default()
        }
    }
}

/// Reorders the notes in a [`Lane`].
pub fn reorder_notes(
    mut lanes: Query<(Entity, &mut Lane)>,
    new_notes: Query<(Entity, &Parent, DebugName), Added<Note>>,
    notes: Query<&Note>,
) {
    let mut to_sort = HashSet::<Entity>::new();

    for (entity, parent, name) in new_notes.iter() {
        // try and get lane
        let Ok((lane_entity, mut lane)) = lanes.get_mut(parent.get()) else {
            warn!("dangling `Note`: {:?}", name);
            continue;
        };

        // add note to list
        lane.notes.push(entity);

        // add to sort
        to_sort.insert(lane_entity);
    }

    for entity in to_sort {
        let (_, mut lane) = lanes
            .get_mut(entity)
            .expect("valid lane found in previous algorithm");

        // first, remove any notes that do not have a Note component.
        lane.notes.retain(|e| notes.contains(*e));

        // sort by remaining
        lane.notes.sort_unstable_by(|a, b| {
            let a = notes.get(*a).expect("note found in prev algorithm");
            let b = notes.get(*b).expect("note found in prev algorithm");

            a.inner.partial_cmp(&b.inner).expect("no NaN for beat")
        })
    }
}

/// Updates the positions of notes in a lane.
pub fn update_note_transform(mut notes: Query<(&Note, &mut Transform)>, rhythm: Res<Time<Rhythm>>) {
    for (note, mut transform) in notes.iter_mut() {
        // get distance to
        let dist = note.inner.beat() - rhythm.beat_number();

        transform.translation = note.scroll_axis * dist;
    }
}
