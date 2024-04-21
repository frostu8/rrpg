//! Custom audio solution for precise audio timings.

mod asset;

use asset::Decoder;
pub use asset::{AudioLoader, AudioSource};

use bevy::prelude::*;

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, Sample, SampleFormat, SampleRate, Stream, StreamConfig,
    SupportedBufferSize,
};

/// The default controlled sample rate.
pub const SAMPLE_RATE: u32 = 44_100;

/// The nanoseconds per sample.
pub const NANOS_PER_SAMPLE: u64 = 1_000_000_000 / 44_100;

/// The default channels of audio.
pub const CHANNEL_COUNT: u16 = 2;

/// Includes audio systems and components.
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<AudioSource>()
            .init_asset_loader::<AudioLoader>()
            .init_non_send_resource::<AudioDevice>()
            .insert_resource(Time::new_with(Rhythm::default()))
            .init_resource::<NextTrack>()
            .add_systems(PreUpdate, update_rhythm_clock)
            .add_systems(Update, start_next_track)
            .add_systems(Startup, setup_sound_device);
    }
}

/// Queues the next song to play on the audio device.
///
/// When the song begins playing, an event will be fired (TBD).
#[derive(Default, Resource)]
pub struct NextTrack(Option<Handle<AudioSource>>);

impl NextTrack {
    /// Sets the next track.
    pub fn set(&mut self, next: Handle<AudioSource>) {
        self.0 = Some(next);
    }
}

/// Has useful low-level sound primitives.
#[derive(Default)]
struct AudioDevice {
    state: Option<AudioState>,
}

struct AudioState {
    device: Device,
    stream: Stream,
    timestamp: Arc<AtomicU64>,
    song_queue: Sender<Decoder>,
}

impl AudioDevice {
    /// Initializes a cpal [`Device`] on this audio device.
    pub fn init(&mut self, device: Device) -> Result<(), String> {
        // find configs
        let mut supported_configs_range = match device.supported_output_configs() {
            Ok(s) => s,
            Err(err) => return Err(format!("no configs found: {}", err)),
        };

        let sample_rate = SampleRate(SAMPLE_RATE);

        let supported_config = supported_configs_range
            .filter(|s| s.channels() == CHANNEL_COUNT)
            .filter(|s| sample_rate >= s.min_sample_rate())
            .filter(|s| sample_rate <= s.max_sample_rate())
            .filter(|s| s.sample_format() == SampleFormat::I16)
            .next()
            .expect("no supported config")
            .with_sample_rate(sample_rate);

        println!("{:?}", supported_config);

        let config = supported_config.into();

        let (song_queue_tx, song_queue_rx) = channel();
        let timestamp = Arc::new(AtomicU64::new(0));

        // build audio decoder thread
        let stream = device
            .build_output_stream(
                &config,
                audio_streamer(timestamp.clone(), song_queue_rx),
                move |err| {
                    error!("stream error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("setup stream err: {}", e))
            .and_then(|s| match s.play() {
                Ok(()) => Ok(s),
                Err(err) => Err(format!("setup stream err: {}", err)),
            });

        match stream {
            Ok(stream) => {
                self.state = Some(AudioState {
                    device,
                    stream,
                    timestamp,
                    song_queue: song_queue_tx,
                });

                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Plays audio on song priority.
    ///
    /// This resets timings! The timings of [`Rhythm`] should also be reset so
    /// these don't get messed up.
    pub fn play(&self, song: AudioSource) -> Result<(), lewton::VorbisError> {
        if let Some(state) = &self.state {
            // get decoder
            let decoder = Decoder::new(song)?;
            // send song over
            let _ = state.song_queue.send(decoder);
        }

        Ok(())
    }

    /// Gets the timestamp of the audio device.
    pub fn timestamp(&self) -> Option<u64> {
        self.state
            .as_ref()
            .map(|s| s.timestamp.load(Ordering::Acquire))
    }
}

fn audio_streamer(
    timestamp: Arc<AtomicU64>,
    song_queue: Receiver<Decoder>,
) -> impl FnMut(&mut [i16], &cpal::OutputCallbackInfo) + Send + 'static {
    let mut source: Option<Decoder> = None;

    move |data, _| {
        if let Ok(decoder) = song_queue.try_recv() {
            let (ident, _, _) = decoder.headers();

            info!(
                "got track, c = {}, sample_rate = {}",
                ident.audio_channels, ident.audio_sample_rate
            );
            // load source onto player
            source = Some(decoder);
            // reset timestamp
            timestamp.store(0, Ordering::Release);
        }

        let mix_len = if let Some(decoder) = &mut source {
            match decoder.sample_all(data) {
                Ok(len) => len,
                Err(err) => {
                    error!("stream dropped: {}", err);
                    0
                }
            }
        } else {
            0
        };

        // fill the rest with silence
        for i in mix_len..data.len() {
            data[i] = Sample::EQUILIBRIUM;
        }

        // count samples requested to produce accumulated time
        let samples = mix_len as u64 / CHANNEL_COUNT as u64;
        timestamp.fetch_add(samples, Ordering::AcqRel);
    }
}

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
            offset: Duration::from_millis(570),
        }
    }
}

/// Rhythm extension methods.
pub trait RhythmExt {
    /// The timestamp of the song, starting from `offset`>
    fn timestamp(&self) -> Duration;

    /// The beat number that the song is on.
    fn beat_number(&self) -> u32;
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

    fn beat_number(&self) -> u32 {
        let ctx = self.context();

        self.timestamp().div_duration_f64(ctx.crochet) as u32
    }
}

fn update_rhythm_clock(
    audio_device: NonSend<AudioDevice>,
    time: Res<Time<Real>>,
    mut rhythm: ResMut<Time<Rhythm>>,
) {
    if let Some(timestamp) = audio_device.timestamp() {
        let elapsed = rhythm.elapsed();
        let rhythm_ctx = rhythm.context_mut();

        // get next timestamp
        rhythm_ctx.timestamp = Duration::from_nanos(NANOS_PER_SAMPLE * timestamp);

        // progress clock to timestamp but do not overstep
        let next_elapsed = elapsed + time.delta();
        let new_time = std::cmp::min(next_elapsed, rhythm.timestamp());

        rhythm.advance_to(new_time);
    }
}

fn start_next_track(
    audio_device: NonSendMut<AudioDevice>,
    mut next_track: ResMut<NextTrack>,
    audio_sources: Res<Assets<AudioSource>>,
) {
    if let Some(next) = &next_track.0 {
        // try and get data
        if let Some(track) = audio_sources.get(next) {
            // load track into audio device
            if let Err(err) = audio_device.play(track.clone()) {
                error!("failed to play track: {}", err);
            }

            next_track.0 = None;
        }
    }
}

fn setup_sound_device(mut audio_device: NonSendMut<AudioDevice>) {
    let host = cpal::default_host();

    let Some(device) = host.default_output_device() else {
        warn!("No audio device found!");
        return;
    };

    let name = device.name();

    match audio_device.init(device) {
        Ok(()) => {
            info!("Successfuly initialized audio on device: {:?}", name);
        }
        Err(err) => {
            error!("got error initializing device stream: {}", err);
        }
    }
}
