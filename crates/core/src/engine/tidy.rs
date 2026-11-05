//! Tidy library (ADR-0002): one change plan that rewrites every song and set not yet in canonical
//! form. Storage is read first ([`Effect::GatherForTidy`]; only the codec can say whether a file's
//! bytes are canonical), the plan is made here (pure), shown as the dry-run list in
//! [`State::plan`] and written only after [`Command::ConfirmPlan`], like every change plan.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::plan::busy;
use crate::canonical;
use crate::command::{Command, Effect};
use crate::ids::{Expect, SongId};
use crate::lyrics;
use crate::plan::{ChangePlan, PlanAction, PlanFile, PlanRefusal, PlanRequest, PlanStep, TidyChange, TidyReport, TidySet, TidySkip, TidySong};
use crate::set::{ItemKind, Set};
use crate::song::Song;
use crate::state::{Problem, State};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::TidyLibrary => {
            if busy(state) {
                return Vec::new();
            }
            state.planning = Some(PlanRequest::Tidy);
            vec![Effect::GatherForTidy]
        }
        Command::TidyGathered { songs, sets, unreadable_songs, unreadable_sets, error } => {
            if state.planning != Some(PlanRequest::Tidy) {
                return Vec::new();
            }
            state.planning = None;
            if let Some(error) = error {
                state.problems.push(Problem::PlanRefused(PlanRefusal::Unreadable(error)));
                return Vec::new();
            }
            // The open set's edits would be overwritten by the rewritten file.
            if let Some(open) = state.set.as_ref().filter(|open| open.modified) {
                state.problems.push(Problem::PlanRefused(PlanRefusal::UnsavedSet(open.id.clone())));
                return Vec::new();
            }
            state.plan = Some(Arc::new(make(state, &songs, &sets, unreadable_songs, unreadable_sets)));
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn make(
    state: &State,
    songs: &[TidySong],
    sets: &[TidySet],
    unreadable_songs: Vec<(SongId, String)>,
    unreadable_sets: Vec<(crate::ids::SetId, String)>,
) -> ChangePlan {
    let mut report = TidyReport::default();
    let mut set_steps = Vec::new();
    let mut song_steps = Vec::new();
    let mut set_changes = Vec::new();
    let mut song_changes = Vec::new();
    // The songs that will be rewritten, as read and as they will be: set items name tags of these.
    let mut renamed: BTreeMap<SongId, (&Song, Song)> = BTreeMap::new();

    for tidy in songs {
        let file = PlanFile::Song(tidy.id.clone());
        let after = canonical::canonicalize(&tidy.song);
        if !tidy.unkept.is_empty() {
            // Never rewrite a file into one that has lost something.
            report.skipped.push((file, TidySkip::Unkept(tidy.unkept.clone())));
        } else if after.tags_kept || canonical::has_tag_collision(&tidy.song) {
            report.skipped.push((file, TidySkip::TagCollision));
        } else if state.drafts.get(&tidy.id).is_some_and(|draft| draft.is_modified()) {
            report.skipped.push((file, TidySkip::UnsavedEdits));
        } else {
            song_changes.push((file, describe_song(&tidy.song, &after.song)));
            song_steps.push(PlanStep::WriteSong { id: tidy.id.clone(), song: Arc::clone(&tidy.song), expect: Expect::Hash(tidy.hash) });
            renamed.insert(tidy.id.clone(), (&tidy.song, after.song));
        }
    }
    for (id, error) in unreadable_songs {
        report.skipped.push((PlanFile::Song(id), TidySkip::Unreadable(error)));
    }
    for (id, error) in unreadable_sets.iter() {
        report.skipped.push((PlanFile::Set(id.clone()), TidySkip::Unreadable(error.clone())));
    }

    for tidy in sets {
        let mut set: Set = (*tidy.set).clone();
        let mut items = Vec::new();
        for item in &mut set.items {
            let Some((before, after)) = item.song_id().and_then(|id| renamed.get(&id)) else { continue };
            let ItemKind::Song(song) = &mut item.kind else { continue };
            let order = canonical::rename_tokens(before, after, &song.presentation);
            if order != song.presentation {
                song.presentation = order;
                items.push(item.name.clone());
            }
        }
        if items.is_empty() && tidy.canonical {
            continue;
        }
        let file = PlanFile::Set(tidy.id.clone());
        if !tidy.unkept.is_empty() {
            // Left alone: its song items' orders still find the renamed tags (old tag names
            // resolve to the canonical ones), so the set keeps working.
            report.skipped.push((file, TidySkip::Unkept(tidy.unkept.clone())));
            continue;
        }
        let mut changes = Vec::new();
        if !items.is_empty() {
            changes.push(TidyChange::ItemOrders(items));
        }
        if !tidy.canonical {
            changes.push(TidyChange::Layout);
        }
        set_changes.push((file, changes));
        set_steps.push(PlanStep::WriteSet { id: tidy.id.clone(), set: Arc::new(set), expect: Expect::Hash(tidy.hash) });
    }

    // Sets first and the song last: applying the same tidy again finishes an interrupted one.
    report.changes = set_changes.into_iter().chain(song_changes).collect();
    report.skipped.sort_by(|a, b| a.0.cmp(&b.0));
    let steps = set_steps.into_iter().chain(song_steps).collect();
    let unreadable_sets = unreadable_sets.into_iter().map(|(id, _)| id).collect();
    ChangePlan { action: PlanAction::TidyLibrary, steps, users: Vec::new(), unreadable_sets, tidy: Some(report) }
}

/// What a tidy changes in a song, for the dry-run list.
fn describe_song(before: &Song, after: &Song) -> Vec<TidyChange> {
    let mut changes = Vec::new();
    let tags: Vec<(String, String)> = lyrics::parse(&before.lyrics)
        .0
        .iter()
        .zip(&after.sections)
        .filter(|(old, new)| old.tag != new.tag)
        .map(|(old, new)| (old.tag.clone(), new.tag.clone()))
        .collect();
    if !tags.is_empty() {
        changes.push(TidyChange::TagsRenamed(tags));
    }
    if before.presentation != after.presentation {
        changes.push(TidyChange::OrderRewritten);
    }
    if before.lyrics.lines().any(|line| line.trim_end() != line) {
        changes.push(TidyChange::TrailingSpaces);
    }
    if changes.is_empty() {
        changes.push(TidyChange::Layout);
    }
    changes
}
