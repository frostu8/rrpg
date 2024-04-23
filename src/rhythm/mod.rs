//! Higher level rhythm tracking.

pub mod asset;
pub mod note;

use bevy::prelude::*;
use bevy::transform::TransformSystem;

use bevy_asset_loader::prelude::*;

use std::time::Duration;

use crate::{
    audio::{AudioControl, AudioSource},
    GameState,
};

use asset::{Beatmap, BeatmapLoader};

use note::{Lane, LaneBundle, Note};

/// The width of a single note.
pub const NOTE_WIDTH: f32 = 16.;

/// Rhythm plugin.
pub struct RhythmPlugin;

impl Plugin for RhythmPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Beatmap>()
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
                PostUpdate,
                (note::reorder_notes, note::update_note_transform)
                    .chain()
                    .before(TransformSystem::TransformPropagate),
            );
    }
}

/// Sprite assets for the rhythm-game UI.
#[derive(AssetCollection, Resource)]
pub struct ImageAssets {
    #[asset(path = "sprites/note_default.png")]
    pub note_default: Handle<Image>,
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
/// This component is inserted when all the notes are finished spawning.
#[derive(Clone, Copy, Component, Default, Debug)]
pub struct BeatmapInstance;

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
#[derive(Clone)]
pub struct Rhythm {
    bpm: u32,
    crotchet: Duration,
    offset: Duration,

    timestamp: Duration,
    started_at: Duration,
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
            started_at: Duration::ZERO,
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
    /// The timestamp of the song, starting from `offset`.
    fn timestamp(&self) -> Duration;

    /// The beat number that the song is on.
    ///
    /// This returns a float that represents the current beat, with `0.0` being
    /// the first beat. This can be negative when waiting for the song to get
    /// past the start offset.
    fn beat_number(&self) -> f32;
}

impl RhythmExt for Time<Rhythm> {
    fn timestamp(&self) -> Duration {
        let ctx = self.context();

        if let Some(timestamp) = self.elapsed().checked_sub(ctx.offset) {
            timestamp
        } else {
            Duration::ZERO
        }
    }

    fn beat_number(&self) -> f32 {
        let elapsed = self.elapsed().as_secs_f32();
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
                // find transform
                let x = first_x + NOTE_WIDTH * (i as f32);

                let transform = Transform::from_xyz(x, 0., 1.);

                // spawn the parent entity
                commands
                    .spawn(LaneBundle {
                        transform,
                        lane: Lane::new(i),
                        ..Default::default()
                    })
                    .set_parent(entity)
                    .with_children(|parent| {
                        // spawn each note in the lane
                        for note in beatmap.notes().iter().filter(|n| n.lane == i) {
                            parent.spawn((
                                SpriteBundle {
                                    texture: image_assets.note_default.clone(),
                                    sprite: Sprite {
                                        color: Color::RED,
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
            commands.entity(entity).insert(BeatmapInstance);

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

        let last_timestamp = rhythm_ctx.timestamp;

        // get next timestamp
        rhythm_ctx.timestamp = ctl.sample_duration() * ctl.timestamp() as u32;

        if rhythm_ctx.is_interpolating {
            // interpolate time on clock
            current_time += time.delta_seconds();

            // if there is a time difference, adjust for the difference
            current_time += (rhythm_ctx.timestamp.as_secs_f32() - current_time) / 8.;

            // update new time
            rhythm.advance_to(Duration::from_secs_f32(current_time));
        } else {
            // check if the source has even elapsed
            if last_timestamp != rhythm_ctx.timestamp {
                // set interpolation
                rhythm_ctx.is_interpolating = true;
            }

            let timestamp = rhythm_ctx.timestamp;
            rhythm.advance_to(timestamp);
        }
    }
}
