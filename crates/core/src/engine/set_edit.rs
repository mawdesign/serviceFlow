//! Set editing (ADR-0001): edits, undo and redo of the open set, and saving it (ADR-0002). Only a
//! set with edits is written, and only if its file still has the hash it was read with; otherwise
//! the operator chooses to overwrite, reload or save a copy. The set's sidecar (item settings,
//! ADR-0003) is saved with it: item settings changed since the last save are written by the
//! same save, after the set. Saving under a new name (or duplicating) a set with a sidecar writes
//! two files, and renaming a set moves its sidecar, so those go through a change plan.

use std::sync::Arc;

use super::{sets, show_song};
use crate::command::{Command, ConflictChoice, Effect, SetWrite};
use crate::ids::{Expect, SetId, SongId, sanitise_filename};
use crate::plan::{ChangePlan, PlanAction, PlanStep};
use crate::set::Set;
use crate::set_edit::{Change, SetEdit, SetVersion};
use crate::sidecar::SetSidecar;
use crate::state::{Problem, State};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::EditSet(edit) => {
            let selected = state.set.as_ref().and_then(|s| s.selected);
            let style = state.styles.default.clone();
            let target = edit.target();
            let adds_song = matches!(edit, SetEdit::AddSong { .. });
            let mut effects = change(state, target, |set| edit.applied(set, selected, &style));
            if adds_song {
                effects.extend(sets::check_songs(state));
            }
            effects
        }
        Command::UndoSet | Command::RedoSet => {
            let undo = command == Command::UndoSet;
            let Some(open) = &mut state.set else { return Vec::new() };
            let history = &mut open.history;
            history.group = None;
            let (from, to) = if undo { (&mut history.undo, &mut history.redo) } else { (&mut history.redo, &mut history.undo) };
            let Some(version) = from.pop() else { return Vec::new() };
            let current = SetVersion { set: std::mem::replace(&mut open.set, version.set), uids: std::mem::replace(&mut open.uids, version.uids) };
            to.push(current);
            open.refresh_modified();
            open.relinking = None;
            open.finding_image = None;
            open.selected = open.selected.filter(|i| *i < open.set.items.len());
            let mut effects = reshow(state);
            effects.extend(sets::check_songs(state));
            effects
        }
        Command::SaveSet => {
            let Some(open) = state.set.as_mut().filter(|o| o.modified && !o.saving && !o.changed_on_disk) else { return Vec::new() };
            open.saving = true;
            let sidecar = changed_sidecar(open);
            if *open.set == *open.base {
                // Only item settings changed: the set file stays as it is.
                return vec![Effect::WriteSidecar { set: open.id.clone(), sidecar: sidecar.unwrap_or_default() }];
            }
            let expect = Expect::Hash(open.hash);
            vec![Effect::WriteSet { id: open.id.clone(), to: open.id.clone(), set: Arc::clone(&open.set), expect, purpose: SetWrite::Save, sidecar }]
        }
        Command::RenameSet { name } => rename(state, &name),
        Command::SaveSetAs { name } => save_as(state, &name, SetWrite::SaveAs),
        Command::DuplicateSet { name } => save_as(state, &name, SetWrite::Duplicate),
        Command::ResolveSetConflict(choice) => {
            let Some(open) = state.set.as_mut().filter(|o| o.changed_on_disk && !o.saving) else { return Vec::new() };
            match choice {
                ConflictChoice::Overwrite => {
                    open.saving = true;
                    let (id, set, sidecar) = (open.id.clone(), Arc::clone(&open.set), changed_sidecar(open));
                    vec![Effect::WriteSet { id: id.clone(), to: id, set, expect: Expect::Any, purpose: SetWrite::Save, sidecar }]
                }
                ConflictChoice::Reload => {
                    let id = open.id.clone();
                    super::step(state, Command::OpenSet(id))
                }
                ConflictChoice::SaveAsCopy => {
                    let id = open.id.clone();
                    let name = copy_name(state, &id);
                    save_as(state, &name, SetWrite::SaveAs)
                }
            }
        }
        Command::SetSaved { id, saved_as, set, hash, purpose, sidecar, sidecar_error } => {
            let mut effects = if purpose == SetWrite::Save { Vec::new() } else { vec![Effect::ListSets] };
            if let Some(error) = sidecar_error {
                state.problems.push(Problem::SidecarUnsaved { id: saved_as.clone(), error });
            }
            let Some(open) = state.set.as_mut().filter(|o| o.id == id) else { return effects };
            if purpose == SetWrite::Duplicate {
                return effects;
            }
            // The file now holds `set`: it is the open set, with no edits (the history stays, so
            // undo past a save makes it modified again).
            if let Some(sidecar) = sidecar {
                open.saved_sidecar = sidecar;
            } else if purpose == SetWrite::SaveAs {
                // A set saved under a new name without settings to keep has no sidecar.
                open.saved_sidecar = Arc::default();
            }
            open.id = saved_as;
            open.base = Arc::clone(&set);
            open.replace_set(set);
            open.hash = hash;
            open.saving = false;
            open.changed_on_disk = false;
            open.history.group = None;
            open.refresh_modified();
            effects.extend(reshow(state));
            effects
        }
        Command::SetSaveConflict { id, saved_as, purpose } => {
            match purpose {
                SetWrite::Save => {
                    if let Some(open) = state.set.as_mut().filter(|o| o.id == id) {
                        open.saving = false;
                        open.changed_on_disk = true;
                    }
                }
                SetWrite::SaveAs | SetWrite::Duplicate => state.problems.push(Problem::SetNameRefused { name: saved_as.0, taken: true }),
            }
            Vec::new()
        }
        Command::SetSaveFailed { id, saved_as, purpose, error } => {
            if purpose == SetWrite::Save
                && let Some(open) = state.set.as_mut().filter(|o| o.id == id)
            {
                open.saving = false;
            }
            state.problems.push(Problem::SetUnsaved { id: saved_as, error });
            Vec::new()
        }
        Command::ListBackgrounds => vec![Effect::ListBackgrounds],
        _ => super::unrouted(&command),
    }
}

/// End the open set's undo step, so the next edit starts a new one.
pub(super) fn end_group(state: &mut State) {
    if let Some(open) = &mut state.set {
        open.history.group = None;
    }
}

/// Apply an edit of the open set (`edit` gives the changed set, or `None` when it does not
/// apply). An edit that changes nothing is not an undo step; edits of the same `target` join.
pub(super) fn change(state: &mut State, target: Option<crate::set_edit::Target>, edit: impl FnOnce(&Set) -> Option<(Set, Change)>) -> Vec<Effect> {
    let Some(open) = &mut state.set else { return Vec::new() };
    let Some((set, change)) = edit(&open.set) else { return Vec::new() };
    if set == *open.set {
        return Vec::new();
    }
    if target.is_none() || open.history.group != target {
        open.history.undo.push(SetVersion { set: Arc::clone(&open.set), uids: Arc::clone(&open.uids) });
    }
    open.history.group = target;
    open.history.redo.clear();
    open.set = Arc::new(set);
    follow_items(open, change);
    open.refresh_modified();
    if change != Change::InPlace {
        open.relinking = None;
        open.finding_image = None;
        // The selection stays on the same item; a custom or image item live keeps its slides
        // under its new position.
        open.selected = open.selected.and_then(|i| change.remap(i));
        if let Some(live) = &mut state.output.live {
            if let Some(index) = live.song.as_str().strip_prefix(":item/").and_then(|i| i.parse::<usize>().ok())
                && let Some(moved) = change.remap(index)
            {
                live.song = SongId::set_item(moved);
            }
            // The live slide stays with its item (a removed item's slide belongs to none).
            if let Some(item) = live.item {
                live.item = Some(change.remap(item).unwrap_or(usize::MAX));
            }
        }
    }
    reshow(state)
}

/// Each item keeps its identity (and so its settings) where `change` put it; a new item gets a
/// new one, and a copy also gets the settings of the item it copies.
fn follow_items(open: &mut crate::state::OpenSet, change: Change) {
    let mut uids = (*open.uids).clone();
    let fresh = open.next_uid;
    match change {
        Change::Inserted(at) => uids.insert(at, fresh),
        Change::Copied { from, at } => {
            if let Some(settings) = uids.get(from).and_then(|source| open.settings.get(source)).copied() {
                open.settings.insert(fresh, settings);
            }
            uids.insert(at, fresh);
        }
        Change::Removed(at) => {
            uids.remove(at);
        }
        Change::Moved { from, to } => {
            let uid = uids.remove(from);
            uids.insert(to, uid);
        }
        Change::InPlace => return,
    }
    if matches!(change, Change::Inserted(_) | Change::Copied { .. }) {
        open.next_uid += 1;
    }
    open.uids = Arc::new(uids);
}

/// The open set's sidecar, if its item settings differ from what its sidecar file holds.
fn changed_sidecar(open: &crate::state::OpenSet) -> Option<Arc<SetSidecar>> {
    let sidecar = open.sidecar();
    (sidecar != *open.saved_sidecar).then(|| Arc::new(sidecar))
}

/// Show the selected item as it is now: a custom or image item's slides are rebuilt, a song item's
/// song gets its (new) order, a relinked item loads its new song. A removed selection closes a
/// custom or image item's slides.
fn reshow(state: &mut State) -> Vec<Effect> {
    let Some(set) = &state.set else { return Vec::new() };
    let Some(index) = set.selected else {
        if state.song.as_ref().is_some_and(|s| s.id.is_set_item()) {
            state.song = None;
        }
        return Vec::new();
    };
    let item = &set.set.items[index];
    if let Some(open) = sets::open_item(index, item) {
        state.song = Some(open);
        return Vec::new();
    }
    let id = item.song_id();
    match (&state.song, id) {
        (Some(open), Some(id)) if open.id == id => {
            let (file, hash) = (Arc::clone(&open.file), open.hash);
            show_song(state, id, file, hash);
            Vec::new()
        }
        (_, Some(id)) if state.loading.as_ref() == Some(&id) => Vec::new(),
        _ => sets::select(state, index, None),
    }
}

/// `name (copy)`, `name (copy 2)`, … : the first that is not a set yet.
fn copy_name(state: &State, id: &SetId) -> String {
    (1..)
        .map(|n| if n == 1 { format!("{id} (copy)") } else { format!("{id} (copy {n})") })
        .find(|name| !state.sets.iter().any(|s| s.as_str().eq_ignore_ascii_case(name)))
        .unwrap_or_default()
}

/// Write the open set under a new name: with its edits (`SaveAs`, which then becomes the open
/// set) or as saved (`Duplicate`). A set with serviceFlow-only settings takes its sidecar along,
/// which is a second file, so that is shown as a change plan first.
fn save_as(state: &mut State, name: &str, purpose: SetWrite) -> Vec<Effect> {
    let Some(to) = new_name(state, name) else { return Vec::new() };
    let Some(open) = &state.set else { return Vec::new() };
    // With its edits the set takes its item settings as they are now; as saved, as saved.
    let (source, sidecar) =
        if purpose == SetWrite::Duplicate { (&open.base, Arc::clone(&open.saved_sidecar)) } else { (&open.set, Arc::new(open.sidecar())) };
    // OpenSong writes the set's name into the file.
    let set = Arc::new(Set { name: to.0.clone(), ..(**source).clone() });
    if sidecar.items.is_empty() {
        return vec![Effect::WriteSet { id: open.id.clone(), to, set, expect: Expect::Absent, purpose, sidecar: None }];
    }
    if super::plan::busy(state) {
        return Vec::new();
    }
    let from = open.id.clone();
    let steps = vec![PlanStep::WriteSet { id: to.clone(), set, expect: Expect::Absent }, PlanStep::WriteSidecar { set: to.clone(), sidecar }];
    let action = if purpose == SetWrite::Duplicate { PlanAction::DuplicateSet { from, to } } else { PlanAction::SaveSetAs { from, to } };
    state.plan = Some(Arc::new(ChangePlan { action, steps, users: Vec::new(), unreadable_sets: Vec::new(), tidy: None }));
    Vec::new()
}

/// The set id a new name for the open set gives, or `None` (with a problem) when it is empty
/// after cleaning or already taken (case-insensitive, the open set's own name included).
fn new_name(state: &mut State, name: &str) -> Option<SetId> {
    let open = state.set.as_ref()?;
    let clean = sanitise_filename(name);
    if clean.is_empty() {
        state.problems.push(Problem::SetNameRefused { name: name.to_owned(), taken: false });
        return None;
    }
    let to = SetId::new(clean);
    let taken = |id: &SetId| id.as_str().eq_ignore_ascii_case(to.as_str());
    if taken(&open.id) || state.sets.iter().any(taken) {
        state.problems.push(Problem::SetNameRefused { name: to.0, taken: true });
        return None;
    }
    Some(to)
}

/// Rename the open set's file, as a change plan: the file moves as it is, and its sidecar (as
/// saved) moves with it. Unsaved edits and item settings stay unsaved on the renamed set.
fn rename(state: &mut State, name: &str) -> Vec<Effect> {
    if state.set.as_ref().is_none_or(|open| open.saving) || super::plan::busy(state) {
        return Vec::new();
    }
    let Some(to) = new_name(state, name) else { return Vec::new() };
    let Some(open) = &state.set else { return Vec::new() };
    let from = open.id.clone();
    let mut steps = vec![PlanStep::RenameSet { from: from.clone(), to: to.clone(), expect: Expect::Hash(open.hash) }];
    if !open.saved_sidecar.items.is_empty() {
        steps.push(PlanStep::WriteSidecar { set: to.clone(), sidecar: Arc::clone(&open.saved_sidecar) });
        steps.push(PlanStep::DeleteSidecar { set: from.clone() });
    }
    let action = PlanAction::RenameSet { from, to };
    state.plan = Some(Arc::new(ChangePlan { action, steps, users: Vec::new(), unreadable_sets: Vec::new(), tidy: None }));
    Vec::new()
}
