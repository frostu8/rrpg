//! Higher level rhythm tracking.

use bevy::prelude::*;

use std::time::Duration;

use crate::audio::AudioControl;

/// Rhythm plugin.
pub struct RhythmPlugin;

impl Plugin for RhythmPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::new_with(Rhythm::default()))
            .add_systems(PreUpdate, update_rhythm_clock);
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
#[derive(Clone)]
pub struct Rhythm {
    crochet: Duration,
    timestamp: Duration,
    offset: Duration,
}

impl Default for Rhythm {
    fn default() -> Self {
        Rhythm {
            crochet: Duration::from_nanos(1_000_000_000 * 60 / 170),
            timestamp: Duration::ZERO,
            offset: Duration::from_millis(670),
        }
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
        let crochet = ctx.crochet.as_secs_f32();

        timestamp / crochet
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
