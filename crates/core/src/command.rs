//! Commands and effects (ADR-0001).
//!
//! A [`Command`] is the only way to change state. The UI, the file watcher, storage results and
//! future automation all send the same commands to the [`Engine`](crate::engine::Engine).
//! Applying a command may ask for IO by returning [`Effect`]s; the runtime performs them and
//! sends the outcome back as another command, so the engine itself never waits.

use std::sync::Arc;

use crate::ids::{ContentHash, SongId};
use crate::song::{SlideOptions, Song};
use crate::style::Styles;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Open a song from the library and show its slides.
    OpenSong(SongId),
    /// Storage result: a song requested by [`Effect::LoadSong`] was read and parsed. `hash` is the hash of the
    /// bytes read, kept so a later save can detect that the file changed meanwhile.
    SongLoaded { id: SongId, song: Arc<Song>, hash: ContentHash },
    /// Storage result: a song requested by [`Effect::LoadSong`] could not be read or parsed.
    SongLoadFailed { id: SongId, error: String },
    /// Read the list of songs and index them in the background (also after the library changed).
    RefreshLibrary,
    /// Storage result: the songs in the library. `generation` identifies the refresh that asked.
    LibraryListed { generation: u64, songs: Vec<SongId> },
    /// Storage result: the library could not be listed.
    LibraryListFailed { generation: u64, error: String },
    /// Storage result: a chunk of songs was read for the index; `None` = unreadable (stays
    /// searchable by filename).
    SongsIndexed { generation: u64, songs: Vec<(SongId, Option<Arc<Song>>)> },
    /// Set the library search query. A blank query clears the results.
    Search(String),
    /// Send slide `index` of the open song live. Ignored when there is no such slide.
    GoLive { index: usize },
    /// Go live with the slide after the live one (the first slide when nothing is live). Stays
    /// put on the last slide.
    NextSlide,
    /// Go live with the slide before the live one. Does nothing on the first slide or when
    /// nothing is live.
    PreviousSlide,
    /// Toggle blank mode (the style's background, no text).
    ToggleBlank,
    /// Toggle black mode (pure black).
    ToggleBlack,
    /// Read the default and scripture styles from OpenSong's PresentSettings (read-only).
    /// Sent on first run and by the "re-import from OpenSong" action.
    ImportOpenSongStyles { trigger: ImportTrigger },
    /// Storage result: PresentSettings was read and parsed.
    StylesImported(Arc<Styles>),
    /// Storage result: PresentSettings could not be read. `missing` is true when the file does
    /// not exist.
    StylesImportFailed { trigger: ImportTrigger, error: String, missing: bool },
    /// The renderer drew `used` instead of the style's font `requested`. Becomes a
    /// [`Problem::FontSubstituted`](crate::state::Problem) the operator sees; repeats are ignored.
    FontSubstituted { requested: String, used: String },
    /// Change how lyrics become slides (such as OpenSong's `blank_is_slide_change`). The open
    /// song's slides are rebuilt; what is live stays live.
    SetSlideOptions(SlideOptions),
}

/// Why styles are being imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportTrigger {
    /// Automatically at start. A data root without PresentSettings is not a problem.
    FirstRun,
    /// The operator asked. Any failure is reported.
    Manual,
}

impl Command {
    /// Live actions are transient: they never enter undo history (ADR-0001). Only document edits
    /// do, and none exist yet.
    pub fn is_live_action(&self) -> bool {
        matches!(self, Command::GoLive { .. } | Command::NextSlide | Command::PreviousSlide | Command::ToggleBlank | Command::ToggleBlack)
    }
}

/// IO the engine asks for. Each effect ends with exactly one result command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Read and parse `Settings/PresentSettings`; answer with [`Command::StylesImported`] or
    /// [`Command::StylesImportFailed`]. Never writes.
    ReadOpenSongStyles(ImportTrigger),
    /// Read and parse a song; answer with [`Command::SongLoaded`] or [`Command::SongLoadFailed`].
    LoadSong(SongId),
    /// List the library; answer with [`Command::LibraryListed`] or [`Command::LibraryListFailed`].
    ListLibrary { generation: u64 },
    /// Read these songs for the search index; answer with [`Command::SongsIndexed`].
    IndexSongs { generation: u64, ids: Vec<SongId> },
}
