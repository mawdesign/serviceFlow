//! serviceFlow-only settings for the items of a set (ADR-0003, GLOSSARY "Sidecar"). They are
//! kept beside the set, never in OpenSong's files.

use std::collections::BTreeMap;

use crate::ids::SongId;
use crate::set::Set;
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

/// The sidecar key of item `index` of `set` (`None` for any item that is not a song item).
pub fn item_key(set: &Set, index: usize) -> Option<ItemKey> {
    let song = set.items.get(index)?.song_id()?;
    let occurrence = set.items[..index].iter().filter(|item| item.song_id().as_ref() == Some(&song)).count();
    Some(ItemKey { song, occurrence })
}

impl SetSidecar {
    /// The sidecar of `set` whose item `i` has `settings(i)`: each song item's settings under its
    /// key (items at the defaults have no entry).
    pub fn of(set: &Set, settings: impl Fn(usize) -> ItemSettings) -> Self {
        let mut sidecar = Self::default();
        for index in 0..set.items.len() {
            if let Some(key) = item_key(set, index) {
                sidecar.set(key, settings(index));
            }
        }
        sidecar
    }

    /// This sidecar of set `before` for set `after`, whose items are the same items in the same
    /// places with (possibly) other songs: each item keeps its settings under its new key.
    /// Entries for no item of `before` are dropped.
    pub fn rekeyed(&self, before: &Set, after: &Set) -> Self {
        Self::of(after, |index| item_key(before, index).map(|key| self.get(&key)).unwrap_or_default())
    }

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
