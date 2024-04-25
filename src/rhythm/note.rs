//! Single note components.
//!
//! # Hierarchy
//! To form a beatmap, components are expected to be formed in a certain
//! hierarchy:
//! * Handle<Beatmap> -> Lane -> Note

use std::time::Duration;

use bevy::{prelude::*, utils::HashSet};

use super::{ImageAssets, Rhythm, RhythmExt, NOTE_HEIGHT};

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

    /// The lane number.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Returns the index of the next note.
    ///
    /// This might be out of bounds!
    pub fn next_note_index(&self) -> usize {
        self.current_note
    }

    /// Returns the next note, but does not increment the current note.
    ///
    /// Returns `None` if at the end of the notes.
    pub fn next_note(&self) -> Option<Entity> {
        self.notes.get(self.current_note).copied()
    }

    /// Returns an iterator of all next notes.
    pub fn all_next_notes(&self) -> impl Iterator<Item = Entity> + '_ {
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
#[allow(dead_code)]
pub struct LaneSprite {
    count: usize,
}

/// An instantiated note.
///
/// Contains a copy of the note that it was created from.
#[derive(Clone, Component, Debug)]
pub struct Note {
    beat: f32,
    kind: NoteType,

    index: usize,
    scroll_axis: Vec3,
}

impl Note {
    /// Creates a new `Note` component.
    pub fn new(beat: f32, kind: NoteType, index: usize) -> Note {
        Note {
            beat,
            kind,
            index,
            ..Default::default()
        }
    }

    /// The kind of the note.
    ///
    /// * [`NoteType::Note`]
    ///   An input in the lane must be created in the window of the note for a
    ///   judgement to pass.
    /// * [`NoteType::SliderBegin`]  
    ///   An input in the lane must be created in the window of the note for a
    ///   judgement to pass. A component that tracks how long the input is down
    ///   for in the lane is attached to the end of the slider.
    /// * [`NoteType::SliderEnd`]  
    ///   The tracking in the next note is compared, along with a proper
    ///   release time on the slider.
    pub fn kind(&self) -> NoteType {
        self.kind
    }

    /// Returns the beat this note occurs on.
    pub fn beat(&self) -> f32 {
        self.beat
    }

    /// Returns the index of the note in the parent [`Lane`] component.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl Default for Note {
    fn default() -> Note {
        Note {
            beat: 0.0,
            kind: NoteType::Note,
            index: 0,
            scroll_axis: Vec3::Y * 48.,
        }
    }
}

/// Note type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NoteType {
    /// A single note.
    #[default]
    Note,
    /// A beginning to a slider.
    SliderBegin,
    /// An end to a slider.
    SliderEnd,
}

/// A ref to the slider component for the full slider object.
///
/// This is placed on the beginning note of a slider.
#[derive(Clone, Component, Debug)]
pub struct SliderRef(pub Entity);

impl SliderRef {
    /// Gets the note entity that is the end of the slider.
    pub fn get(&self) -> Entity {
        self.0
    }
}

/// The slider component for a note.
///
/// This component is attached to the "end note" of the slider.
#[derive(Clone, Component, Debug, Default)]
pub struct Slider {
    duration_held: Duration,
    down: bool,
}

impl Slider {
    /// Whether the input on the slider is down or not.
    pub fn down(&self) -> bool {
        self.down
    }

    /// Sets whether an input is down on the slider.
    ///
    /// While the slider is down, it will automatically start counting rhythm
    /// deltas.
    pub fn set_down(&mut self, down: bool) {
        self.down = down;
    }

    /// The total duration the slider was held.
    pub fn duration_held(&self) -> Duration {
        self.duration_held
    }
}

/// Ticks slider down durations.
pub fn tick_sliders(mut sliders: Query<&mut Slider>, rhythm: Res<Time<Rhythm>>) {
    for mut slider in sliders.iter_mut() {
        if slider.down() {
            slider.duration_held += rhythm.delta();
        }
    }
}

/// Reorders the notes in a [`Lane`].
pub fn reorder_notes(
    mut lanes: Query<(Entity, &mut Lane)>,
    mut set: ParamSet<(
        Query<(Entity, &Parent, DebugName), Added<Note>>,
        Query<&mut Note>,
    )>,
) {
    let mut to_sort = HashSet::<Entity>::new();

    let new_notes = set.p0();

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

    let mut notes = set.p1();

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

            a.beat().partial_cmp(&b.beat()).expect("no NaN for beat")
        });

        // update indices for notes
        for (i, note_entity) in lane.notes.iter().copied().enumerate() {
            let mut note = notes
                .get_mut(note_entity)
                .expect("note found in prev algorithm");

            note.index = i;
        }
    }
}

/// Updates the positions of notes in a lane.
pub fn update_note_transform(mut notes: Query<(&Note, &mut Transform)>, rhythm: Res<Time<Rhythm>>) {
    for (note, mut transform) in notes.iter_mut() {
        // get distance to
        let dist = note.beat() - rhythm.beat_number();

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
            let y = i as f32 * NOTE_HEIGHT;

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
