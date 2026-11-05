//! Song editing: edits, undo and redo of the open song (ADR-0001). Document edits only; live
//! actions never enter the history.

use std::sync::Arc;

use crate::command::{Command, Effect};
use crate::edit::Draft;
use crate::state::State;

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    let Some(open) = &state.song else { return Vec::new() };
    let id = open.id.clone();
    let draft = match command {
        Command::EditSong(edit) => {
            // Edits start from the song as in its file, not with a set item's order.
            let draft = state.drafts.entry(id.clone()).or_insert_with(|| Draft::new(Arc::clone(&open.file), open.hash));
            draft.edit(&edit).then_some(draft)
        }
        Command::EndEditGroup => {
            if let Some(draft) = state.drafts.get_mut(&id) {
                draft.end_group();
            }
            None
        }
        Command::Undo => state.drafts.get_mut(&id).and_then(|draft| draft.undo().then_some(draft)),
        Command::Redo => state.drafts.get_mut(&id).and_then(|draft| draft.redo().then_some(draft)),
        _ => {
            super::unrouted(&command);
            None
        }
    };
    if let Some(draft) = draft {
        let song = Arc::clone(&draft.song);
        let hash = state.song.as_ref().map_or(draft.hash, |open| open.hash);
        super::show_song(state, id, song, hash);
    }
    Vec::new()
}
