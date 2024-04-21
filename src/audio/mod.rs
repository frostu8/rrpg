//! Custom audio solution for precise audio timings.

mod asset;

use asset::Decoder;
pub use asset::{AudioLoader, AudioSource};

use bevy::prelude::*;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SampleFormat, SampleRate, Stream,
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
        app.add_event::<TrackStart>()
            .init_asset::<AudioSource>()
            .init_asset_loader::<AudioLoader>()
            .init_non_send_resource::<AudioDevice>()
            .init_resource::<NextTrack>()
            .add_systems(PreUpdate, send_sound_events)
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
pub struct AudioDevice {
    state: Option<AudioState>,
}

struct AudioState {
    _stream: Stream,
    inner: Arc<AudioStateInner>,
    song_queue: Sender<Decoder>,
}

struct AudioStateInner {
    timestamp: AtomicU64,
    started: AtomicBool,
}

impl AudioDevice {
    /// Initializes a cpal [`Device`] on this audio device.
    fn init(&mut self, device: Device) -> Result<(), String> {
        // find configs
        let supported_configs_range = match device.supported_output_configs() {
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
        let inner = Arc::new(AudioStateInner {
            timestamp: AtomicU64::new(0),
            started: AtomicBool::new(false),
        });

        // build audio decoder thread
        let stream = device
            .build_output_stream(
                &config,
                audio_streamer(inner.clone(), song_queue_rx),
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
                    _stream: stream,
                    inner,
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
    fn play(&self, song: AudioSource) -> Result<(), lewton::VorbisError> {
        if let Some(state) = &self.state {
            // get decoder
            let decoder = Decoder::new(song)?;
            // send song over
            let _ = state.song_queue.send(decoder);
        }

        Ok(())
    }

    fn state(&self) -> Option<&AudioState> {
        self.state.as_ref()
    }

    /// Gets the sample rate of the audio.
    ///
    /// For now, always `44_100`.
    pub fn sample_rate(&self) -> u64 {
        SAMPLE_RATE as u64
    }

    /// Returns the duration of each sample.
    pub fn sample_duration(&self) -> Duration {
        Duration::from_nanos(NANOS_PER_SAMPLE)
    }

    /// Gets the timestamp of the currently playing track in samples.
    ///
    /// This says nothing about the duration of the stream, rather, it says
    /// how long the main track (enqueued using [`NextTrack`]) has been
    /// playing.
    pub fn timestamp(&self) -> u64 {
        self.try_timestamp().expect("init audio device")
    }

    /// Gets the timestamp of the currently playing track, or `None` if there
    /// is no initialized audio stream.
    pub fn try_timestamp(&self) -> Option<u64> {
        self.state
            .as_ref()
            .map(|s| s.inner.timestamp.load(Ordering::Acquire))
    }
}

/// An event sent when the track starts.
#[derive(Clone, Debug, Event)]
pub struct TrackStart;

fn audio_streamer(
    inner: Arc<AudioStateInner>,
    song_queue: Receiver<Decoder>,
) -> impl FnMut(&mut [i16], &cpal::OutputCallbackInfo) + Send + 'static {
    let mut source: Option<Decoder> = None;

    move |data, _| {
        let mut set_started = false;

        if let Ok(decoder) = song_queue.try_recv() {
            let (ident, _, _) = decoder.headers();

            info!(
                "got track, c = {}, sample_rate = {}",
                ident.audio_channels, ident.audio_sample_rate
            );
            // load source onto player
            source = Some(decoder);
            // reset timestamp
            inner.timestamp.store(0, Ordering::Release);
            set_started = true;
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
        inner.timestamp.fetch_add(samples, Ordering::AcqRel);

        if set_started {
            inner.started.store(true, Ordering::Release);
        }
    }
}

fn send_sound_events(
    audio_device: NonSendMut<AudioDevice>,
    mut track_start_tx: EventWriter<TrackStart>,
) {
    // gets started flag
    if let Some(state) = audio_device.state() {
        if state.inner.started.load(Ordering::Acquire) {
            // clear started bit
            state.inner.started.store(false, Ordering::Release);

            // send event
            track_start_tx.send(TrackStart);
        }
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
