//! Application state. The engine publishes it as an immutable snapshot (`Arc<State>`); the UI
//! reads a snapshot each frame and never mutates it.

use std::sync::Arc;

use crate::ids::SongId;
use crate::song::{Slide, Song};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// The song whose slides are shown.
    pub song: Option<OpenSong>,
    /// A song that has been asked for and is still loading.
    pub loading: Option<SongId>,
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
    /// The song's slides in presentation order.
    pub slides: Vec<Slide>,
}
