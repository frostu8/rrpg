//! Rhythm and beatmap assets.

use bevy::asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext};
use bevy::prelude::*;
use bevy::utils::BoxedFuture;

use serde::{Deserialize, Serialize};

use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::time::Duration;

use crate::audio::AudioSource;

/// An asset loader for beatmaps.
#[derive(Default)]
pub struct BeatmapLoader;

impl AssetLoader for BeatmapLoader {
    type Asset = Beatmap;
    type Settings = ();
    type Error = BeatmapLoadError;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Beatmap, Self::Error>> {
        Box::pin(async move {
            let mut contents = String::new();
            reader.read_to_string(&mut contents).await?;

            // deserialize data
            let mut data = ron::from_str::<Beatmap>(&contents)?;

            // load song
            let handle = load_context.load::<AudioSource>(data.song.path.clone());
            data.song.handle = handle;

            Ok(data)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

/// An error from beatmap loading.
#[derive(Debug)]
pub enum BeatmapLoadError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
}

impl From<std::io::Error> for BeatmapLoadError {
    fn from(value: std::io::Error) -> Self {
        BeatmapLoadError::Io(value)
    }
}

impl From<ron::error::SpannedError> for BeatmapLoadError {
    fn from(value: ron::error::SpannedError) -> Self {
        BeatmapLoadError::Ron(value)
    }
}

impl Display for BeatmapLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BeatmapLoadError::Io(io) => Display::fmt(io, f),
            BeatmapLoadError::Ron(ron) => Display::fmt(ron, f),
        }
    }
}

impl std::error::Error for BeatmapLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BeatmapLoadError::Io(e) => Some(e),
            BeatmapLoadError::Ron(e) => Some(e),
        }
    }
}

/// A beatmap asset.
#[derive(Asset, Clone, Debug, Default, Deserialize, Serialize, TypePath)]
pub struct Beatmap {
    /// Song definitions.
    pub song: BeatmapSong,
}

/// A beatmap's song definition.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BeatmapSong {
    /// The path to the song, relative to the beatmap's package.
    pub path: PathBuf,
    /// A handle to the song.
    #[serde(skip)]
    pub handle: Handle<AudioSource>,
    /// The BPM of the song.
    pub bpm: u32,
    /// The offset of where the song actually starts, in milliseconds.
    pub offset: u32,
}

impl BeatmapSong {
    /// Returns `offset` as a [`Duration`].
    pub fn offset(&self) -> Duration {
        Duration::from_millis(self.offset.into())
    }
}
