//! Sets: the running order of a service (GLOSSARY "Set").

use std::sync::Arc;

use crate::ids::SongId;
use crate::song::RawElement;
use crate::style::{ImageSource, Resize, Style};

/// A set as read from `Sets/`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Set {
    /// The `name` attribute of the file's root, `""` when absent (OpenSong's sample set has none).
    pub name: String,
    pub items: Vec<SetItem>,
    /// Attributes of the root other than `name`, kept for saving.
    pub attributes: Vec<(String, String)>,
    /// Children of the root other than `slide_groups`, kept for saving.
    pub other_elements: Vec<RawElement>,
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
    /// A custom slide item: a title, a subtitle and slides of text (GLOSSARY "Custom slide item").
    Custom(CustomItem),
    /// An image item: pictures shown one per slide (GLOSSARY "Image item").
    Image(ImageItem),
    /// Anything serviceFlow keeps but cannot present yet (scripture and external items). `xml` is the whole `slide_group`, so a
    /// later save writes it back unchanged.
    Placeholder {
        item_type: String,
        xml: String,
    },
}

/// A custom slide item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomItem {
    pub title: String,
    pub subtitle: String,
    /// The text of each slide (`slides/slide/body`), lines separated by `\n`.
    pub slides: Vec<String>,
    /// The whole `slide_group` as read. It keeps the attributes serviceFlow does not model
    /// (`seconds`, `loop`, `print`, `transition`); a writer starts from it.
    pub xml: String,
}

/// An image item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageItem {
    pub title: String,
    pub subtitle: String,
    /// What area the pictures are fitted to (`resize`: absent or `screen` = screen, `body` =
    /// the text area, empty = natural size).
    pub resize: Resize,
    /// Keep the pictures' proportions (`keep_aspect`).
    pub keep_aspect: bool,
    pub slides: Vec<ImageSlide>,
    /// The whole `slide_group` as read, as for [`CustomItem::xml`].
    pub xml: String,
}

/// One picture of an image item.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSlide {
    /// A file under `Backgrounds/`, or the picture embedded as base64. `None` when the slide
    /// names neither.
    pub image: Option<ImageSource>,
    pub description: String,
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
    /// Item attributes not modelled here (such as `transition`), kept for saving.
    pub attributes: Vec<(String, String)>,
}

/// The line that separates slides in the custom slide editor, as in OpenSong's.
pub const SLIDE_SEPARATOR: &str = "---";

impl CustomItem {
    /// The slides as one text, separated by [`SLIDE_SEPARATOR`] lines.
    pub fn slides_text(&self) -> String {
        self.slides.join(&format!("\n{SLIDE_SEPARATOR}\n"))
    }

    /// The slides of a text written as [`slides_text`](Self::slides_text) gives it: split at
    /// every line holding only `---` (spaces around it are allowed). Nothing else changes.
    pub fn slides_from_text(text: &str) -> Vec<String> {
        let text = text.replace("\r\n", "\n");
        let mut slides = vec![Vec::new()];
        for line in text.split('\n') {
            if line.trim() == SLIDE_SEPARATOR {
                slides.push(Vec::new());
            } else if let Some(slide) = slides.last_mut() {
                slide.push(line);
            }
        }
        slides.into_iter().map(|lines| lines.join("\n")).collect()
    }
}

impl SetItem {
    /// The song a song item refers to.
    pub fn song_id(&self) -> Option<SongId> {
        match &self.kind {
            ItemKind::Song(song) => Some(song.id(&self.name)),
            ItemKind::Style { .. } | ItemKind::Placeholder { .. } | ItemKind::Custom(_) | ItemKind::Image(_) => None,
        }
    }

    /// The id the open item's slides are shown under: the song's id, or [`SongId::set_item`]
    /// for a custom or image item at `index` of its set. `None` for items without slides.
    pub fn open_id(&self, index: usize) -> Option<SongId> {
        match &self.kind {
            ItemKind::Song(song) => Some(song.id(&self.name)),
            ItemKind::Custom(_) | ItemKind::Image(_) => Some(SongId::set_item(index)),
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
