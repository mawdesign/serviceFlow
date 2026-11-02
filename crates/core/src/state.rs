//! Application state. The engine publishes it as an immutable snapshot (`Arc<State>`); the UI
//! reads a snapshot each frame and never mutates it.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::ids::{ContentHash, SongId};
use crate::search::SearchIndex;
use crate::song::{Slide, Song};
use crate::style::Styles;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The song whose slides are shown.
    pub song: Option<OpenSong>,
    /// A song that has been asked for and is still loading.
    pub loading: Option<SongId>,
    /// The library: songs, search index and search results.
    pub library: Library,
    /// What the outputs show.
    pub output: Output,
    /// The styles slides are drawn with: imported from OpenSong's PresentSettings on first run
    /// and again on request; serviceFlow's own copy, never written back to OpenSong.
    pub styles: Arc<Styles>,
    /// Problems to show the operator, oldest first.
    pub problems: Vec<Problem>,
}

/// Something that went wrong, for the operator. Structured so the UI words it (through i18n).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A song could not be read or parsed. `error` is the technical detail.
    SongUnreadable { id: SongId, error: String },
    /// The library could not be listed. `error` is the technical detail.
    LibraryUnreadable { error: String },
    /// OpenSong's PresentSettings could not be read when the operator asked for a re-import.
    StylesUnreadable { error: String },
    /// A style names a font that is not installed, so slides use `used` instead (which may wrap
    /// text differently if it does not have the same measurements).
    FontSubstituted { requested: String, used: String },
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
    /// The song, for the title and subtitle boxes.
    pub source: Arc<Song>,
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

/// The library (GLOSSARY "Library") and the search over it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    /// Every song, sorted by id (so by category, then filename).
    pub songs: Vec<SongId>,
    pub index: Arc<SearchIndex>,
    /// `true` while songs are still being read for the index.
    pub indexing: bool,
    /// Songs read so far.
    pub indexed: usize,
    /// The current query, as typed.
    pub query: String,
    /// Songs matching `query`, best first. Empty when the query is blank.
    pub results: Vec<SongId>,
    /// Which refresh the in-flight results belong to; older results are dropped.
    pub generation: u64,
    pub(crate) queue: VecDeque<SongId>,
}

impl Library {
    /// The title to show for a song (its filename until the song has been read).
    pub fn title<'a>(&'a self, id: &'a SongId) -> &'a str {
        self.index.title(id)
    }
}
