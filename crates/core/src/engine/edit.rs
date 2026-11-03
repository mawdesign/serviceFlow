//! Song editing: edits, undo and redo of the open song (ADR-0001). Document edits only; live
//! actions never enter the history.

use std::sync::Arc;

use crate::command::{Command, Effect};
use crate::edit::Draft;
use crate::state::{OpenSong, State};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    let Some(open) = &state.song else { return Vec::new() };
    let id = open.id.clone();
    let draft = match command {
        Command::EditSong(edit) => {
            let draft = state.drafts.entry(id.clone()).or_insert_with(|| Draft::new(Arc::clone(&open.song)));
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
        _ => None,
    };
    if let Some(draft) = draft {
        let song = Arc::clone(&draft.song);
        let options = state.slide_options;
        if let Some(open) = &mut state.song {
            *open = OpenSong::new(open.id.clone(), song, open.hash, &options);
        }
    }
    Vec::new()
}
