//! serviceFlow-only settings for the items of a set (ADR-0003, GLOSSARY "Sidecar"). They are
//! kept beside the set, never in OpenSong's files.

use std::collections::BTreeMap;

use crate::ids::SongId;
use crate::song::LanguageMode;

/// Which song item of a set: its song and how many items for the same song come before it. A
/// song used twice in one set has two keys, and neither depends on the item's position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemKey {
    pub song: SongId,
    pub occurrence: usize,
}

/// The serviceFlow-only settings of one song item. `Default` is what an item without a stored
/// entry has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemSettings {
    /// Which lines of multilanguage sections show (restored when the item is selected).
    pub language: LanguageMode,
    /// Semitones the song is transposed up (negative: down) for this item only.
    pub transpose: i32,
    /// A capo fret for this item only (`None`: the song's own capo).
    pub capo: Option<u8>,
}

/// The sidecar of one set: settings for the items that differ from the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetSidecar {
    pub items: BTreeMap<ItemKey, ItemSettings>,
}

impl SetSidecar {
    /// The settings of an item (the default when there is no entry).
    pub fn get(&self, key: &ItemKey) -> ItemSettings {
        self.items.get(key).copied().unwrap_or_default()
    }

    /// Change an item's settings. An item back at its defaults has no entry.
    pub fn set(&mut self, key: ItemKey, settings: ItemSettings) {
        if settings == ItemSettings::default() {
            self.items.remove(&key);
        } else {
            self.items.insert(key, settings);
        }
    }
}
