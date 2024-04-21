//! Audio assets and asset loading.

use bevy::asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext};
use bevy::prelude::*;
use bevy::utils::BoxedFuture;

use std::io::Cursor;
use std::sync::Arc;

use lewton::header::{CommentHeader, IdentHeader, SetupHeader};
use lewton::inside_ogg::OggStreamReader;

/// Loads files as [`AudioSource`] [`Assets`](bevy::asset::Assets).
#[derive(Default)]
pub struct AudioLoader;

impl AssetLoader for AudioLoader {
    type Asset = AudioSource;
    type Settings = ();
    type Error = std::io::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<AudioSource, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            Ok(AudioSource {
                bytes: bytes.into(),
            })
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ogg"]
    }
}

/// A single audio asset.
///
/// This is in the OGG format.
#[derive(Asset, Debug, Clone, TypePath)]
pub struct AudioSource {
    pub bytes: Arc<[u8]>,
}

impl AsRef<[u8]> for AudioSource {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..]
    }
}

/// A decoder for an audio asset.
///
/// Samples from decoders are 44100 sample rate, 2 channels interleaved.
pub struct Decoder {
    buffer: Vec<i16>,
    stream: OggStreamReader<Cursor<AudioSource>>,
}

impl Decoder {
    /// Create a new `Decoder`.
    pub fn new(source: AudioSource) -> Result<Decoder, lewton::VorbisError> {
        OggStreamReader::new(Cursor::new(source)).map(|stream| Decoder {
            stream,
            buffer: Vec::new(),
        })
    }

    /// Reads until the buffer is full or no more samples can be fetched.
    ///
    /// The `usize` returned is how many samples were read, or `0` if EOF was
    /// reached.
    pub fn sample_all(&mut self, buf: &mut [i16]) -> Result<usize, lewton::VorbisError> {
        let mut cursor = 0;

        while cursor < buf.len() {
            match self.sample(&mut buf[cursor..]) {
                Ok(0) => break,
                Ok(len) => cursor += len,
                Err(err) => return Err(err),
            }
        }

        Ok(cursor)
    }

    /// Reads in some samples.
    pub fn sample(&mut self, buf: &mut [i16]) -> Result<usize, lewton::VorbisError> {
        if self.buffer.len() > 0 {
            // copy from buffer first
            let len = std::cmp::min(self.buffer.len(), buf.len());

            (&mut buf[..len]).copy_from_slice(&self.buffer[..len]);

            // remove older data
            let mut new_buf = (0..(self.buffer.len() - len))
                .map(|_| 0)
                .collect::<Vec<i16>>();
            (&mut new_buf[..]).copy_from_slice(&self.buffer[len..]);
            self.buffer = new_buf;

            return Ok(len);
        }

        // get next packet from stream
        match self.stream.read_dec_packet_itl() {
            Ok(Some(data)) => {
                // we got some data, write it to the buffer
                if data.len() > buf.len() {
                    // write everything up to data
                    buf.copy_from_slice(&data[..buf.len()]);
                    // copy rest into buffer
                    self.buffer = Vec::from(&data[buf.len()..]);
                    Ok(buf.len())
                } else {
                    // copy everything
                    (&mut buf[..data.len()]).copy_from_slice(&data);
                    Ok(data.len())
                }
            }
            Ok(None) => Ok(0),
            Err(err) => Err(err),
        }
    }

    /// Returns header information.
    pub fn headers(&self) -> (&IdentHeader, &CommentHeader, &SetupHeader) {
        (
            &self.stream.ident_hdr,
            &self.stream.comment_hdr,
            &self.stream.setup_hdr,
        )
    }
}
