//! Application state. The engine publishes it as an immutable snapshot (`Arc<State>`); the UI
//! reads a snapshot each frame and never mutates it.

use std::sync::Arc;

use crate::ids::{ContentHash, SongId};
use crate::song::{Slide, SlideOptions, Song};

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
    /// How lyrics become slides, for every song opened.
    pub slide_options: SlideOptions,
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
    /// The slides of the control window's grid: the first [`in_flow`](Self::in_flow) follow the
    /// presentation order; the rest come from sections the order leaves out (shown greyed,
    /// skipped by next/previous, but can be sent live by hand; ADR-0007).
    pub slides: Vec<Slide>,
    /// How many of [`slides`](Self::slides) are in the flow.
    pub in_flow: usize,
}

impl OpenSong {
    /// Build the grid for `song` with these options.
    pub fn new(id: SongId, song: Arc<Song>, hash: ContentHash, options: &SlideOptions) -> Self {
        let mut slides = song.slides(options);
        let in_flow = slides.len();
        slides.extend(song.unused_slides(options));
        Self { id, song, hash, slides, in_flow }
    }

    /// Whether slide `index` follows the presentation order.
    pub fn is_in_flow(&self, index: usize) -> bool {
        index < self.in_flow
    }
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
