//! Single note components.
//!
//! # Hierarchy
//! To form a beatmap, components are expected to be formed in a certain
//! hierarchy:
//! * Handle<Beatmap> -> Lane -> Note

use bevy::{prelude::*, utils::HashSet};

use super::{asset::BeatmapNote, ImageAssets, Rhythm, RhythmExt};

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
    sprite_count: usize,

    number: u32,
    notes: Vec<Entity>,
    current_note: usize,
}

impl Lane {
    /// Creates a new `Lane`.
    pub fn new(number: u32) -> Lane {
        Lane {
            sprite_count: 30,
            number,
            notes: Vec::with_capacity(512),
            current_note: 0,
        }
    }

    /// Returns the next note, but does not increment the current note.
    ///
    /// Returns `None` if at the end of the notes.
    pub fn next_note(&self) -> Option<Entity> {
        self.notes.get(self.current_note).copied()
    }

    /// Returns an iterator of all next notes.
    pub fn all_next_notes<'a>(&'a self) -> impl Iterator<Item = Entity> + 'a {
        self.notes.iter().skip(self.current_note).copied()
    }

    /// Advances to the next note, returning the last current note.
    ///
    /// Returns `None` if at the end of the notes.
    pub fn advance_note(&mut self) -> Option<Entity> {
        let note = self.next_note();
        self.current_note += 1;
        note
    }

    /// Skips a lot of notes.
    pub fn skip_notes(&mut self, to_skip: usize) {
        self.current_note += to_skip;
    }
}

/// The container for the lane visuals.
#[derive(Clone, Component, Debug, Default)]
pub struct LaneSprite {
    count: usize,
}

/// An instantiated note.
///
/// Contains a copy of the note that it was created from.
#[derive(Clone, Component, Debug)]
pub struct Note {
    inner: BeatmapNote,
    scroll_axis: Vec3,
}

impl Note {
    /// Returns the beat this note occurs on.
    pub fn beat(&self) -> f32 {
        self.inner.beat()
    }
}

impl Default for Note {
    fn default() -> Note {
        Note {
            inner: BeatmapNote::default(),
            scroll_axis: Vec3::Y * 48.,
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

/// Creates lane visuals.
pub fn create_lane_sprite(
    new_lanes: Query<(Entity, &Lane), Added<Lane>>,
    image_assets: Res<ImageAssets>,
    mut commands: Commands,
) {
    for (entity, lane) in new_lanes.iter() {
        let count = lane.sprite_count;

        // spawn lane sprite entity
        let lane_sprite = commands
            .spawn((SpatialBundle::default(), LaneSprite { count }))
            .set_parent(entity)
            .id();

        // spawn all lane sprites
        for i in 0..count {
            // TODO: Magic number!!!
            let y = i as f32 * 8.;

            commands
                .spawn((
                    SpriteBundle {
                        texture: image_assets.lane_sheet.clone(),
                        transform: Transform::from_xyz(0., y, -5.),
                        ..Default::default()
                    },
                    TextureAtlas {
                        layout: image_assets.lane_sheet_layout.clone(),
                        index: i % 4,
                    },
                ))
                .set_parent(lane_sprite);
        }
    }
}
