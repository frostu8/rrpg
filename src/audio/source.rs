//! Lower level audio processing types and traits.

use std::cmp::min;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Cursor;
use std::time::Duration;

use super::asset::AudioSource;

use dasp_sample::Sample;
use lewton::inside_ogg::OggStreamReader;
use rubato::{FftFixedIn, Resampler as _};

/// A source of audio that can be sampled.
pub trait Source {
    /// The error type returned by [`Source::sample`].
    type Error: Debug;

    /// The sample rate of the audio source.
    ///
    /// The return value of this **SHOULD REMAIN CONSTANT** from the start of
    /// reading indefinitely.
    fn sample_rate(&self) -> u32;

    /// The channel count of the audio source.
    fn channels(&self) -> u8;

    /// Reads some audio samples.
    ///
    /// The read samples are channel-interleaved. Returns how many **single**
    /// samples were read. To get how many sample blocks were read, divide the
    /// returned usize by `channels`.
    ///
    /// # Note on Channels
    /// Attempting to read a number of samples that isn't evenly divisible by
    /// `channels` is not valid, and implementations are free to panic or
    /// return errors as a result. Similarly, it is incorrect for
    /// implementations to return results that aren't evenly divisible by
    /// `channels`.
    fn sample(&mut self, buf: &mut [i16]) -> Result<usize, Self::Error>;

    /// Seeks the audio source for a specific position in samples on a
    /// per-channel basis.
    fn seek(&mut self, position: usize) -> Result<(), Self::Error>;
}

/// A thin wrapper over a [`Source`] that counts position in samples.
pub struct TrackedSource<T> {
    inner: T,
    position: usize,
}

impl<T> TrackedSource<T> {
    /// Creates a new `TrackedSource`.
    pub fn new(inner: T) -> TrackedSource<T> {
        TrackedSource { inner, position: 0 }
    }

    /// Returns the position of the reader in samples.
    ///
    /// Specifically, this returns how many samples have cumulatively been read
    /// from the source. If this audio is fed into an audio device, this is how
    /// many samples have been fed *into* the audio device, **not** how many
    /// samples the audio has played.
    ///
    /// For rhythm tracking, use an interpolating clock!
    pub fn position(&self) -> usize {
        self.position
    }
}

impl<T> TrackedSource<T>
where
    T: Source,
{
    /// Returns the current position of the song as a [`Duration`].
    ///
    /// See [`TrackedSource::position`] for caveats.
    pub fn position_time(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / self.inner.sample_rate() as u64) * self.position as u32
    }
}

impl<T> Source for TrackedSource<T>
where
    T: Source,
{
    type Error = T::Error;

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn channels(&self) -> u8 {
        self.inner.channels()
    }

    fn sample(&mut self, buf: &mut [i16]) -> Result<usize, Self::Error> {
        let sample_count = self.inner.sample(buf)?;

        self.position += sample_count;

        Ok(sample_count)
    }

    fn seek(&mut self, position: usize) -> Result<(), Self::Error> {
        self.inner.seek(position)?;

        self.position = position;

        Ok(())
    }
}

/// A resampler source that converts between sample rates using [`rubato`][1].
/// This struct intelligently acts as a passthrough in the case that `from` ==
/// `to`.
///
/// [1]: https://github.com/HEnquist/rubato
pub struct Resampler<T> {
    inner: T,
    to: u32,

    state: Option<ResamplerState>,
}

struct ResamplerState {
    from_buffer_itl: Vec<i16>,
    from_buffer: Vec<Vec<f32>>,
    to_buffer: Vec<Vec<f32>>,
    to_buffer_rem: usize,
    fft: FftFixedIn<f32>,
    eof: bool,
}

impl<T> Resampler<T>
where
    T: Source,
{
    /// Creates a new `Resampler`.
    ///
    /// # Panics
    /// Panics if `to` or the `sample_rate` of `T` is `0`.
    pub fn new(inner: T, to: u32) -> Result<Resampler<T>, rubato::ResamplerConstructionError> {
        // ideally we want to choose a number close to the frame size of the
        // audio output device's frame to prevent lag spikes
        Resampler::<T>::new_with_chunk_size(inner, to, 4096)
    }

    /// Creates a new `Resampler` with the provided chunk size.
    ///
    /// # Panics
    /// Panics if `to` or the `sample_rate` of `T` is `0`.
    pub fn new_with_chunk_size(
        inner: T,
        to: u32,
        chunk_size: usize,
    ) -> Result<Resampler<T>, rubato::ResamplerConstructionError> {
        let from = inner.sample_rate();
        let channels = inner.channels();

        assert!(from > 0);
        assert!(to > 0);

        let state = if from == to {
            // activate passthrough mode
            None
        } else {
            let fft =
                FftFixedIn::new(from as usize, to as usize, chunk_size, 1, channels as usize)?;
            let from_buffer = fft.input_buffer_allocate(false);

            let itl_buffer_len = from_buffer
                .iter()
                .map(|s| s.capacity())
                .fold(0, |a, b| a + b);

            Some(ResamplerState {
                from_buffer_itl: (0..itl_buffer_len).map(|_| 0).collect(),
                from_buffer,
                to_buffer: fft.output_buffer_allocate(true),
                to_buffer_rem: 0,
                fft,
                eof: false,
            })
        };

        Ok(Resampler { inner, to, state })
    }
}

impl<T> Source for Resampler<T>
where
    T: Source,
{
    type Error = ResampleError<T::Error>;

    fn sample_rate(&self) -> u32 {
        self.to
    }

    fn channels(&self) -> u8 {
        self.inner.channels()
    }

    fn sample(&mut self, buf: &mut [i16]) -> Result<usize, Self::Error> {
        let Self { state, inner, .. } = self;

        if let Some(state) = state.as_mut() {
            // try to fill buffer
            let mut buf_cursor = 0;

            while buf_cursor < buf.len() {
                let channels = inner.channels() as usize;
                let len = next_chunk(&mut buf[buf_cursor..], inner, state, channels)?;

                buf_cursor += len;
            }

            Ok(buf_cursor)
        } else {
            inner.sample(buf).map_err(Into::into)
        }
    }

    fn seek(&mut self, position: usize) -> Result<(), Self::Error> {
        if let Some(_state) = self.state.as_mut() {
            // reset resampler to avoid weird interpolation
            todo!()
        } else {
            self.inner.seek(position).map_err(Into::into)
        }
    }
}

fn next_chunk<T>(
    buf: &mut [i16],
    inner: &mut T,
    state: &mut ResamplerState,
    channels: usize,
) -> Result<usize, ResampleError<T::Error>>
where
    T: Source,
{
    // My hope is this code is so terrible I am never allowed to write DSP code
    // ever again.
    if state.eof {
        return Ok(0);
    }

    if state.to_buffer_rem > 0 {
        // use up contained buffer stuff before doing more processing
        return Ok(consume_buffer(buf, state));
    }

    // what is next required for the next resample?
    let requested_len = state.fft.input_frames_next();
    let mut have_len = state.from_buffer[0].len();

    while have_len < requested_len {
        // fill buffer with samples from inner
        let read_len = inner.sample(&mut state.from_buffer_itl)?;

        if read_len > 0 {
            // convert samples from itl to sep-f32
            let read_len = read_len / channels;

            deinterleave_samples(&state.from_buffer_itl, &mut state.from_buffer, read_len);

            have_len += read_len;
        } else {
            // break! we do not have enough data left in the stream
            break;
        }
    }

    let (in_len, out_len) = if have_len >= requested_len {
        // do processing
        state
            .fft
            .process_into_buffer(&state.from_buffer, &mut state.to_buffer, None)
            .map_err(ResampleError::Resample)?
    } else {
        // we are at the end!
        state.eof = true;

        state
            .fft
            .process_partial_into_buffer(Some(&state.from_buffer), &mut state.to_buffer, None)
            .map_err(ResampleError::Resample)?
    };

    // drain processed samples
    for buffer in state.from_buffer.iter_mut() {
        buffer.drain(0..in_len);
    }

    // `out_len` is in frames and we need to convert to total samples
    state.to_buffer_rem = out_len * channels;

    Ok(consume_buffer(buf, state))
}

/// Returns the frames consumed.
fn consume_buffer(buf: &mut [i16], state: &mut ResamplerState) -> usize {
    let channels = state.to_buffer.len();
    let out_len = min(state.to_buffer_rem, buf.len());
    let out_len_frames = out_len / channels;
    let out_len_rem = out_len % channels;

    interleave_samples(&state.to_buffer, buf, out_len);

    // rotate rest
    for (channel_no, buffer) in state.to_buffer.iter_mut().enumerate() {
        if channel_no >= out_len_rem {
            buffer.rotate_left(out_len_frames);
        } else {
            buffer.rotate_left(out_len_frames + 1);
        }
    }

    state.to_buffer_rem -= out_len;

    out_len
}

fn deinterleave_samples(audio_in: &[i16], audio_out: &mut [Vec<f32>], frame_count: usize) {
    let channels = audio_out.len();

    for samples in audio_in[..(frame_count * channels)].chunks(channels) {
        for (i, sample) in samples.iter().enumerate() {
            let sample = sample.to_sample::<f32>();
            audio_out[i].push(sample);
        }
    }
}

fn interleave_samples<T: AsRef<[f32]>>(audio_in: &[T], audio_out: &mut [i16], len: usize) {
    let channels = audio_in.len();

    for i in 0..len {
        let channel_no = i % channels;
        let sample_no = i / channels;

        let sample = audio_in[channel_no].as_ref()[sample_no];

        audio_out[i] = sample.to_sample::<i16>();
    }
}

/// An error that [`Resampler`] returns.
#[derive(Debug)]
pub enum ResampleError<T> {
    Source(T),
    Resample(rubato::ResampleError),
}

impl<T> From<T> for ResampleError<T> {
    fn from(source: T) -> ResampleError<T> {
        ResampleError::Source(source)
    }
}

impl<T> Display for ResampleError<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ResampleError::Source(err) => Display::fmt(err, f),
            ResampleError::Resample(err) => Display::fmt(err, f),
        }
    }
}

impl<T> std::error::Error for ResampleError<T>
where
    T: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResampleError::Source(err) => Some(err),
            ResampleError::Resample(err) => Some(err),
        }
    }
}

/// An `.ogg` decoder for an audio asset.
pub struct OggDecoder {
    buffer: Vec<i16>,
    buffer_cursor: usize,
    stream: OggStreamReader<Cursor<AudioSource>>,
}

impl OggDecoder {
    /// Create a new `OggDecoder`.
    pub fn new(source: AudioSource) -> Result<OggDecoder, lewton::VorbisError> {
        OggStreamReader::new(Cursor::new(source)).map(|stream| OggDecoder {
            stream,
            buffer: Vec::new(),
            buffer_cursor: 0,
        })
    }

    fn remaining(&self) -> usize {
        self.buffer.len() - self.buffer_cursor
    }
}

impl Source for OggDecoder {
    type Error = lewton::VorbisError;

    fn sample_rate(&self) -> u32 {
        self.stream.ident_hdr.audio_sample_rate
    }

    fn channels(&self) -> u8 {
        self.stream.ident_hdr.audio_channels
    }

    fn sample(&mut self, buf: &mut [i16]) -> Result<usize, lewton::VorbisError> {
        let mut cursor = 0;

        while cursor < buf.len() {
            if self.remaining() > 0 {
                // copy from buffer
                let remaining = self.remaining();
                let len = std::cmp::min(remaining, buf.len() - cursor);

                buf[cursor..(len + cursor)]
                    .copy_from_slice(&self.buffer[self.buffer_cursor..(self.buffer_cursor + len)]);

                // advance buffer cursor
                self.buffer_cursor += len;

                cursor += len;
            } else {
                // try to read more data
                match self.stream.read_dec_packet_itl()? {
                    Some(data) => {
                        // reinit buffer
                        self.buffer = data;
                        self.buffer_cursor = 0;
                    }
                    None => break,
                }
            }
        }

        Ok(cursor)
    }

    fn seek(&mut self, position: usize) -> Result<(), Self::Error> {
        self.stream.seek_absgp_pg(position as u64)?;

        // reset buffers
        self.buffer.clear();
        self.buffer_cursor = 0;

        Ok(())
    }
}
