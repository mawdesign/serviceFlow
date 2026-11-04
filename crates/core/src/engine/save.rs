//! Canonical save of songs (ADR-0002, GLOSSARY "Canonical save"). Only songs with edits are
//! written, and only if the file still has the hash it was read with; otherwise the operator
//! chooses to overwrite, reload or save a copy. A save that renames tags goes through a change
//! plan ([`super::plan`]) when sets refer to the song with an order naming those tags.

use std::sync::Arc;

use super::{refresh_results, show_song};
use crate::canonical;
use crate::command::{Command, ConflictChoice, Effect};
use crate::plan::PlanRequest;
use crate::ids::{Expect, SongId};
use crate::song::Song;
use crate::state::{Problem, State};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::SaveSong(id) => {
            // Waiting for the operator's choice, or already on its way.
            if state.save_conflicts.contains(&id) || state.saving.contains(&id) {
                return Vec::new();
            }
            let Some(draft) = state.drafts.get(&id).filter(|d| d.is_modified()) else { return Vec::new() };
            let expect = Expect::Hash(draft.hash);
            save(state, id, expect)
        }
        Command::SongSaved { id, saved_as, song, hash, tags_kept } => {
            state.saving.retain(|s| *s != id);
            if tags_kept {
                state.problems.push(Problem::TagsNotRenamed { id: saved_as.clone() });
            }
            if saved_as == id {
                if let Some(draft) = state.drafts.get_mut(&id) {
                    draft.saved(Arc::clone(&song), hash);
                }
                if state.song.as_ref().is_some_and(|open| open.id == id) {
                    show_song(state, id.clone(), song.clone(), hash);
                }
                Arc::make_mut(&mut state.library.index).insert(&id, &song);
                refresh_results(&mut state.library);
                Vec::new()
            } else {
                // The edits went to a new file; the original stays as it is on disk.
                state.drafts.remove(&id);
                if state.song.as_ref().is_some_and(|open| open.id == id) {
                    if let Some(set) = &mut state.set {
                        set.selected = None;
                        set.pending = None;
                    }
                    show_song(state, saved_as, song, hash);
                }
                super::step(state, Command::RefreshLibrary)
            }
        }
        Command::SongSaveConflict { id } => {
            state.saving.retain(|s| *s != id);
            if !state.save_conflicts.contains(&id) {
                state.save_conflicts.push(id);
            }
            Vec::new()
        }
        Command::SongSaveFailed { id, error } => {
            state.saving.retain(|s| *s != id);
            state.problems.push(Problem::SongUnsaved { id, error });
            Vec::new()
        }
        Command::ResolveSaveConflict { id, choice } => {
            if !state.save_conflicts.contains(&id) {
                return Vec::new();
            }
            state.save_conflicts.retain(|s| *s != id);
            match choice {
                ConflictChoice::Overwrite => save(state, id, Expect::Any),
                ConflictChoice::SaveAsCopy => write(state, id, Expect::Absent, true),
                ConflictChoice::Reload => {
                    state.drafts.remove(&id);
                    if state.song.as_ref().is_some_and(|open| open.id == id) {
                        state.loading = Some(id.clone());
                        vec![Effect::LoadSong(id)]
                    } else {
                        Vec::new()
                    }
                }
            }
        }
        _ => Vec::new(),
    }
}

/// Save the draft of `id`: straight away, or, when its tags get renamed, through a change plan
/// that also updates the sets whose items name those tags.
fn save(state: &mut State, id: SongId, expect: Expect) -> Vec<Effect> {
    let renames = state.drafts.get(&id).is_some_and(|draft| canonical::renames_tags(&draft.song));
    if !renames {
        return write(state, id, expect, false);
    }
    if super::plan::busy(state) {
        return Vec::new();
    }
    state.saving.push(id.clone());
    super::plan::request(state, PlanRequest::Save { id, expect })
}

/// Ask for the draft of `id` to be written.
pub(super) fn write(state: &mut State, id: SongId, expect: Expect, as_copy: bool) -> Vec<Effect> {
    let Some(draft) = state.drafts.get(&id) else { return Vec::new() };
    let song: Arc<Song> = Arc::clone(&draft.song);
    if !state.saving.contains(&id) {
        state.saving.push(id.clone());
    }
    vec![Effect::WriteSong { id, song, expect, as_copy }]
}
