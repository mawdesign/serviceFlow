//! The sidecar codec (ADR-0003): serviceFlow-only settings as TOML.
//!
//! A set's sidecar lists the settings of the song items that differ from the defaults:
//!
//! ```toml
//! version = 1
//!
//! [[item]]
//! song = "Hymns/Amazing Grace"
//! occurrence = 0          # which item for this song, counting from 0 (omitted when 0)
//! language = "primary"    # "both" (default), "primary" or "translation"
//! transpose = 2           # semitones (omitted when 0)
//! capo = 3                # omitted when the song's own capo applies
//! ```
//!
//! Reading is lenient about extra keys (a newer serviceFlow may add some) and strict about what it
//! understands: a value of the wrong type is an error, and the caller falls back to defaults.

use std::fmt;

use serde::{Deserialize, Serialize};
use serviceflow_core::ids::SongId;
use serviceflow_core::sidecar::{ItemKey, ItemSettings, SetSidecar};
use serviceflow_core::song::LanguageMode;

mod styles;
pub use styles::{read_styles_sidecar, write_styles_sidecar};

/// The version written; a file with a higher one is still read for what this version knows.
const VERSION: u32 = 1;

/// The sidecar could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarError(String);

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SidecarError {}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Language {
    Both,
    Primary,
    Translation,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default, rename = "item", skip_serializing_if = "Vec::is_empty")]
    items: Vec<Item>,
}

#[derive(Serialize, Deserialize)]
struct Item {
    song: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    occurrence: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<Language>,
    #[serde(default, skip_serializing_if = "is_zero")]
    transpose: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capo: Option<u8>,
}

fn is_zero<T: Default + PartialEq>(n: &T) -> bool {
    *n == T::default()
}

/// Read a set's sidecar.
pub fn read_set_sidecar(bytes: &[u8]) -> Result<SetSidecar, SidecarError> {
    let text = std::str::from_utf8(bytes).map_err(|e| SidecarError(format!("not UTF-8: {e}")))?;
    let file: File = toml::from_str(text).map_err(|e| SidecarError(e.message().to_owned()))?;
    let mut sidecar = SetSidecar::default();
    for item in file.items {
        let language = match item.language {
            None | Some(Language::Both) => LanguageMode::Both,
            Some(Language::Primary) => LanguageMode::Primary,
            Some(Language::Translation) => LanguageMode::Translation,
        };
        sidecar.set(
            ItemKey { song: SongId::new(item.song), occurrence: item.occurrence },
            ItemSettings { language, transpose: item.transpose, capo: item.capo },
        );
    }
    Ok(sidecar)
}

/// Write a set's sidecar. Items at their defaults are left out; writing is stable (same sidecar,
/// same bytes).
pub fn write_set_sidecar(sidecar: &SetSidecar) -> Vec<u8> {
    let items = sidecar
        .items
        .iter()
        .map(|(key, settings)| Item {
            song: key.song.0.clone(),
            occurrence: key.occurrence,
            language: match settings.language {
                LanguageMode::Both => None,
                LanguageMode::Primary => Some(Language::Primary),
                LanguageMode::Translation => Some(Language::Translation),
            },
            transpose: settings.transpose,
            capo: settings.capo,
        })
        .collect();
    let file = File { version: VERSION, items };
    // A struct of strings and numbers always serialises.
    let mut text = toml::to_string(&file).unwrap_or_else(|_| format!("version = {VERSION}\n"));
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.into_bytes()
}
