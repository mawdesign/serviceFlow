//! Sets: the running order of a service (GLOSSARY "Set").

use std::sync::Arc;

use crate::ids::SongId;
use crate::song::RawElement;
use crate::style::Style;

/// A set as read from `Sets/`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Set {
    /// The `name` attribute of the file's root, `""` when absent (OpenSong's sample set has none).
    pub name: String,
    pub items: Vec<SetItem>,
}

/// One entry of a set (GLOSSARY "Set item").
#[derive(Debug, Clone, PartialEq)]
pub struct SetItem {
    /// Caption: for a song the song filename (not its title), otherwise the item's name.
    pub name: String,
    /// Operator-only notes; never projected.
    pub notes: String,
    /// The item's own `<style>` (GLOSSARY "Set item style"), parsed for display; the XML is kept
    /// in the kind (`other_elements` / `xml`) for saving. A style item's style is the one it
    /// pushes.
    pub style: Option<Arc<Style>>,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Song(SongItem),
    /// A style item: changes the style of the items after it (GLOSSARY "Style item"). `xml` is
    /// the whole `slide_group`, so a save can write it back unchanged.
    Style {
        action: StyleAction,
        xml: String,
    },
    /// Anything serviceFlow keeps but cannot present yet (scripture, external, and the custom,
    /// image and style items that later tickets add). `xml` is the whole `slide_group`, so a
    /// later save writes it back unchanged.
    Placeholder {
        item_type: String,
        xml: String,
    },
}

/// What a style item does (`slide_group/@action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleAction {
    /// Push the item's style on the style stack (`new`).
    New,
    /// Go back to the style before the last push (`revert`).
    Revert,
    /// Any other action: OpenSong pushes only if the item has a style.
    Other,
}

/// A song item (GLOSSARY "Song item").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongItem {
    /// Folder under `Songs/` without leading or trailing `/`; `""` for the root.
    pub path: String,
    /// Presentation order override as section tokens. Empty: use the song's own order.
    pub presentation: Vec<String>,
    /// Item children not modelled here (such as a set-item `<style>`), kept for saving.
    pub other_elements: Vec<RawElement>,
}

impl SetItem {
    /// The song a song item refers to.
    pub fn song_id(&self) -> Option<SongId> {
        match &self.kind {
            ItemKind::Song(song) => Some(song.id(&self.name)),
            ItemKind::Style { .. } | ItemKind::Placeholder { .. } => None,
        }
    }
}

impl SongItem {
    /// The song id for a song named `name` (its filename) in this item's folder.
    pub fn id(&self, name: &str) -> SongId {
        if self.path.is_empty() { SongId::new(name) } else { SongId::new(format!("{}/{name}", self.path)) }
    }
}
