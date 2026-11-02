//! Application state. The engine publishes it as an immutable snapshot (`Arc<State>`); the UI
//! reads a snapshot each frame and never mutates it.

use std::sync::Arc;

use crate::ids::{ContentHash, SongId};
use crate::song::{Slide, Song};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// The song whose slides are shown.
    pub song: Option<OpenSong>,
    /// A song that has been asked for and is still loading.
    pub loading: Option<SongId>,
    /// What the outputs show.
    pub output: Output,
    /// Problems to show the operator, oldest first.
    pub problems: Vec<Problem>,
}

/// Something that went wrong, for the operator. Structured so the UI words it (through i18n).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A song could not be read or parsed. `error` is the technical detail.
    SongUnreadable { id: SongId, error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSong {
    pub id: SongId,
    pub song: Arc<Song>,
    /// Hash of the file bytes this song was parsed from, recorded on read (conflict detection).
    pub hash: ContentHash,
    /// The song's slides in presentation order.
    pub slides: Vec<Slide>,
}

/// What the outputs show (GLOSSARY "Output", "Live"). Live actions change only this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// The live slide, if any slide has been sent live.
    pub live: Option<LiveSlide>,
    pub mode: OutputMode,
}

/// A slide that is live. It keeps its own copy, so opening another song does not change it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSlide {
    /// The song it came from.
    pub song: SongId,
    /// Its position in that song's slides.
    pub index: usize,
    pub slide: Slide,
}

/// How the outputs show the live slide (GLOSSARY "Blank", "Black").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// The live slide.
    #[default]
    Normal,
    /// The style's background with no text.
    Blank,
    /// Pure black.
    Black,
}
