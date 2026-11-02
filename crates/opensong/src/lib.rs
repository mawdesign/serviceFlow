//! OpenSong codec for songs, sets and settings. Reads leniently and writes canonically
//! (ADR-0002).

mod settings;
mod set;
mod song;
mod xml;

pub use set::read_set;
pub use settings::read_present_settings;
pub use song::read_song;

/// Why bytes could not be read as an OpenSong document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError(pub String);

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CodecError {}
