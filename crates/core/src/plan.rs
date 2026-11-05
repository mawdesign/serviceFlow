//! Change plans (ADR-0002, GLOSSARY "Change plan"): every file one operator action writes,
//! listed for the operator before anything is written.
//!
//! The engine builds a plan from what storage holds (the sets and the song list), shows it in
//! [`State::plan`](crate::state::State), and only on [`Command::ConfirmPlan`](crate::command::Command)
//! asks the runtime to apply it. Every step is conditional on the hash the file had when the plan
//! was made. The runtime checks every step before writing any, then writes sets first and the
//! song last, so a conflict that still lands midway leaves sets that already agree with the
//! finished change: doing the same action again completes it.

use std::sync::Arc;

use crate::ids::{ContentHash, Expect, SetId, SongId};
use crate::set::Set;
use crate::sidecar::SetSidecar;
use crate::song::Song;

/// What the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    /// Save the edits of a song whose tags get renamed to the canonical form.
    SaveSong(SongId),
    /// Rename or move a song file (another filename, another category, or both).
    RenameSong {
        from: SongId,
        to: SongId,
    },
    DeleteSong(SongId),
    /// Create a new song file.
    NewSong(SongId),
    /// Save a set, with its edits, under a new name (its sidecar goes with it).
    SaveSetAs {
        from: SetId,
        to: SetId,
    },
    /// Copy a set, as saved, to a new name (its sidecar goes with it).
    DuplicateSet {
        from: SetId,
        to: SetId,
    },
    /// Rename a set file (its sidecar moves with it).
    RenameSet {
        from: SetId,
        to: SetId,
    },
    /// Rewrite every song and set not yet in canonical form (ADR-0002, GLOSSARY "Tidy library").
    TidyLibrary,
}

/// One file written by a plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanStep {
    /// Rewrite a set (song items renamed, moved, or with renamed tags in their order).
    WriteSet {
        id: SetId,
        set: Arc<Set>,
        expect: Expect,
    },
    /// Write a song in canonical form.
    WriteSong {
        id: SongId,
        song: Arc<Song>,
        expect: Expect,
    },
    /// Move a song file, bytes unchanged.
    RenameSong {
        from: SongId,
        to: SongId,
        expect: Expect,
    },
    DeleteSong {
        id: SongId,
        expect: Expect,
    },
    /// Move a set file, bytes unchanged.
    RenameSet {
        from: SetId,
        to: SetId,
        expect: Expect,
    },
    /// Write a set's sidecar (ADR-0003). Sidecars hold serviceFlow extras only, so this is written
    /// whatever the file holds.
    WriteSidecar {
        set: SetId,
        sidecar: Arc<SetSidecar>,
    },
    /// Delete a set's sidecar (the set moved away from it), if there is one.
    DeleteSidecar {
        set: SetId,
    },
}

/// A file a plan touches, for showing and reporting.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanFile {
    Song(SongId),
    Set(SetId),
    /// The sidecar of a set.
    Sidecar(SetId),
}

impl PlanStep {
    /// The file this step changes (for a rename: the file as it is now).
    pub fn file(&self) -> PlanFile {
        match self {
            PlanStep::WriteSet { id, .. } => PlanFile::Set(id.clone()),
            PlanStep::WriteSong { id, .. } | PlanStep::DeleteSong { id, .. } => PlanFile::Song(id.clone()),
            PlanStep::RenameSong { from, .. } => PlanFile::Song(from.clone()),
            PlanStep::RenameSet { from, .. } => PlanFile::Set(from.clone()),
            PlanStep::WriteSidecar { set, .. } | PlanStep::DeleteSidecar { set } => PlanFile::Sidecar(set.clone()),
        }
    }
}

/// Every file an action writes, in the order they are written.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangePlan {
    pub action: PlanAction,
    pub steps: Vec<PlanStep>,
    /// Sets that use the song being deleted. They are not changed: their items will show the
    /// song as missing. The operator is warned before confirming.
    pub users: Vec<SetId>,
    /// Sets that could not be read, so could not be checked or updated.
    pub unreadable_sets: Vec<SetId>,
    /// For [`PlanAction::TidyLibrary`]: what changes in each file, and what was left alone.
    pub tidy: Option<TidyReport>,
}

/// What a library tidy does to one file, in words the operator can check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TidyChange {
    /// Section tags renamed to the canonical form, as (before, after).
    TagsRenamed(Vec<(String, String)>),
    /// The song's presentation order names the renamed tags.
    OrderRewritten,
    /// Spaces at the ends of lyrics lines removed.
    TrailingSpaces,
    /// Set items (by name) whose presentation order names tags the tidy renames.
    ItemOrders(Vec<String>),
    /// Nothing but the file layout: encoding, line endings, element order, indentation.
    Layout,
}

/// Why a file was left alone by a tidy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TidySkip {
    /// Renaming the tags would merge two sections (ADR-0002).
    TagCollision,
    /// The file could not be read or parsed.
    Unreadable(String),
    /// The open song has unsaved edits.
    UnsavedEdits,
    /// The file holds things the rewrite would not keep (serviceFlow does not read them).
    Unkept(Vec<Unkept>),
}

/// Something in a song or set file that serviceFlow does not read, so a rewrite of the file
/// would drop it. The element names are as in the file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unkept {
    /// XML comments, processing instructions or a document type declaration.
    Comments,
    /// Attributes of this element.
    Attributes(String),
    /// A second copy of this element: only the first is read.
    Repeated(String),
    /// Elements inside this element, which is read as plain text.
    Elements(String),
    /// Text directly inside this element, between its elements.
    Text(String),
    /// Both `timesig` (the old name) and `time_sig`: only `time_sig` is kept.
    OldTimeSignature,
}

/// The dry-run list of a tidy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TidyReport {
    /// Every file that will be rewritten, sets first and then songs, each in name order.
    pub changes: Vec<(PlanFile, Vec<TidyChange>)>,
    /// Files left alone, and why.
    pub skipped: Vec<(PlanFile, TidySkip)>,
}

/// A song read for a tidy that is not in canonical form (its bytes differ from what a save writes).
#[derive(Debug, Clone, PartialEq)]
pub struct TidySong {
    pub id: SongId,
    pub song: Arc<Song>,
    pub hash: ContentHash,
    /// What the file holds that rewriting it would drop (empty: nothing). Such a file is left alone.
    pub unkept: Vec<Unkept>,
}

/// A set read for a tidy. Canonical sets are listed too: song tag renames may still change them.
#[derive(Debug, Clone, PartialEq)]
pub struct TidySet {
    pub id: SetId,
    pub set: Arc<Set>,
    pub hash: ContentHash,
    /// The file's bytes are what a save would write.
    pub canonical: bool,
    /// What the file holds that rewriting it would drop (empty: nothing). Such a file is left alone.
    pub unkept: Vec<Unkept>,
}

/// An action waiting for storage to be read so its plan can be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRequest {
    /// Save the draft of a song whose tags get renamed, with this condition on its file.
    Save {
        id: SongId,
        expect: Expect,
    },
    Rename {
        from: SongId,
        to: SongId,
    },
    Delete(SongId),
    New {
        category: String,
        title: String,
    },
    Tidy,
}

/// A set read while building a plan, with the hash its file had.
#[derive(Debug, Clone, PartialEq)]
pub struct GatheredSet {
    pub id: SetId,
    pub set: Arc<Set>,
    pub hash: ContentHash,
    /// Its sidecar, when it has a readable one (its keys name songs, so renaming a song
    /// rewrites it with the set).
    pub sidecar: Option<Arc<SetSidecar>>,
}

/// A song a plan wrote, as read back from the written bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenSong {
    pub id: SongId,
    pub song: Arc<Song>,
    pub hash: ContentHash,
    /// The tags were left as written because renaming them would have collided.
    pub tags_kept: bool,
}

/// Why applying a plan stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFailure {
    /// Files already written before the failure (empty when the check before writing failed).
    pub written: Vec<PlanFile>,
    /// The file that could not be written.
    pub failed: PlanFile,
    /// It had changed (or vanished, or the target was taken) since the plan was made.
    pub conflict: bool,
    pub error: String,
}

/// Why no plan could be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRefusal {
    /// The song is not in the library (any more).
    SongMissing(SongId),
    /// A song with the new name already exists.
    NameTaken(SongId),
    /// The new name is not a usable file name (empty, or a part starting with `.` or `_`, which
    /// OpenSong would hide).
    BadName(SongId),
    /// The open set has unsaved edits, which a tidy would overwrite.
    UnsavedSet(SetId),
    /// Storage could not be read to make the plan.
    Unreadable(String),
}
