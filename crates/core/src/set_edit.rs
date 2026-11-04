//! Set edits (ADR-0001): what the operator changes in the open set, and the set's undo history.
//!
//! Edits only change the set in memory; saving is a separate step (ADR-0002). Each edit is a
//! [`SetEdit`] sent as [`Command::EditSet`](crate::command::Command); undo and redo walk the
//! open set's own history, which live actions never enter.

use std::sync::Arc;

use crate::ids::SongId;
use crate::search::{category, filename};
use crate::set::{CustomItem, ImageItem, ImageSlide, ItemKind, Set, SetItem, SongItem, StyleAction};
use crate::style::{ImageSource, Style};

/// A text of a set item the operator can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemField {
    /// The caption of a custom, image or style item (a song item's name is its song's filename).
    Name,
    /// A song item's presentation-order override, as typed: section tokens separated by spaces.
    /// Empty: the song's own order.
    Presentation,
    /// The item notes (GLOSSARY "Item notes"), for any kind of item.
    Notes,
    /// The title of a custom or image item.
    Title,
    /// The subtitle of a custom or image item.
    Subtitle,
    /// The slides of a custom item, as typed: one slide after another, separated by lines holding
    /// only `---` (OpenSong's custom slide editor).
    Slides,
}

/// One change to the open set. `at` is where a new item goes: `None` puts it after the selected
/// item, or at the end when nothing is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetEdit {
    /// Add a song item for a song of the library.
    AddSong { song: SongId, at: Option<usize> },
    /// Add a custom slide item called `name` with one empty slide.
    AddCustom { name: String, at: Option<usize> },
    /// Add an image item called `name` showing these files of `Backgrounds/`, one per slide.
    AddImages { name: String, files: Vec<String>, at: Option<usize> },
    /// Add a style item. `New` pushes a copy of the default style (as OpenSong does); `Revert`
    /// goes back to the style before the last push.
    AddStyle { name: String, action: StyleAction, at: Option<usize> },
    /// Move item `from` so it ends up at position `to`.
    Move { from: usize, to: usize },
    /// Remove an item.
    Remove(usize),
    /// Set a text of item `item`. Ignored for a field the item's kind does not have.
    Field { item: usize, field: ItemField, text: String },
}

/// What an edit did to the list of items, to keep the selection on the same item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    Inserted(usize),
    Removed(usize),
    Moved { from: usize, to: usize },
    /// The items stayed where they were.
    InPlace,
}

impl Change {
    /// Where the item that was at `index` is now (`None`: it was removed).
    pub(crate) fn remap(self, index: usize) -> Option<usize> {
        match self {
            Change::Inserted(at) if index >= at => Some(index + 1),
            Change::Removed(at) if index == at => None,
            Change::Removed(at) if index > at => Some(index - 1),
            Change::Moved { from, to } if index == from => Some(to),
            Change::Moved { from, to } => {
                let after_removal = if index > from { index - 1 } else { index };
                Some(if after_removal >= to { after_removal + 1 } else { after_removal })
            }
            _ => Some(index),
        }
    }
}

/// What edits of the same thing join into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    pub item: usize,
    pub field: ItemField,
}

impl SetEdit {
    /// Edits of the same text of the same item, one after another, are one undo step.
    pub(crate) fn target(&self) -> Option<Target> {
        match self {
            SetEdit::Field { item, field, .. } => Some(Target { item: *item, field: *field }),
            _ => None,
        }
    }

    /// The set with the edit applied, and what happened to the items; `None` when the edit does
    /// not apply (no such item, or a field this kind of item does not have). `selected` decides
    /// where an item goes without an `at`; `default_style` is what a new push style item copies.
    pub(crate) fn applied(&self, set: &Set, selected: Option<usize>, default_style: &Style) -> Option<(Set, Change)> {
        let mut set = set.clone();
        let len = set.items.len();
        let place = |at: Option<usize>| at.map_or_else(|| selected.map_or(len, |i| (i + 1).min(len)), |at| at.min(len));
        let mut insert = |item: SetItem, at: Option<usize>| {
            let at = place(at);
            set.items.insert(at, item);
            Change::Inserted(at)
        };
        let new = |name: &str, kind| SetItem { name: name.to_owned(), notes: String::new(), style: None, kind };
        let change = match self {
            SetEdit::AddSong { song, at } => {
                let item = SongItem { path: category(song).to_owned(), ..SongItem::default() };
                insert(new(filename(song), ItemKind::Song(item)), *at)
            }
            SetEdit::AddCustom { name, at } => {
                insert(new(name, ItemKind::Custom(CustomItem { slides: vec![String::new()], ..CustomItem::default() })), *at)
            }
            SetEdit::AddImages { name, files, at } => {
                let slides = files.iter().map(|f| ImageSlide { image: Some(ImageSource::File(f.clone())), description: String::new() }).collect();
                insert(new(name, ItemKind::Image(ImageItem { slides, ..ImageItem::default() })), *at)
            }
            SetEdit::AddStyle { name, action, at } => {
                let style = (*action == StyleAction::New).then(|| Arc::new(default_style.clone()));
                insert(SetItem { style, ..new(name, ItemKind::Style { action: *action, xml: String::new() }) }, *at)
            }
            SetEdit::Move { from, to } => {
                if *from >= len {
                    return None;
                }
                let to = (*to).min(len - 1);
                let item = set.items.remove(*from);
                set.items.insert(to, item);
                Change::Moved { from: *from, to }
            }
            SetEdit::Remove(index) => {
                if *index >= len {
                    return None;
                }
                set.items.remove(*index);
                Change::Removed(*index)
            }
            SetEdit::Field { item, field, text } => {
                let item = set.items.get_mut(*item)?;
                set_field(item, *field, text)?;
                Change::InPlace
            }
        };
        Some((set, change))
    }
}

/// Set a text of an item; `None` when its kind has no such text.
fn set_field(item: &mut SetItem, field: ItemField, text: &str) -> Option<()> {
    match (field, &mut item.kind) {
        (ItemField::Notes, _) => item.notes = text.to_owned(),
        (ItemField::Name, ItemKind::Custom(_) | ItemKind::Image(_) | ItemKind::Style { .. }) => item.name = text.to_owned(),
        (ItemField::Presentation, ItemKind::Song(song)) => song.presentation = text.split_ascii_whitespace().map(str::to_owned).collect(),
        (ItemField::Title, ItemKind::Custom(CustomItem { title, .. }) | ItemKind::Image(ImageItem { title, .. })) => *title = text.to_owned(),
        (ItemField::Subtitle, ItemKind::Custom(CustomItem { subtitle, .. }) | ItemKind::Image(ImageItem { subtitle, .. })) => {
            *subtitle = text.to_owned()
        }
        (ItemField::Slides, ItemKind::Custom(custom)) => custom.slides = CustomItem::slides_from_text(text),
        _ => return None,
    }
    Some(())
}

/// The text of an item's field as the editor shows it (`None`: the kind has no such text).
pub fn field_text(item: &SetItem, field: ItemField) -> Option<String> {
    Some(match (field, &item.kind) {
        (ItemField::Notes, _) => item.notes.clone(),
        (ItemField::Name, ItemKind::Custom(_) | ItemKind::Image(_) | ItemKind::Style { .. }) => item.name.clone(),
        (ItemField::Presentation, ItemKind::Song(song)) => song.presentation.join(" "),
        (ItemField::Title, ItemKind::Custom(CustomItem { title, .. }) | ItemKind::Image(ImageItem { title, .. })) => title.clone(),
        (ItemField::Subtitle, ItemKind::Custom(CustomItem { subtitle, .. }) | ItemKind::Image(ImageItem { subtitle, .. })) => subtitle.clone(),
        (ItemField::Slides, ItemKind::Custom(custom)) => custom.slides_text(),
        _ => return None,
    })
}

/// The open set's undo history: earlier versions of the set, newest last.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetHistory {
    pub(crate) undo: Vec<Arc<Set>>,
    pub(crate) redo: Vec<Arc<Set>>,
    /// The text the last edit changed, while edits to it are still joined into one step.
    pub(crate) group: Option<Target>,
}

impl SetHistory {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
