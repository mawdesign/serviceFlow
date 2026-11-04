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
use crate::song::Song;

/// What the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    /// Save the edits of a song whose tags get renamed to the canonical form.
    SaveSong(SongId),
    /// Rename or move a song file (another filename, another category, or both).
    RenameSong { from: SongId, to: SongId },
    DeleteSong(SongId),
    /// Create a new song file.
    NewSong(SongId),
}

/// One file written by a plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanStep {
    /// Rewrite a set (song items renamed, moved, or with renamed tags in their order).
    WriteSet { id: SetId, set: Arc<Set>, expect: Expect },
    /// Write a song in canonical form.
    WriteSong { id: SongId, song: Arc<Song>, expect: Expect },
    /// Move a song file, bytes unchanged.
    RenameSong { from: SongId, to: SongId, expect: Expect },
    DeleteSong { id: SongId, expect: Expect },
}

/// A file a plan touches, for showing and reporting.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanFile {
    Song(SongId),
    Set(SetId),
}

impl PlanStep {
    /// The file this step changes (for a rename: the file as it is now).
    pub fn file(&self) -> PlanFile {
        match self {
            PlanStep::WriteSet { id, .. } => PlanFile::Set(id.clone()),
            PlanStep::WriteSong { id, .. } | PlanStep::DeleteSong { id, .. } => PlanFile::Song(id.clone()),
            PlanStep::RenameSong { from, .. } => PlanFile::Song(from.clone()),
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
}

/// An action waiting for storage to be read so its plan can be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRequest {
    /// Save the draft of a song whose tags get renamed, with this condition on its file.
    Save { id: SongId, expect: Expect },
    Rename { from: SongId, to: SongId },
    Delete(SongId),
    New { category: String, title: String },
}

/// A set read while building a plan, with the hash its file had.
#[derive(Debug, Clone, PartialEq)]
pub struct GatheredSet {
    pub id: SetId,
    pub set: Arc<Set>,
    pub hash: ContentHash,
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
    /// Storage could not be read to make the plan.
    Unreadable(String),
}
