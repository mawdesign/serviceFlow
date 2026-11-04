//! Changes made on disk by other programs (OpenSong, another instance, sync). The library and
//! search index follow; an open clean document reloads and loses its undo history; one with
//! unsaved edits is marked as changed on disk and left alone (ADR-0001).

use super::{refresh_results, sets, show_song};
use crate::command::{Command, Effect, Reloaded};
use crate::ids::SongId;
use crate::set::Set;
use crate::state::{OpenSet, Problem, State};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::DocumentsChanged { mut songs, mut sets } => {
            songs.sort();
            songs.dedup();
            sets.sort();
            sets.dedup();
            // A song being saved by us announces itself; the save's result updates everything.
            let songs = songs.into_iter().filter(|id| !state.saving.contains(id)).map(Effect::ReloadSong);
            songs.chain(sets.into_iter().map(Effect::ReloadSet)).collect()
        }
        Command::SongReloaded { id, file } => song_reloaded(state, id, file),
        Command::SetReloaded { id, file } => set_reloaded(state, id, file),
        _ => Vec::new(),
    }
}

/// The song's edits are over a file that changed: wait for the operator's choice.
pub(super) fn mark_changed_on_disk(state: &mut State, id: &SongId) {
    if !state.save_conflicts.contains(id) {
        state.save_conflicts.push(id.clone());
    }
}

fn song_reloaded(state: &mut State, id: SongId, file: Reloaded<crate::song::Song>) -> Vec<Effect> {
    if state.saving.contains(&id) {
        return Vec::new();
    }
    let is_open = state.song.as_ref().is_some_and(|open| open.id == id);
    let modified = state.drafts.get(&id).is_some_and(|d| d.is_modified());
    match file {
        Reloaded::Gone => {
            let lib = &mut state.library;
            let listed = lib.songs.binary_search(&id);
            if let Ok(position) = listed {
                lib.songs.remove(position);
            }
            lib.queue.retain(|queued| *queued != id);
            std::sync::Arc::make_mut(&mut lib.index).remove(&id);
            refresh_results(lib);
            if modified {
                mark_changed_on_disk(state, &id);
            } else {
                state.drafts.remove(&id);
                if is_open {
                    state.song = None;
                }
            }
            sets::check_songs(state)
        }
        Reloaded::Unreadable(error) => {
            state.problems.push(Problem::SongUnreadable { id, error });
            Vec::new()
        }
        Reloaded::Read(song, hash) => {
            let lib = &mut state.library;
            let added = match lib.songs.binary_search(&id) {
                Ok(_) => false,
                Err(position) => {
                    lib.songs.insert(position, id.clone());
                    true
                }
            };
            std::sync::Arc::make_mut(&mut lib.index).insert(&id, &song);
            refresh_results(lib);
            let effects = if added { sets::check_songs(state) } else { Vec::new() };
            let known = state.drafts.get(&id).map(|d| d.hash).or_else(|| state.song.as_ref().filter(|o| o.id == id).map(|o| o.hash));
            if known == Some(hash) {
                return effects;
            }
            if modified {
                mark_changed_on_disk(state, &id);
                return effects;
            }
            state.drafts.remove(&id);
            if is_open {
                show_song(state, id, song, hash);
            }
            effects
        }
    }
}

fn set_reloaded(state: &mut State, id: crate::ids::SetId, file: Reloaded<Set>) -> Vec<Effect> {
    match file {
        Reloaded::Gone => vec![Effect::ListSets],
        Reloaded::Unreadable(error) => {
            state.problems.push(Problem::SetUnreadable { id, error });
            Vec::new()
        }
        Reloaded::Read(set, _) => {
            let mut effects = Vec::new();
            if state.sets.binary_search(&id).is_err() {
                effects.push(Effect::ListSets);
            }
            let library_listed = !state.library.songs.is_empty();
            let songs = state.library.songs.clone();
            let Some(open) = state.set.as_mut().filter(|open| open.id == id) else { return effects };
            if *open.set == *set {
                return effects;
            }
            if open.modified {
                open.changed_on_disk = true;
                return effects;
            }
            let selected = open.selected;
            // The sidecar is not watched (#23): keep the settings already read.
            let mut fresh = OpenSet::new(id, set, std::sync::Arc::clone(&open.sidecar));
            if library_listed {
                fresh.missing = fresh.set.items.iter().filter_map(|item| item.song_id()).filter(|song| songs.binary_search(song).is_err()).collect();
            }
            *open = fresh;
            effects.extend(sets::check_songs(state));
            if let Some(index) = selected.filter(|i| state.set.as_ref().is_some_and(|s| *i < s.set.items.len())) {
                effects.extend(sets::select(state, index, None));
            }
            effects
        }
    }
}
