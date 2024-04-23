//! Rhythm and beatmap assets.

use bevy::asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext};
use bevy::prelude::*;
use bevy::utils::BoxedFuture;

use serde::{Deserialize, Serialize};

use std::cmp::Ordering;
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

            // sort notes
            data.notes
                .sort_unstable_by(|a, b| a.partial_cmp(b).expect("got NaN as beat for note"));

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
    /// Lane count.
    ///
    /// This is used to initialize the lanes without having to scan through
    /// the entire ron.
    pub lane_count: u32,
    /// Song definitions.
    pub song: BeatmapSong,
    notes: Vec<BeatmapNote>,
}

impl Beatmap {
    /// The actual beatmap, a-la where all the notes are placed.
    ///
    /// The notes returned are sorted by the beat they start on. To modify
    /// notes, one must use the [`Beatmap::change`] function.
    pub fn notes(&self) -> &[BeatmapNote] {
        &self.notes
    }
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

/// A single placement of a note.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BeatmapNote {
    beat: f32,
    #[serde(default)]
    end_beat: Option<f32>,
    /// What lane the note appears in.
    pub lane: u32,
}

impl BeatmapNote {
    /// Where the note actually occurs in the song according to BPM.
    ///
    /// # Warning!
    /// It is an invariant for this to be `NaN`. There's nothing really that
    /// enforces that, but bad times will happen if this is `NaN`. Do not set
    /// this as `NaN` in your beatmaps and **REALLY** do not set this
    /// programmatically or everything will be terrible.
    ///
    /// This field and [`BeatmapNote::end_beat`] are hidden to preserve the
    /// invariants of [`Beatmap::notes`] (everything is sorted), so it's
    /// really hard to mess with this programmatically. The only way to get a
    /// `NaN` in here is through editing the beatmap file, but the loader
    /// thread will just crash and nothing catastrophic will happen.
    pub fn beat(&self) -> f32 {
        self.beat
    }

    /// `None` if the note is a single (tap) note. If the note is a slider,
    /// this will be `Some(x)` where `x` is the end beat.
    pub fn end_beat(&self) -> Option<f32> {
        self.end_beat
    }
}

impl PartialEq for BeatmapNote {
    fn eq(&self, other: &Self) -> bool {
        self.beat.eq(&other.beat)
    }
}

impl PartialOrd for BeatmapNote {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.beat.partial_cmp(&other.beat)
    }
}
