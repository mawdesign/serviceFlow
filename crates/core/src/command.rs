//! Commands and effects (ADR-0001).
//!
//! A [`Command`] is the only way to change state. The UI, the file watcher, storage results and
//! future automation all send the same commands to the [`Engine`](crate::engine::Engine).
//! Applying a command may ask for IO by returning [`Effect`]s; the runtime performs them and
//! sends the outcome back as another command, so the engine itself never waits.

use std::sync::Arc;

use crate::ids::SongId;
use crate::song::Song;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open a song from the library and show its slides.
    OpenSong(SongId),
    /// Storage result: a song requested by [`Effect::LoadSong`] was read and parsed.
    SongLoaded { id: SongId, song: Arc<Song> },
    /// Storage result: a song requested by [`Effect::LoadSong`] could not be read or parsed.
    SongLoadFailed { id: SongId, error: String },
}

/// IO the engine asks for. Each effect ends with exactly one result command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Read and parse a song; answer with [`Command::SongLoaded`] or [`Command::SongLoadFailed`].
    LoadSong(SongId),
}
