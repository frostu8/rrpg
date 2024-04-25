//! Higher level rhythm tracking.

pub mod asset;
pub mod judgement;
pub mod note;

use bevy::prelude::*;
use bevy::transform::TransformSystem;

use bevy_asset_loader::prelude::*;

use std::time::Duration;

use crate::{
    audio::{AudioControl, AudioSource},
    effect::{AnimationFrames, AnimationTimer},
    rhythm::judgement::LaneInputKeyboard,
    GameState,
};

pub use self::judgement::{JudgementEvent, KeyEvent};

use asset::{Beatmap, BeatmapLoader};

use note::{Lane, LaneBundle, Note};

/// The width of a single note.
pub const NOTE_WIDTH: f32 = 16.;

/// Rhythm plugin.
pub struct RhythmPlugin;

impl Plugin for RhythmPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<KeyEvent>()
            .add_event::<JudgementEvent>()
            .init_asset::<Beatmap>()
            .register_asset_loader(BeatmapLoader)
            .insert_resource(Time::new_with(Rhythm::default()))
            .configure_loading_state(
                LoadingStateConfig::new(GameState::LoadingBattle).load_collection::<ImageAssets>(),
            )
            .add_systems(
                PreUpdate,
                (
                    spawn_beatmap.run_if(in_state(GameState::InBattle)),
                    interpolate_rhythm_clock,
                ),
            )
            .add_systems(
                PreUpdate,
                (judgement::create_key_events_keyboard,).in_set(RhythmSystem::Input),
            )
            .add_systems(
                Update,
                (
                    judgement::create_judgements,
                    judgement::create_dropped_judgements,
                )
                    .chain()
                    .in_set(RhythmSystem::Judgement)
                    .after(RhythmSystem::Input),
            )
            .add_systems(
                Update,
                spawn_hit_effects
                    .in_set(RhythmSystem::Visual)
                    .after(RhythmSystem::Judgement)
                    .run_if(in_state(GameState::InBattle)),
            )
            .add_systems(
                PostUpdate,
                (note::reorder_notes, note::update_note_transform)
                    .chain()
                    .in_set(RhythmSystem::NoteUpdate)
                    .before(TransformSystem::TransformPropagate),
            )
            .add_systems(
                Update,
                note::create_lane_sprite
                    .run_if(in_state(GameState::InBattle))
                    .in_set(RhythmSystem::SpawnSprites),
            );
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum RhythmSystem {
    /// Spawns and does note visual effects.
    Visual,
    /// Does actual judgements.
    Judgement,
    /// Create input events.
    Input,
    /// Spawns related sprites.
    SpawnSprites,
    /// Updates note positions, placements and loading.
    NoteUpdate,
}

/// Sprite assets for the rhythm-game UI.
#[derive(AssetCollection, Resource)]
pub struct ImageAssets {
    #[asset(path = "sprites/note_default.png")]
    pub note_default: Handle<Image>,
    #[asset(path = "sprites/judgement_area.png")]
    pub judgement_area: Handle<Image>,
    #[asset(path = "sprites/judgement_hit_sheet.png")]
    pub judgement_hit: Handle<Image>,
    #[asset(texture_atlas_layout(tile_size_x = 24., tile_size_y = 16., columns = 5, rows = 1))]
    pub judgement_hit_layout: Handle<TextureAtlasLayout>,
    #[asset(path = "sprites/lane_sheet.png")]
    pub lane_sheet: Handle<Image>,
    #[asset(texture_atlas_layout(tile_size_x = 16., tile_size_y = 8., columns = 2, rows = 2))]
    pub lane_sheet_layout: Handle<TextureAtlasLayout>,
}

/// Loads a beatmap in.
///
/// This bundle includes [`MainTrack`] as a component. Remember to clean this
/// up after the song is concluded!
#[derive(Bundle, Default)]
pub struct BeatmapBundle {
    pub global_transform: GlobalTransform,
    pub transform: Transform,
    pub visibility: Visibility,
    pub view_visibility: ViewVisibility,
    pub inherited_visibility: InheritedVisibility,
    pub beatmap: Handle<Beatmap>,
    pub audio_source: Handle<AudioSource>,
    pub audio_control: AudioControl,
    pub main_track: MainTrack,
}

impl BeatmapBundle {
    /// Creates a new `BeatmapBundle`.
    pub fn new(beatmap: Handle<Beatmap>) -> BeatmapBundle {
        BeatmapBundle {
            beatmap,
            ..Default::default()
        }
    }
}

/// An instanced beatmap.
///
/// This component is inserted when all the notes are finished spawning. This
/// component also contains some useful information about the beatmap.
#[derive(Clone, Copy, Component, Debug)]
pub struct BeatmapInstance {
    /// The judgement window.
    pub note_window: Duration,
}

impl Default for BeatmapInstance {
    fn default() -> Self {
        BeatmapInstance {
            note_window: Duration::from_millis(100),
        }
    }
}

/// The main track.
///
/// This is an [`AudioBundle`](crate::audio::AudioBundle) that the [`Rhythm`]
/// clock will base its timings off of.
#[derive(Clone, Copy, Component, Default, Debug)]
pub struct MainTrack;

/// The rhythm clock, a more high level abstraction over rhythm timings.
///
/// Can be accessed through the [`Time`] resource. For accessor and mutator
/// methods, see [`RhythmExt`].
///
/// The rhythm clock runs independent of the actual battle logic frequency,
/// which is typically around 60hz. In an ideal world, the rhythm clock will
/// run at the same pace at all times, but because of latency and time drift,
/// the pace of the rhythm clock will have to be adjusted.
///
/// # Warning!
/// Do **not** use [`Time::elapsed`] to get elapsed time since the song starts,
/// since it does not take into account seeks and rate changes, and is
/// particularly useless after loading more than one beatmap.
#[derive(Clone)]
pub struct Rhythm {
    bpm: u32,
    crotchet: Duration,
    offset: Duration,

    timestamp: Duration,

    position: Duration,
    is_interpolating: bool,
}

impl Rhythm {
    /// Initializes a rhythm clock with settings.
    pub fn new(bpm: u32, offset: Duration) -> Rhythm {
        Rhythm {
            bpm,
            crotchet: Duration::from_nanos(1_000_000_000 * 60 / bpm as u64),
            offset,

            timestamp: Duration::ZERO,

            position: Duration::ZERO,
            is_interpolating: false,
        }
    }

    /// Returns the BPM of the current song.
    pub fn bpm(&self) -> u32 {
        self.bpm
    }

    /// Returns the crotchet (the time between beats) of the current song.
    pub fn crotchet(&self) -> Duration {
        self.crotchet
    }

    /// Returns the start offset of the current song.
    pub fn offset(&self) -> Duration {
        self.offset
    }
}

impl Default for Rhythm {
    fn default() -> Self {
        Rhythm::new(60, Duration::from_millis(0))
    }
}

/// Rhythm extension methods.
pub trait RhythmExt {
    /// The current position of the song, interpolated by the rhythm clock.
    fn position(&self) -> Duration;

    /// The timestamp of the song, starting from `offset`.
    ///
    /// This returns how much data was processed of the song. This does not
    /// update smoothly between frames! On most modern systems, this updates
    /// once every 3 or 4 frames due to audio buffering. For elapsed time since
    /// the start of the song, use [`RhythmExt::position`].
    fn dsp_time(&self) -> Duration;

    /// Returns the position of a beat in the song.
    ///
    /// # Panics
    /// Panics if `beat` is negative.
    fn beat_position(&self, beat: f32) -> Duration;

    /// The beat number that the song is on.
    ///
    /// This returns a float that represents the current beat, with `0.0` being
    /// the first beat. This can be negative when waiting for the song to get
    /// past the start offset.
    fn beat_number(&self) -> f32;
}

impl RhythmExt for Time<Rhythm> {
    fn position(&self) -> Duration {
        self.context().position
    }

    fn dsp_time(&self) -> Duration {
        let ctx = self.context();

        if let Some(timestamp) = self.elapsed().checked_sub(ctx.offset) {
            timestamp
        } else {
            Duration::ZERO
        }
    }

    fn beat_position(&self, beat: f32) -> Duration {
        assert!(beat >= 0.);

        let ctx = self.context();
        ctx.crotchet.mul_f32(beat) + ctx.offset
    }

    fn beat_number(&self) -> f32 {
        let elapsed = self.position().as_secs_f32();
        let ctx = self.context();

        // get timestamp
        let timestamp = elapsed - ctx.offset.as_secs_f32();

        // get crochet
        let crochet = ctx.crotchet.as_secs_f32();

        timestamp / crochet
    }
}

fn spawn_beatmap(
    mut new_beatmaps: Query<
        (Entity, &Handle<Beatmap>, &mut Handle<AudioSource>),
        Without<BeatmapInstance>,
    >,
    beatmaps: Res<Assets<Beatmap>>,
    image_assets: Res<ImageAssets>,
    mut rhythm: ResMut<Time<Rhythm>>,
    mut commands: Commands,
) {
    for (entity, beatmap_handle, mut audio_handle) in new_beatmaps.iter_mut() {
        if let Some(beatmap) = beatmaps.get(beatmap_handle) {
            // update audio handle
            *audio_handle = beatmap.song.handle.clone();

            // create new rhythm clock
            *rhythm = Time::new_with(Rhythm::new(beatmap.song.bpm, beatmap.song.offset()));

            // spawn lanes
            let first_x = (1. - beatmap.lane_count as f32) * (NOTE_WIDTH / 2.);

            for i in 0..beatmap.lane_count {
                let map = [KeyCode::KeyZ, KeyCode::KeyX, KeyCode::KeyN, KeyCode::KeyM];

                // find transform
                let x = first_x + NOTE_WIDTH * (i as f32);

                let transform = Transform::from_xyz(x, 0., 1.);

                // spawn the parent entity
                commands
                    // TODO: input remapping
                    .spawn((
                        LaneBundle {
                            transform,
                            lane: Lane::new(i),
                            ..Default::default()
                        },
                        LaneInputKeyboard::new(map[i as usize]),
                    ))
                    .set_parent(entity)
                    .with_children(|parent| {
                        // spawn judgement area
                        parent.spawn(SpriteBundle {
                            texture: image_assets.judgement_area.clone(),
                            sprite: Sprite {
                                color: Color::WHITE,
                                ..Default::default()
                            },
                            ..Default::default()
                        });

                        // spawn each note in the lane
                        for note in beatmap.notes().iter().filter(|n| n.lane == i) {
                            parent.spawn((
                                SpriteBundle {
                                    texture: image_assets.note_default.clone(),
                                    sprite: Sprite {
                                        color: Color::WHITE,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                                Note::from(note.clone()),
                            ));
                        }
                    });
            }

            // instance beatmap
            commands.entity(entity).insert(BeatmapInstance::default());

            info!(
                "spawned beatmap (song: \"{}\")",
                beatmap.song.path.display()
            );
        }
    }
}

fn interpolate_rhythm_clock(
    main_track: Query<&AudioControl, With<MainTrack>>,
    time: Res<Time<Real>>,
    mut rhythm: ResMut<Time<Rhythm>>,
) {
    if let Ok(ctl) = main_track.get_single() {
        let mut current_time = rhythm.elapsed().as_secs_f32();
        let rhythm_ctx = rhythm.context_mut();

        let Rhythm {
            timestamp: last_timestamp,
            position: last_position,
            ..
        } = *rhythm_ctx;

        // get next timestamp
        rhythm_ctx.timestamp = ctl.sample_duration() * ctl.timestamp() as u32;

        if rhythm_ctx.is_interpolating {
            // interpolate time on clock
            current_time += time.delta_seconds();

            // if there is a time difference, adjust for the difference
            current_time += (rhythm_ctx.timestamp.as_secs_f32() - current_time) / 8.;

            // update new time
            rhythm_ctx.position = Duration::from_secs_f32(current_time);
        } else {
            // check if the source has even elapsed
            if last_timestamp != rhythm_ctx.timestamp {
                // set interpolation
                rhythm_ctx.is_interpolating = true;
            }

            rhythm_ctx.position = rhythm_ctx.timestamp;
        }

        // if we moved forward, set elapsed time
        if let Some(delta) = rhythm_ctx.position.checked_sub(last_position) {
            rhythm.advance_by(delta);
        }
    }
}

/// Spawns "hit effects" after notes **hit**.
pub fn spawn_hit_effects(
    mut judgements: EventReader<JudgementEvent>,
    notes: Query<&Parent, With<Note>>,
    lanes: Query<&GlobalTransform, With<Lane>>,
    mut commands: Commands,
    image_assets: Res<ImageAssets>,
) {
    for judgement in judgements.read() {
        if judgement.offset.is_none() {
            // skip missed notes :(
            continue;
        }

        // get note
        let Ok(parent) = notes.get(judgement.note) else {
            continue;
        };

        // get lane position
        let Ok(lane_pos) = lanes.get(parent.get()) else {
            continue;
        };

        // spawn hit effect **at** lane position
        commands.spawn((
            SpriteBundle {
                texture: image_assets.judgement_hit.clone(),
                transform: Transform::from_translation(lane_pos.translation()),
                ..Default::default()
            },
            TextureAtlas {
                index: 0,
                layout: image_assets.judgement_hit_layout.clone(),
            },
            AnimationFrames::new(0, 5),
            AnimationTimer::despawn_after(Duration::from_millis(125)),
        ));
    }
}

/// Makes notes disappear after they have been hit (or missed).
///
/// This is not a system that is added automatically, the app composer should
/// add flair as necessary.
pub fn vanish_passed_notes(
    mut judgements: EventReader<JudgementEvent>,
    mut notes: Query<&mut Visibility, With<Note>>,
) {
    for judgement in judgements.read() {
        if let Ok(mut note) = notes.get_mut(judgement.note) {
            *note = Visibility::Hidden;
        }
    }
}
