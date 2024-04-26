//! Custom audio solution for precise audio timings.

mod asset;
pub mod source;

pub use asset::{AudioLoader, AudioSource};
use source::{OggDecoder, Resampler, Source};

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
    Device, Sample, SampleFormat, SampleRate, Stream, StreamConfig,
};

/// The desired sample rate.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

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
            .add_systems(PreUpdate, send_sound_events)
            .add_systems(Update, start_spawned_audio)
            .add_systems(Startup, setup_sound_device);
    }
}

/// A bundle for playing audio.
///
/// When this is spawned, the audio will immediately begin playing.
#[derive(Bundle, Default)]
pub struct AudioBundle {
    pub source: Handle<AudioSource>,
    pub actl: AudioControl,
}

/// A component for audio source control.
///
/// # Note
/// The buffer for `cpal` on most platforms (including WASM, the platform this
/// game will ship on!) is quite large. Controlling audio may have delays of
/// multiple frames, and the [`AudioControl::timestamp`] function may record
/// large jumps in timestamp.
#[derive(Clone, Component)]
pub struct AudioControl {
    sample_rate: u32,
    inner: Arc<AudioControlState>,
}

impl AudioControl {
    /// Returns the song position as a [`Duration`].
    pub fn position(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / self.sample_rate as u64) * self.timestamp() as u32
    }

    /// Returns the timestamp of the audio in samples.
    pub fn timestamp(&self) -> u64 {
        self.inner.timestamp.load(Ordering::Acquire)
    }
}

impl Default for AudioControl {
    /// Creates an unheaded `AudioControl`.
    fn default() -> Self {
        AudioControl {
            sample_rate: DEFAULT_SAMPLE_RATE,
            inner: Arc::new(AudioControlState {
                timestamp: AtomicU64::new(0),
            }),
        }
    }
}

struct AudioControlState {
    timestamp: AtomicU64,
}

/// Marker component for loaded audio.
#[derive(Clone, Copy, Component, Debug, Default)]
pub struct LoadedAudio;

#[derive(Default)]
struct AudioDevice {
    state: Option<AudioState>,
}

struct AudioState {
    _stream: Stream,
    streamer_options: StreamerOptions,
    audio_queue: Sender<(OggDecoder, Arc<AudioControlState>)>,
}

impl AudioDevice {
    /// Returns the sample rate of the audio device.
    ///
    /// Timestamps returned by [`AudioControl`] are sampled at this rate, so
    /// this can be used to give an approximate timestamp in real time.
    ///
    /// Returns `None` if the audio device hasn't been initialized
    pub fn sample_rate(&self) -> Option<u32> {
        self.state.as_ref().map(|s| s.streamer_options.sample_rate)
    }

    /// Initializes a cpal [`Device`] on this audio device.
    #[allow(clippy::filter_next)] // disable lint for readability
    pub fn init(&mut self, device: Device) -> Result<(), String> {
        // find configs
        let supported_configs_range = match device.supported_output_configs() {
            Ok(s) => s,
            Err(err) => return Err(format!("no configs found: {}", err)),
        };

        let desired_sample_rate = DEFAULT_SAMPLE_RATE;

        let mut supported_configs = supported_configs_range
            .filter(|s| s.channels() == CHANNEL_COUNT)
            .filter(|s| s.sample_format() == SampleFormat::I16)
            .map(|s| {
                // score everything based on a distance from desired sample
                // rate (44_100 or 48_000)
                let min_sample_rate = s.min_sample_rate().0;
                let max_sample_rate = s.max_sample_rate().0;

                if desired_sample_rate < min_sample_rate {
                    let score = min_sample_rate - desired_sample_rate;
                    (s.with_sample_rate(SampleRate(min_sample_rate)), score)
                } else if desired_sample_rate > max_sample_rate {
                    let score = min_sample_rate - desired_sample_rate;
                    (s.with_sample_rate(SampleRate(max_sample_rate)), score)
                } else {
                    (s.with_sample_rate(SampleRate(desired_sample_rate)), 0)
                }
            })
            .collect::<Vec<_>>();

        // sort by score asc
        supported_configs.sort_unstable_by(|(_, a), (_, b)| a.cmp(b));

        info!("found audio configs:");

        for config in supported_configs.iter() {
            info!("{:?}", config);
        }

        // pull first config
        let config = Into::<StreamConfig>::into(
            supported_configs
                .into_iter()
                .next()
                .map(|(s, _)| s)
                .ok_or_else(|| String::from("No valid audio device config found!"))?,
        );

        let streamer_options = StreamerOptions {
            channels: config.channels as u8,
            sample_rate: config.sample_rate.0,
        };

        let (audio_queue_tx, audio_queue_rx) = channel();

        // build audio decoder thread
        let stream = device
            .build_output_stream(
                &config,
                audio_streamer(streamer_options.clone(), audio_queue_rx),
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
                    streamer_options,
                    audio_queue: audio_queue_tx,
                });

                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Plays audio.
    pub fn play(&self, audio: AudioSource, ctl: &AudioControl) -> Result<(), lewton::VorbisError> {
        if let Some(state) = &self.state {
            // create decoder and state
            let decoder = OggDecoder::new(audio)?;
            // send song over
            let _ = state.audio_queue.send((decoder, ctl.inner.clone()));
        }

        Ok(())
    }
}

#[derive(Clone)]
struct StreamerOptions {
    sample_rate: u32,
    channels: u8,
}

/// An event sent when the track starts.
#[derive(Clone, Debug, Event)]
pub struct TrackStart;

fn audio_streamer(
    streamer_options: StreamerOptions,
    audio_queue: Receiver<(OggDecoder, Arc<AudioControlState>)>,
) -> impl FnMut(&mut [i16], &cpal::OutputCallbackInfo) + Send + 'static {
    let mut source: Option<(Resampler<OggDecoder>, Arc<AudioControlState>)> = None;

    move |data, _| {
        if let Ok((decoder, actl)) = audio_queue.try_recv() {
            info!(
                "got track, c = {}, sample_rate = {}",
                decoder.channels(),
                decoder.sample_rate(),
            );
            // reset timestamp
            actl.timestamp.store(0, Ordering::Release);
            // load source onto player
            let resampler = Resampler::new(decoder, streamer_options.sample_rate).unwrap();
            source = Some((resampler, actl));
        }

        let mix_len = if let Some((decoder, actl)) = &mut source {
            let mix_len = match decoder.sample(data) {
                Ok(len) => len,
                Err(err) => {
                    error!("stream dropped: {}", err);
                    0
                }
            };

            // count mix len as samples
            let samples = mix_len as u64 / CHANNEL_COUNT as u64;
            actl.timestamp.fetch_add(samples, Ordering::AcqRel);

            mix_len
        } else {
            0
        };

        // fill the rest with silence
        for data in data.iter_mut().skip(mix_len) {
            *data = Sample::EQUILIBRIUM;
        }
    }
}

fn send_sound_events(
    _audio_device: NonSendMut<AudioDevice>,
    mut _track_start_tx: EventWriter<TrackStart>,
) {
    // TODO: impl
}

fn start_spawned_audio(
    mut query: Query<(Entity, &Handle<AudioSource>, &mut AudioControl), Without<LoadedAudio>>,
    audio_sources: Res<Assets<AudioSource>>,
    audio_device: NonSendMut<AudioDevice>,
    mut commands: Commands,
) {
    for (entity, audio_source, mut actl) in query.iter_mut() {
        if let Some(audio_source) = audio_sources.get(audio_source) {
            if let Some(state) = audio_device.state.as_ref() {
                actl.sample_rate = state.streamer_options.sample_rate;
            }

            // start playing sound
            if let Err(err) = audio_device.play(audio_source.clone(), &actl) {
                error!("Failed to play audio: {}", err);
            }

            commands.entity(entity).insert(LoadedAudio);
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
