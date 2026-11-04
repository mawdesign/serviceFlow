//! Seam 1, the engine: Tidy library (#21, ADR-0002). A dry-run list first, then one change plan;
//! tidying twice changes nothing and files already canonical are never written.

use serviceflow_core::command::Command;
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::{SetId, SongId};
use serviceflow_core::plan::{PlanAction, PlanFile, PlanRefusal, PlanStep, TidyChange, TidyReport, TidySkip};
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

/// Legacy tags, a trailing space and a presentation order naming the legacy tags.
const LEGACY: &str = "<song><title>Legacy</title><presentation>1 C 2</presentation>\
<lyrics>[1]\n First verse \n[C]\n Chorus\n[2]\n Second verse</lyrics></song>";
/// Two sections that would become one when renamed.
const CLASH: &str = "<song><title>Clash</title><lyrics>[1]\n One\n[V1]\n Also one</lyrics></song>";
const BROKEN: &str = "<song><title>Broken";

fn set_file(items: &[&str]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<set name=\"S\">\r\n  <slide_groups>\r\n");
    for item in items {
        out.push_str("    ");
        out.push_str(item);
        out.push_str("\r\n");
    }
    out.push_str("  </slide_groups>\r\n</set>\r\n");
    out
}

fn harness() -> EngineHarness {
    let ordered = set_file(&[r#"<slide_group name="Legacy" type="song" path="" presentation="2 C 1"/>"#]);
    let plain = set_file(&[r#"<slide_group name="Legacy" type="song" path=""/>"#]);
    let loose = "<set name=\"Loose\"><slide_groups><slide_group name=\"Other\" type=\"song\" path=\"\"/></slide_groups></set>";
    let mut harness = EngineHarness::with_songs_and_sets(
        &[("Legacy", LEGACY), ("Clash", CLASH), ("Broken", BROKEN)],
        &[("Ordered", &ordered), ("Plain", &plain), ("Loose", loose)],
    );
    // A song already in canonical form: tidy a one-song library and take its file.
    let canonical = {
        let mut h = EngineHarness::with_songs(&[("Other", "<song><title>Other</title><lyrics>[V1]\n Fine</lyrics></song>")]);
        h.send(Command::TidyLibrary).send(Command::ConfirmPlan);
        h.song_bytes(&SongId::new("Other")).unwrap()
    };
    harness.write_song_file(&SongId::new("Other"), &canonical);
    harness.settle();
    harness
}

fn report(harness: &EngineHarness) -> TidyReport {
    harness.state().plan.as_ref().expect("a plan is shown").tidy.clone().expect("a tidy report")
}

fn everything(harness: &EngineHarness) -> Vec<Option<Vec<u8>>> {
    let mut all: Vec<_> = ["Legacy", "Clash", "Broken", "Other"].iter().map(|id| harness.song_bytes(&SongId::new(*id))).collect();
    all.extend(["Ordered", "Plain", "Loose"].iter().map(|id| harness.set_bytes(id)));
    all
}

fn song(id: &str) -> PlanFile {
    PlanFile::Song(SongId::new(id))
}

fn set(id: &str) -> PlanFile {
    PlanFile::Set(SetId::new(id))
}

#[test]
fn the_dry_run_lists_every_file_not_in_canonical_form_with_what_changes_and_writes_nothing() {
    let mut harness = harness();
    let before = everything(&harness);
    harness.send(Command::TidyLibrary);

    let plan = harness.state().plan.clone().unwrap();
    assert_eq!(plan.action, PlanAction::TidyLibrary);
    let report = report(&harness);
    let files: Vec<_> = report.changes.iter().map(|(file, _)| file.clone()).collect();
    // Sets first, then the song; canonical files nothing renames are absent.
    assert_eq!(files, [set("Loose"), set("Ordered"), song("Legacy")]);
    let changes = |file: PlanFile| report.changes.iter().find(|(f, _)| *f == file).unwrap().1.clone();
    assert_eq!(
        changes(song("Legacy")),
        [
            TidyChange::TagsRenamed(vec![("1".into(), "V1".into()), ("2".into(), "V2".into())]),
            TidyChange::OrderRewritten,
            TidyChange::TrailingSpaces
        ]
    );
    assert_eq!(changes(set("Ordered")), [TidyChange::ItemOrders(vec!["Legacy".into()])]);
    assert_eq!(changes(set("Loose")), [TidyChange::Layout]);
    assert_eq!(plan.steps.len(), 3);
    assert!(matches!(plan.steps[2], PlanStep::WriteSong { .. }));
    assert_eq!(everything(&harness), before, "a dry run writes nothing");
}

#[test]
fn songs_with_tag_collisions_and_unreadable_files_are_skipped_and_reported() {
    let mut harness = harness();
    harness.send(Command::TidyLibrary);
    let report = report(&harness);
    assert_eq!(report.skipped.len(), 2);
    assert!(report.skipped.contains(&(song("Clash"), TidySkip::TagCollision)));
    assert!(report.skipped.iter().any(|(f, s)| *f == song("Broken") && matches!(s, TidySkip::Unreadable(_))));
    assert!(!report.changes.iter().any(|(f, _)| *f == song("Clash") || *f == song("Broken")));
}

#[test]
fn confirming_rewrites_the_files_and_their_sets_agree_with_the_renamed_tags() {
    let mut harness = harness();
    let before = everything(&harness);
    harness.send(Command::TidyLibrary).send(Command::ConfirmPlan);
    assert_eq!(harness.state().plan, None);
    assert!(harness.state().problems.is_empty(), "{:?}", harness.state().problems);

    let after = everything(&harness);
    let (legacy, ordered) = (String::from_utf8(after[0].clone().unwrap()).unwrap(), String::from_utf8(after[4].clone().unwrap()).unwrap());
    assert!(legacy.contains("[V1]") && legacy.contains("<presentation>V1 C V2</presentation>"), "{legacy}");
    assert!(!legacy.contains("First verse \n"), "{legacy}");
    assert!(ordered.contains(r#"presentation="V2 C V1""#), "{ordered}");
    // Skipped and already canonical files are untouched byte for byte.
    for index in [1, 2, 3] {
        assert_eq!(after[index], before[index]);
    }
}

#[test]
fn tidying_twice_changes_nothing_and_the_second_list_is_empty() {
    let mut harness = harness();
    harness.send(Command::TidyLibrary).send(Command::ConfirmPlan);
    let once = everything(&harness);

    harness.send(Command::TidyLibrary);
    let report = report(&harness);
    assert!(report.changes.is_empty(), "{:?}", report.changes);
    assert!(harness.state().plan.as_ref().unwrap().steps.is_empty());
    // The skipped files are still reported, so the operator knows why they were left.
    assert_eq!(report.skipped.len(), 2);
    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.state().plan, None);
    assert_eq!(everything(&harness), once);
}

#[test]
fn the_fixture_library_is_tidied_in_one_plan_and_is_then_already_tidy() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::TidyLibrary);
    let first = report(&harness);
    assert!(!first.changes.is_empty(), "the fixtures are legacy files");
    harness.send(Command::ConfirmPlan);
    assert!(harness.state().problems.is_empty(), "{:?}", harness.state().problems);

    harness.send(Command::TidyLibrary);
    assert!(report(&harness).changes.is_empty());
}

#[test]
fn a_song_with_unsaved_edits_is_skipped() {
    let mut harness = harness();
    harness.send(Command::OpenSong(SongId::new("Legacy")));
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, "Edited".to_owned())));
    harness.send(Command::TidyLibrary);
    let report = report(&harness);
    assert!(report.skipped.contains(&(song("Legacy"), TidySkip::UnsavedEdits)));
    // Its sets keep their orders too: the tags were not renamed.
    assert!(!report.changes.iter().any(|(f, c)| *f == set("Ordered") && c.iter().any(|c| matches!(c, TidyChange::ItemOrders(_)))));
}

#[test]
fn an_open_set_with_unsaved_edits_refuses_the_tidy() {
    let mut harness = harness();
    harness.send(Command::OpenSet(SetId::new("Plain")));
    harness.send(Command::EditSet(serviceflow_core::set_edit::SetEdit::Remove(0)));
    harness.send(Command::TidyLibrary);
    assert_eq!(harness.state().plan, None);
    assert_eq!(harness.state().problems.last(), Some(&Problem::PlanRefused(PlanRefusal::UnsavedSet(SetId::new("Plain")))));
}

#[test]
fn a_file_changed_after_the_list_was_shown_stops_the_tidy_before_anything_is_written() {
    let mut harness = harness();
    harness.send(Command::TidyLibrary);
    harness.write_song_file(&SongId::new("Legacy"), LEGACY.replace("Legacy", "Edited elsewhere").as_bytes());
    let before = everything(&harness);
    harness.send(Command::ConfirmPlan);
    assert!(matches!(harness.state().problems.last(), Some(Problem::PlanIncomplete { action: PlanAction::TidyLibrary, .. })));
    assert_eq!(everything(&harness), before);
}

#[test]
fn content_serviceflow_does_not_model_survives_a_tidy() {
    let song = "<song><title>Odd</title><alttheme>Warm</alttheme><custom_thing a=\"1\"><x>y</x></custom_thing>\
<lyrics>[1]\n Verse</lyrics></song>";
    let set = "<set name=\"S\" extra=\"kept\"><slide_groups><slide_group name=\"Odd\" type=\"song\" path=\"\" transition=\"1\"/></slide_groups></set>";
    let mut harness = EngineHarness::with_songs_and_sets(&[("Odd", song)], &[("S", set)]);
    harness.send(Command::TidyLibrary).send(Command::ConfirmPlan);
    let song = String::from_utf8(harness.song_bytes(&SongId::new("Odd")).unwrap()).unwrap();
    assert!(song.contains("<alttheme>Warm</alttheme>") && song.contains("<custom_thing a=\"1\"><x>y</x></custom_thing>"), "{song}");
    let set = String::from_utf8(harness.set_bytes("S").unwrap()).unwrap();
    assert!(set.contains("extra=\"kept\"") && set.contains("transition=\"1\""), "{set}");
}
