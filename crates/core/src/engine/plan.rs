//! Change plans: renaming tags on save, renaming/moving, deleting and creating songs. Storage is
//! read first ([`Effect::GatherForPlan`]), the plan is made here (pure), shown in
//! [`State::plan`], and written only after [`Command::ConfirmPlan`].

use std::sync::Arc;

use super::show_song;
use crate::canonical;
use crate::command::{Command, Effect};
use crate::ids::{ContentHash, Expect, SongId};
use crate::plan::{ChangePlan, GatheredSet, PlanAction, PlanFile, PlanRefusal, PlanRequest, PlanStep, WrittenSong};
use crate::search::{category, filename};
use crate::set::{ItemKind, Set};
use crate::song::Song;
use crate::state::{Problem, State};

/// Whether a plan is being made, shown or written: only one at a time.
pub(super) fn busy(state: &State) -> bool {
    state.planning.is_some() || state.plan.is_some()
}

/// Start making the plan for `request`: read storage first.
pub(super) fn request(state: &mut State, request: PlanRequest) -> Vec<Effect> {
    if busy(state) {
        return Vec::new();
    }
    let subject = match &request {
        PlanRequest::Rename { from, .. } => Some(from.clone()),
        PlanRequest::Delete(id) => Some(id.clone()),
        PlanRequest::Save { .. } | PlanRequest::New { .. } | PlanRequest::Tidy => None,
    };
    state.planning = Some(request);
    vec![Effect::GatherForPlan { subject }]
}

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::RenameSong { from, to } => request(state, PlanRequest::Rename { from, to }),
        Command::DeleteSong(id) => request(state, PlanRequest::Delete(id)),
        Command::NewSong { category, title } => request(state, PlanRequest::New { category, title }),
        Command::TidyLibrary | Command::TidyGathered { .. } => super::tidy::apply(state, command),
        Command::PlanGathered { songs, sets, unreadable_sets, subject, error } => {
            let Some(request) = state.planning.take() else { return Vec::new() };
            if let Some(error) = error {
                return refuse(state, &request, PlanRefusal::Unreadable(error));
            }
            let gathered = Gathered { songs: &songs, sets: &sets, subject };
            match make(state, &request, &gathered) {
                Made::Plan(plan) => {
                    let plan = ChangePlan { unreadable_sets, ..plan };
                    state.plan = Some(Arc::new(plan));
                    Vec::new()
                }
                // A save that changes no set is just the save.
                Made::PlainSave { id, expect } => super::save::write(state, id, expect, false),
                Made::Refused(refusal) => refuse(state, &request, refusal),
            }
        }
        Command::ConfirmPlan => match &state.plan {
            // A tidy that finds nothing to do is shown as an empty list: confirming just closes it.
            Some(plan) if plan.steps.is_empty() && !state.applying_plan => {
                state.plan = None;
                Vec::new()
            }
            Some(plan) if !state.applying_plan => {
                state.applying_plan = true;
                vec![Effect::ApplyPlan(Arc::clone(plan))]
            }
            _ => Vec::new(),
        },
        Command::CancelPlan => {
            if state.applying_plan {
                return Vec::new();
            }
            if let Some(plan) = state.plan.take()
                && let PlanAction::SaveSong(id) = &plan.action
            {
                state.saving.retain(|s| s != id);
            }
            Vec::new()
        }
        Command::PlanApplied { plan, result } => {
            state.plan = None;
            state.applying_plan = false;
            match result {
                Ok(written) => applied(state, &plan, written),
                Err(failure) => {
                    if let PlanAction::SaveSong(id) = &plan.action {
                        state.saving.retain(|s| s != id);
                    }
                    let written = failure.written.clone();
                    state.problems.push(Problem::PlanIncomplete { action: plan.action.clone(), failure });
                    after_writes(state, &plan, &written)
                }
            }
        }
        _ => Vec::new(),
    }
}

fn refuse(state: &mut State, request: &PlanRequest, refusal: PlanRefusal) -> Vec<Effect> {
    if let PlanRequest::Save { id, .. } = request {
        state.saving.retain(|s| s != id);
    }
    state.problems.push(Problem::PlanRefused(refusal));
    Vec::new()
}

struct Gathered<'a> {
    songs: &'a [SongId],
    sets: &'a [GatheredSet],
    subject: Option<ContentHash>,
}

enum Made {
    Plan(ChangePlan),
    PlainSave { id: SongId, expect: Expect },
    Refused(PlanRefusal),
}

fn plan(action: PlanAction, steps: Vec<PlanStep>) -> Made {
    Made::Plan(ChangePlan { action, steps, users: Vec::new(), unreadable_sets: Vec::new(), tidy: None })
}

fn make(state: &State, request: &PlanRequest, gathered: &Gathered<'_>) -> Made {
    match request {
        PlanRequest::Save { id, expect } => {
            let Some(draft) = state.drafts.get(id) else { return Made::Refused(PlanRefusal::SongMissing(id.clone())) };
            let before = &draft.song;
            let after = canonical::canonicalize(before).song;
            let mut steps = set_steps(gathered.sets, id, |item_order| {
                let renamed = canonical::rename_tokens(before, &after, item_order);
                (renamed != item_order).then_some(Rewrite { order: Some(renamed), to: None })
            });
            if steps.is_empty() {
                return Made::PlainSave { id: id.clone(), expect: *expect };
            }
            steps.push(PlanStep::WriteSong { id: id.clone(), song: Arc::clone(before), expect: *expect });
            plan(PlanAction::SaveSong(id.clone()), steps)
        }
        PlanRequest::Rename { from, to } => {
            let Some(hash) = gathered.subject else { return Made::Refused(PlanRefusal::SongMissing(from.clone())) };
            if !usable_name(to) {
                return Made::Refused(PlanRefusal::BadName(to.clone()));
            }
            let same_file = from.as_str().to_lowercase() == to.as_str().to_lowercase();
            if from == to || (!same_file && gathered.songs.iter().any(|s| s.as_str().to_lowercase() == to.as_str().to_lowercase())) {
                return Made::Refused(PlanRefusal::NameTaken(to.clone()));
            }
            let mut steps = set_steps(gathered.sets, from, |_| Some(Rewrite { order: None, to: Some(to) }));
            steps.push(PlanStep::RenameSong { from: from.clone(), to: to.clone(), expect: Expect::Hash(hash) });
            plan(PlanAction::RenameSong { from: from.clone(), to: to.clone() }, steps)
        }
        PlanRequest::Delete(id) => {
            let Some(hash) = gathered.subject else { return Made::Refused(PlanRefusal::SongMissing(id.clone())) };
            let users = gathered.sets.iter().filter(|g| g.set.items.iter().any(|i| i.song_id().as_ref() == Some(id))).map(|g| g.id.clone()).collect();
            Made::Plan(ChangePlan {
                action: PlanAction::DeleteSong(id.clone()),
                steps: vec![PlanStep::DeleteSong { id: id.clone(), expect: Expect::Hash(hash) }],
                users,
                unreadable_sets: Vec::new(),
                tidy: None,
            })
        }
        PlanRequest::Tidy => Made::Refused(PlanRefusal::Unreadable("not a plan of this kind".to_owned())),
        PlanRequest::New { category, title } => {
            let id = SongId::for_new_song(category, title, gathered.songs);
            if !usable_name(&id) {
                return Made::Refused(PlanRefusal::BadName(id));
            }
            let song = Song { title: title.trim().to_owned(), ..Song::default() };
            plan(PlanAction::NewSong(id.clone()), vec![PlanStep::WriteSong { id, song: Arc::new(song), expect: Expect::Absent }])
        }
    }
}

/// A song id storage and OpenSong both accept and show: no empty, `.` or `..` parts, none
/// starting with `.` or `_`, no `\`, `:` or control characters.
fn usable_name(id: &SongId) -> bool {
    let path = id.as_str();
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !part.trim().is_empty() && !part.starts_with(['.', '_']))
}

/// What happens to a song item.
struct Rewrite<'a> {
    /// A new presentation-order override.
    order: Option<Vec<String>>,
    /// The song's new id (name and path).
    to: Option<&'a SongId>,
}

/// One [`PlanStep::WriteSet`] for every set with a song item referring to `id` that `rewrite`
/// changes. `rewrite` gets the item's presentation-order override (empty: none); renamed tags
/// only matter for items with an override.
fn set_steps<'a>(sets: &[GatheredSet], id: &SongId, rewrite: impl Fn(&[String]) -> Option<Rewrite<'a>>) -> Vec<PlanStep> {
    let mut steps = Vec::new();
    for gathered in sets {
        let mut set: Set = (*gathered.set).clone();
        if rewrite_items(&mut set, id, &rewrite) {
            steps.push(PlanStep::WriteSet { id: gathered.id.clone(), set: Arc::new(set), expect: Expect::Hash(gathered.hash) });
        }
    }
    steps
}

/// Rewrite every song item of `set` referring to `id` as `rewrite` says; whether any changed.
fn rewrite_items<'a>(set: &mut Set, id: &SongId, rewrite: impl Fn(&[String]) -> Option<Rewrite<'a>>) -> bool {
    let mut changed = false;
    for item in &mut set.items {
        if item.song_id().as_ref() != Some(id) {
            continue;
        }
        let ItemKind::Song(song) = &mut item.kind else { continue };
        let Some(change) = rewrite(&song.presentation) else { continue };
        if let Some(order) = change.order {
            song.presentation = order;
        }
        if let Some(to) = change.to {
            item.name = filename(to).to_owned();
            song.path = category(to).to_owned();
        }
        changed = true;
    }
    changed
}

/// `set` with the change `plan` makes to the song items of the sets it writes, for a set the
/// plan did not read from storage (the open set's unsaved edits and their undo history).
/// `None`: the plan changes nothing in it.
fn rewritten_like(plan: &ChangePlan, set: &Set) -> Option<Set> {
    let mut set = set.clone();
    let changed = match &plan.action {
        PlanAction::RenameSong { from, to } => rewrite_items(&mut set, from, |_| Some(Rewrite { order: None, to: Some(to) })),
        PlanAction::SaveSong(id) => {
            let before = plan.steps.iter().find_map(|step| match step {
                PlanStep::WriteSong { id: written, song, .. } if written == id => Some(song),
                _ => None,
            })?;
            let after = canonical::canonicalize(before).song;
            rewrite_items(&mut set, id, |order| {
                let renamed = canonical::rename_tokens(before, &after, order);
                (renamed != order).then_some(Rewrite { order: Some(renamed), to: None })
            })
        }
        _ => false,
    };
    changed.then_some(set)
}

/// Bring the state in line with a plan that was written in full.
fn applied(state: &mut State, plan: &ChangePlan, written: Vec<WrittenSong>) -> Vec<Effect> {
    let mut effects = Vec::new();
    match &plan.action {
        PlanAction::SaveSong(id) => {
            if let Some(w) = written.into_iter().find(|w| w.id == *id) {
                effects.extend(super::save::apply(
                    state,
                    Command::SongSaved { id: id.clone(), saved_as: w.id, song: w.song, hash: w.hash, tags_kept: w.tags_kept },
                ));
            }
        }
        PlanAction::RenameSong { from, to } => {
            if let Some(draft) = state.drafts.remove(from) {
                state.drafts.insert(to.clone(), draft);
            }
            for list in [&mut state.saving, &mut state.save_conflicts] {
                for s in list.iter_mut().filter(|s| *s == from) {
                    *s = to.clone();
                }
            }
            if let Some(open) = state.song.take() {
                if open.id == *from {
                    show_song(state, to.clone(), open.file, open.hash);
                } else {
                    state.song = Some(open);
                }
            }
            if let Some(live) = state.output.live.as_mut().filter(|l| l.song == *from) {
                live.song = to.clone();
            }
            effects.extend(super::step(state, Command::RefreshLibrary));
        }
        PlanAction::DeleteSong(id) => {
            state.drafts.remove(id);
            state.saving.retain(|s| s != id);
            state.save_conflicts.retain(|s| s != id);
            if state.song.as_ref().is_some_and(|open| open.id == *id) {
                state.song = None;
            }
            effects.extend(super::step(state, Command::RefreshLibrary));
        }
        PlanAction::NewSong(id) => {
            effects.extend(super::step(state, Command::RefreshLibrary));
            effects.extend(super::step(state, Command::OpenSong(id.clone())));
        }
        PlanAction::SaveSetAs { from, to } => {
            // The new set holds the edits: open it (read back, with its hash); the old file is as
            // it was.
            if state.set.as_ref().is_some_and(|open| open.id == *from) {
                effects.extend(super::step(state, Command::OpenSet(to.clone())));
            }
            effects.push(Effect::ListSets);
        }
        PlanAction::DuplicateSet { .. } => effects.push(Effect::ListSets),
        // Files are rewritten in place; the watcher reloads the open song and set.
        PlanAction::TidyLibrary => {}
    }
    let files: Vec<PlanFile> = plan.steps.iter().map(PlanStep::file).collect();
    effects.extend(after_writes(state, plan, &files));
    effects
}

/// The open set follows what was written to its file, and which of its songs are missing now.
/// Its unsaved edits are never dropped: they stay on top of the rewritten file (with the plan's
/// change applied to them and to their undo history), and the set stays modified.
fn after_writes(state: &mut State, plan: &ChangePlan, files: &[PlanFile]) -> Vec<Effect> {
    let Some(open) = &mut state.set else { return Vec::new() };
    let written = plan.steps.iter().filter(|step| files.contains(&step.file())).find_map(|step| match step {
        PlanStep::WriteSet { id, set, .. } if *id == open.id => Some(Arc::clone(set)),
        _ => None,
    });
    let touches_songs = files.iter().any(|f| matches!(f, PlanFile::Song(_)));
    if written.is_none() && !touches_songs {
        return Vec::new();
    }
    if open.modified {
        // The song items refer to the song as the files now do.
        if let Some(set) = rewritten_like(plan, &open.set) {
            open.set = Arc::new(set);
        }
        for version in open.history.undo.iter_mut().chain(open.history.redo.iter_mut()) {
            if let Some(set) = rewritten_like(plan, version) {
                *version = Arc::new(set);
            }
        }
        if let Some(set) = written {
            // The file's new hash arrives with the watcher's reload, which adopts it.
            open.base = set;
        }
        open.modified = open.set != open.base;
    } else if let Some(set) = written {
        open.set = Arc::clone(&set);
        open.base = set;
    }
    let mut ids: Vec<SongId> = open.set.items.iter().filter_map(|item| item.song_id()).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() { Vec::new() } else { vec![Effect::CheckSetSongs { set: open.id.clone(), ids }] }
}
