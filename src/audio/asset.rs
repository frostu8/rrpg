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
