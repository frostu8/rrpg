//! Higher level rhythm tracking.

pub mod asset;

use bevy::prelude::*;

use std::time::Duration;

use crate::audio::{AudioControl, AudioSource};

use asset::{Beatmap, BeatmapLoader};

/// Rhythm plugin.
pub struct RhythmPlugin;

impl Plugin for RhythmPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Beatmap>()
            .register_asset_loader(BeatmapLoader)
            .insert_resource(Time::new_with(Rhythm::default()))
            .add_systems(PreUpdate, (spawn_beatmap, update_rhythm_clock));
    }
}

/// Loads a beatmap in.
///
/// This bundle includes [`MainTrack`] as a component. Remember to clean this
/// up after the song is concluded!
#[derive(Bundle, Default)]
pub struct BeatmapBundle {
    beatmap: Handle<Beatmap>,
    audio_source: Handle<AudioSource>,
    audio_control: AudioControl,
    main_track: MainTrack,
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
}

impl Rhythm {
    /// Initializes a rhythm clock with settings.
    pub fn new(bpm: u32, offset: Duration) -> Rhythm {
        Rhythm {
            bpm,
            crotchet: Duration::from_nanos(1_000_000_000 * 60 / bpm as u64),
            offset,

            timestamp: Duration::ZERO,
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
    /// The timestamp of the song, starting from `offset`>
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

        if let Some(timestamp) = ctx.timestamp.checked_sub(ctx.offset) {
            timestamp
        } else {
            Duration::ZERO
        }
    }

    fn beat_number(&self) -> f32 {
        let ctx = self.context();

        // get timestamp
        let timestamp = ctx.timestamp.as_secs_f32();
        let timestamp = timestamp - ctx.offset.as_secs_f32();

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
    mut rhythm: ResMut<Time<Rhythm>>,
    mut commands: Commands,
) {
    for (entity, beatmap_handle, mut audio_handle) in new_beatmaps.iter_mut() {
        if let Some(beatmap) = beatmaps.get(beatmap_handle) {
            // update audio handle
            *audio_handle = beatmap.song.handle.clone();

            // create new rhythm clock
            *rhythm = Time::new_with(Rhythm::new(beatmap.song.bpm, beatmap.song.offset()));

            // TODO: spawn notes

            // instance beatmap
            commands.entity(entity).insert(BeatmapInstance);

            info!(
                "spawned beatmap (song: \"{}\")",
                beatmap.song.path.display()
            );
        }
    }
}

fn update_rhythm_clock(
    main_track: Query<&AudioControl, With<MainTrack>>,
    time: Res<Time<Real>>,
    mut rhythm: ResMut<Time<Rhythm>>,
) {
    if let Ok(ctl) = main_track.get_single() {
        let timestamp = ctl.timestamp();

        let elapsed = rhythm.elapsed();
        let rhythm_ctx = rhythm.context_mut();

        // get next timestamp
        rhythm_ctx.timestamp = ctl.sample_duration() * timestamp as u32;

        // progress clock to timestamp but do not overstep
        let next_elapsed = elapsed + time.delta();
        let new_time = std::cmp::min(next_elapsed, rhythm.timestamp());

        rhythm.advance_to(new_time);
    }
}
