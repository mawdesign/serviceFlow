//! Song edits (ADR-0001): what the song editor changes, and the per-document undo history.
//!
//! Edits only change the song in memory. Saving is a separate step (ADR-0002); until then the
//! edited song lives in the state as a [`Draft`] that undo and redo walk through.

use std::sync::Arc;

use crate::ids::ContentHash;
use crate::lyrics;
use crate::song::Song;

/// A text field of a song's details (the metadata form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SongField {
    Title,
    Author,
    Copyright,
    Ccli,
    HymnNumber,
    Key,
    Capo,
    Aka,
    /// `; `-separated song themes.
    Theme,
    User1,
    User2,
    User3,
}

impl SongField {
    /// Every field, in the form's order.
    pub const ALL: [SongField; 12] = [
        Self::Title,
        Self::Author,
        Self::Copyright,
        Self::Ccli,
        Self::HymnNumber,
        Self::Key,
        Self::Capo,
        Self::Aka,
        Self::Theme,
        Self::User1,
        Self::User2,
        Self::User3,
    ];

    pub fn get(self, song: &Song) -> &str {
        match self {
            Self::Title => &song.title,
            Self::Author => &song.author,
            Self::Copyright => &song.copyright,
            Self::Ccli => &song.ccli,
            Self::HymnNumber => &song.hymn_number,
            Self::Key => &song.key,
            Self::Capo => &song.capo,
            Self::Aka => &song.aka,
            Self::Theme => &song.theme,
            Self::User1 => &song.user1,
            Self::User2 => &song.user2,
            Self::User3 => &song.user3,
        }
    }

    fn set(self, song: &mut Song, value: String) {
        *match self {
            Self::Title => &mut song.title,
            Self::Author => &mut song.author,
            Self::Copyright => &mut song.copyright,
            Self::Ccli => &mut song.ccli,
            Self::HymnNumber => &mut song.hymn_number,
            Self::Key => &mut song.key,
            Self::Capo => &mut song.capo,
            Self::Aka => &mut song.aka,
            Self::Theme => &mut song.theme,
            Self::User1 => &mut song.user1,
            Self::User2 => &mut song.user2,
            Self::User3 => &mut song.user3,
        } = value;
    }
}

/// One change to the open song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongEdit {
    /// Set a detail to this text.
    Field(SongField, String),
    /// Replace the whole lyrics text; the sections and warnings are read from it again.
    Lyrics(String),
}

/// What an edit changes, to join consecutive edits of the same thing into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Field(SongField),
    Lyrics,
}

impl SongEdit {
    fn target(&self) -> Target {
        match self {
            Self::Field(field, _) => Target::Field(*field),
            Self::Lyrics(_) => Target::Lyrics,
        }
    }

    /// The song with the edit applied, or `None` when it would change nothing.
    fn applied(&self, song: &Song) -> Option<Song> {
        let mut edited = song.clone();
        match self {
            Self::Field(field, value) => {
                if field.get(song) == value {
                    return None;
                }
                field.set(&mut edited, value.clone());
            }
            Self::Lyrics(text) => {
                // Lines end in LF inside the engine, whatever the editor widget produced.
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                if text == song.lyrics {
                    return None;
                }
                (edited.sections, edited.warnings) = lyrics::parse(&text);
                edited.lyrics = text;
            }
        }
        Some(edited)
    }
}

/// A song being edited: the edited song and its undo history. Each document has its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The song with its edits applied.
    pub song: Arc<Song>,
    /// The song as its file holds it (as read, or as last saved).
    pub base: Arc<Song>,
    /// The hash of the file's bytes [`base`](Self::base) was read from or saved as. A save
    /// writes only if the file still has it.
    pub hash: ContentHash,
    undo: Vec<Arc<Song>>,
    redo: Vec<Arc<Song>>,
    /// The thing the last edit changed, while edits to it are still joined into one step.
    group: Option<Target>,
}

impl Draft {
    pub(crate) fn new(song: Arc<Song>, hash: ContentHash) -> Self {
        Self { base: Arc::clone(&song), song, hash, undo: Vec::new(), redo: Vec::new(), group: None }
    }

    /// Whether the song differs from its file (so a save would write it). Undoing every edit
    /// since the last save makes it unmodified again.
    pub fn is_modified(&self) -> bool {
        !Arc::ptr_eq(&self.song, &self.base) && *self.song != *self.base
    }

    /// The song was saved: the file now holds `song` (as read back) with this hash. The undo
    /// history stays.
    pub(crate) fn saved(&mut self, song: Arc<Song>, hash: ContentHash) {
        self.base = Arc::clone(&song);
        self.song = song;
        self.hash = hash;
        self.group = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Apply an edit. Returns whether the song changed. Consecutive edits of the same field (or
    /// of the lyrics) join into one undo step until [`end_group`](Self::end_group), undo or redo.
    pub(crate) fn edit(&mut self, edit: &SongEdit) -> bool {
        let Some(edited) = edit.applied(&self.song) else { return false };
        let target = edit.target();
        if self.group != Some(target) {
            self.undo.push(Arc::clone(&self.song));
            self.group = Some(target);
        }
        self.redo.clear();
        self.song = Arc::new(edited);
        true
    }

    pub(crate) fn end_group(&mut self) {
        self.group = None;
    }

    pub(crate) fn undo(&mut self) -> bool {
        self.group = None;
        let Some(previous) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.song, previous));
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        self.group = None;
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.song, next));
        true
    }
}
