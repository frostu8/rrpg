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
/// Samples from decoders are 44100 sample rate, 2 channels interleaved. This
/// is meant to be run on the mixer thread and is thus not exposed.
pub struct Decoder {
    buffer: Vec<i16>,
    buffer_cursor: usize,
    stream: OggStreamReader<Cursor<AudioSource>>,
}

impl Decoder {
    /// Create a new `Decoder`.
    pub fn new(source: AudioSource) -> Result<Decoder, lewton::VorbisError> {
        OggStreamReader::new(Cursor::new(source)).map(|stream| Decoder {
            stream,
            buffer: Vec::new(),
            buffer_cursor: 0,
        })
    }

    /// Reads until the buffer is full or no more samples can be fetched.
    ///
    /// The `usize` returned is how many samples were read, or `0` if EOF was
    /// reached.
    pub fn sample(&mut self, buf: &mut [i16]) -> Result<usize, lewton::VorbisError> {
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

    fn remaining(&self) -> usize {
        self.buffer.len() - self.buffer_cursor
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
