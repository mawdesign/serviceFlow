//! Seam 1, the engine: change plans across files (#20). Every file an action writes is listed
//! first; nothing is written before the operator confirms; writes are conditional on hashes.

use serviceflow_core::command::Command;
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::ids::{Expect, SetId, SongId};
use serviceflow_core::plan::{PlanAction, PlanFile, PlanRefusal, PlanStep};
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

const HYMN: &str = "<song><title>Hymn</title><presentation>1 C 2</presentation>\
<lyrics>[1]\n First verse\n[C]\n Chorus\n[2]\n Second verse</lyrics></song>";
const OTHER: &str = "<song><title>Other</title><lyrics>[1]\n Other verse</lyrics></song>";

/// A set as serviceFlow writes it, so the expected bytes after a change are easy to state.
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

fn sunday() -> String {
    set_file(&[
        r#"<slide_group name="Hymn" type="song" path="" presentation="2 C 1" transition="1"/>"#,
        r#"<slide_group name="Other" type="song" path="" presentation="1"/>"#,
        r#"<slide_group name="Psalm" type="scripture"><title>Psalm 23</title></slide_group>"#,
    ])
}

fn plain() -> String {
    set_file(&[r#"<slide_group name="Hymn" type="song" path=""/>"#])
}

fn unrelated() -> String {
    set_file(&[r#"<slide_group name="Other" type="song" path="" presentation="1"/>"#])
}

fn harness() -> EngineHarness {
    let sets = [("Sunday", sunday()), ("Plain", plain()), ("Unrelated", unrelated())];
    let sets: Vec<(&str, &str)> = sets.iter().map(|(id, xml)| (*id, xml.as_str())).collect();
    let mut harness = EngineHarness::with_songs_and_sets(&[("Hymn", HYMN), ("Other", OTHER)], &sets);
    harness.send(Command::OpenSong(SongId::new("Hymn")));
    harness
}

fn text(bytes: Option<Vec<u8>>) -> String {
    String::from_utf8(bytes.expect("the file exists")).unwrap()
}

fn files(harness: &EngineHarness) -> Vec<PlanFile> {
    harness.state().plan.as_ref().expect("a plan is shown").steps.iter().map(PlanStep::file).collect()
}

fn snapshot(harness: &EngineHarness) -> Vec<Option<Vec<u8>>> {
    let mut all: Vec<_> = ["Hymn", "Other", "Hymn.xml", "Hymns/Hymn.xml"].iter().map(|id| harness.song_bytes(&SongId::new(*id))).collect();
    all.extend(["Sunday", "Plain", "Unrelated"].iter().map(|id| harness.set_bytes(id)));
    all
}

fn edit_title(harness: &mut EngineHarness, title: &str) {
    harness.send(Command::EditSong(SongEdit::Field(SongField::Title, title.to_owned())));
}

#[test]
fn a_save_that_renames_tags_lists_the_song_and_the_sets_whose_order_names_them() {
    let mut harness = harness();
    edit_title(&mut harness, "Hymn (edited)");
    let before = snapshot(&harness);
    harness.send(Command::SaveSong(SongId::new("Hymn")));

    // Nothing is written until the operator confirms.
    assert_eq!(snapshot(&harness), before);
    let plan = harness.state().plan.clone().unwrap();
    assert_eq!(plan.action, PlanAction::SaveSong(SongId::new("Hymn")));
    // Sets first, the song last; "Plain" has no order and "Unrelated" does not use the song.
    assert_eq!(files(&harness), [PlanFile::Set(SetId::new("Sunday")), PlanFile::Song(SongId::new("Hymn"))]);
    assert!(matches!(plan.steps[0], PlanStep::WriteSet { expect: Expect::Hash(_), .. }));
    assert!(matches!(plan.steps[1], PlanStep::WriteSong { expect: Expect::Hash(_), .. }));

    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.state().plan, None);
    assert_eq!(
        text(harness.set_bytes("Sunday")),
        set_file(&[
            r#"<slide_group name="Hymn" type="song" path="" presentation="V2 C V1" transition="1"/>"#,
            r#"<slide_group name="Other" type="song" path="" presentation="1"/>"#,
            r#"<slide_group name="Psalm" type="scripture"><title>Psalm 23</title></slide_group>"#,
        ])
    );
    assert_eq!(text(harness.set_bytes("Plain")), plain());
    assert_eq!(text(harness.set_bytes("Unrelated")), unrelated());
    let song = text(harness.song_bytes(&SongId::new("Hymn")));
    assert!(song.contains("<title>Hymn (edited)</title>"), "{song}");
    assert!(song.contains("<presentation>V1 C V2</presentation>"), "{song}");
    assert!(song.contains("[V1]\r\n First verse\r\n[C]\r\n Chorus\r\n[V2]\r\n Second verse"), "{song}");
    assert!(!harness.state().draft().unwrap().is_modified());
}

#[test]
fn cancelling_a_plan_writes_nothing_and_keeps_the_edits() {
    let mut harness = harness();
    edit_title(&mut harness, "Changed");
    let before = snapshot(&harness);
    harness.send(Command::SaveSong(SongId::new("Hymn")));
    harness.send(Command::CancelPlan);
    assert_eq!(harness.state().plan, None);
    assert_eq!(snapshot(&harness), before);
    assert!(harness.state().draft().unwrap().is_modified());
    assert!(harness.state().saving.is_empty());
    // It can be saved again later.
    harness.send(Command::SaveSong(SongId::new("Hymn")));
    assert!(harness.state().plan.is_some());
}

#[test]
fn a_save_whose_tag_renames_no_set_refers_to_is_written_without_a_plan() {
    let mut harness = EngineHarness::with_songs_and_sets(&[("Hymn", HYMN)], &[("Plain", &plain())]);
    harness.send(Command::OpenSong(SongId::new("Hymn")));
    edit_title(&mut harness, "Changed");
    harness.send(Command::SaveSong(SongId::new("Hymn")));
    assert_eq!(harness.state().plan, None);
    assert!(text(harness.song_bytes(&SongId::new("Hymn"))).contains("[V1]"));
    assert_eq!(text(harness.set_bytes("Plain")), plain());
}

#[test]
fn editing_a_title_never_renames_the_file() {
    let mut harness = harness();
    edit_title(&mut harness, "A Completely New Title");
    harness.send(Command::SaveSong(SongId::new("Hymn")));
    harness.send(Command::ConfirmPlan);
    assert!(harness.song_bytes(&SongId::new("Hymn")).is_some());
    assert_eq!(harness.song_ids(), [SongId::new("Hymn"), SongId::new("Other")]);
}

#[test]
fn renaming_a_song_file_moves_it_and_rewrites_every_referring_set_item() {
    let mut harness = harness();
    let song_before = harness.song_bytes(&SongId::new("Hymn"));
    let before = snapshot(&harness);
    let to = SongId::new("Hymns/Hymn.xml");
    harness.send(Command::RenameSong { from: SongId::new("Hymn"), to: to.clone() });
    assert_eq!(snapshot(&harness), before);
    assert_eq!(files(&harness), [PlanFile::Set(SetId::new("Plain")), PlanFile::Set(SetId::new("Sunday")), PlanFile::Song(SongId::new("Hymn"))]);
    assert!(matches!(&harness.state().plan.as_ref().unwrap().steps[2], PlanStep::RenameSong { to: t, .. } if *t == to));

    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.song_bytes(&SongId::new("Hymn")), None);
    // Same bytes: renaming a file never changes its title.
    assert_eq!(harness.song_bytes(&to), song_before);
    assert_eq!(
        text(harness.set_bytes("Sunday")),
        set_file(&[
            r#"<slide_group name="Hymn.xml" type="song" path="Hymns/" presentation="2 C 1" transition="1"/>"#,
            r#"<slide_group name="Other" type="song" path="" presentation="1"/>"#,
            r#"<slide_group name="Psalm" type="scripture"><title>Psalm 23</title></slide_group>"#,
        ])
    );
    assert_eq!(text(harness.set_bytes("Plain")), set_file(&[r#"<slide_group name="Hymn.xml" type="song" path="Hymns/"/>"#]));
    assert_eq!(text(harness.set_bytes("Unrelated")), unrelated());
    // The open song follows its file.
    let state = harness.state();
    assert_eq!(state.song.as_ref().unwrap().id, to);
    assert!(state.library.songs.contains(&to));
}

#[test]
fn renaming_onto_a_taken_name_is_refused_and_writes_nothing() {
    let mut harness = harness();
    let before = snapshot(&harness);
    harness.send(Command::RenameSong { from: SongId::new("Hymn"), to: SongId::new("other") });
    assert_eq!(harness.state().plan, None);
    assert_eq!(harness.state().problems.last(), Some(&Problem::PlanRefused(PlanRefusal::NameTaken(SongId::new("other")))));
    harness.send(Command::RenameSong { from: SongId::new("Hymn"), to: SongId::new("_Hidden/Hymn") });
    assert_eq!(harness.state().problems.last(), Some(&Problem::PlanRefused(PlanRefusal::BadName(SongId::new("_Hidden/Hymn")))));
    assert_eq!(snapshot(&harness), before);
}

#[test]
fn deleting_a_song_sets_still_use_warns_with_those_sets_first() {
    let mut harness = harness();
    let before = snapshot(&harness);
    harness.send(Command::DeleteSong(SongId::new("Hymn")));
    assert_eq!(snapshot(&harness), before);
    let plan = harness.state().plan.clone().unwrap();
    assert_eq!(plan.users, [SetId::new("Plain"), SetId::new("Sunday")]);
    assert_eq!(files(&harness), [PlanFile::Song(SongId::new("Hymn"))]);

    harness.send(Command::ConfirmPlan);
    assert_eq!(harness.song_bytes(&SongId::new("Hymn")), None);
    assert_eq!(text(harness.set_bytes("Sunday")), sunday());
    assert_eq!(harness.state().song, None);
    assert!(!harness.state().library.songs.contains(&SongId::new("Hymn")));
}

#[test]
fn a_new_song_is_named_from_its_sanitised_title_with_a_suffix_when_taken() {
    let mut harness =
        EngineHarness::with_songs(&[("Amazing Grace.xml", OTHER), ("Hymns/Amazing Grace.xml", OTHER), ("Hymns/amazing grace (2).xml", OTHER)]);
    harness.send(Command::NewSong { category: String::new(), title: "Be Still".into() });
    assert_eq!(files(&harness), [PlanFile::Song(SongId::new("Be Still.xml"))]);
    harness.send(Command::CancelPlan);

    harness.send(Command::NewSong { category: "Hymns".into(), title: "  Amazing: Grace?  ".into() });
    let id = SongId::new("Hymns/Amazing Grace (3).xml");
    assert_eq!(files(&harness), [PlanFile::Song(id.clone())]);
    assert!(harness.song_bytes(&id).is_none());
    harness.send(Command::ConfirmPlan);
    assert_eq!(
        text(harness.song_bytes(&id)),
        [
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<song>",
            "  <title>Amazing: Grace?</title>",
            "  <author></author>",
            "  <copyright></copyright>",
            "  <hymn_number></hymn_number>",
            "  <presentation></presentation>",
            "  <ccli></ccli>",
            "  <capo></capo>",
            "  <key></key>",
            "  <aka></aka>",
            "  <key_line></key_line>",
            "  <user1></user1>",
            "  <user2></user2>",
            "  <user3></user3>",
            "  <theme></theme>",
            "  <tempo></tempo>",
            "  <time_sig></time_sig>",
            "  <lyrics></lyrics>",
            "</song>",
            "",
        ]
        .join("\r\n")
    );
    // The new song is opened.
    assert_eq!(harness.state().song.as_ref().unwrap().id, id);
}

#[test]
fn a_file_changed_after_the_plan_was_made_stops_it_before_anything_is_written() {
    let mut harness = harness();
    edit_title(&mut harness, "Changed");
    harness.send(Command::SaveSong(SongId::new("Hymn")));
    // OpenSong saves the song meanwhile: the set would be fine, but the song is not.
    harness.write_song_file(&SongId::new("Hymn"), HYMN.replace("Hymn", "Theirs").as_bytes());
    let before = snapshot(&harness);
    harness.send(Command::ConfirmPlan);
    assert_eq!(snapshot(&harness), before);
    let Some(Problem::PlanIncomplete { action, failure }) = harness.state().problems.last().cloned() else { panic!("a problem") };
    assert_eq!(action, PlanAction::SaveSong(SongId::new("Hymn")));
    assert_eq!(failure.written, []);
    assert_eq!(failure.failed, PlanFile::Song(SongId::new("Hymn")));
    assert!(failure.conflict);
    assert!(harness.state().draft().unwrap().is_modified());
    assert!(harness.state().saving.is_empty());
}

#[test]
fn a_set_changed_after_a_rename_was_planned_stops_it_and_the_song_keeps_its_name() {
    let mut harness = harness();
    harness.send(Command::RenameSong { from: SongId::new("Hymn"), to: SongId::new("Renamed") });
    harness.write_set_file("Sunday", sunday().replace("Psalm 23", "Psalm 24").as_bytes());
    harness.send(Command::ConfirmPlan);
    assert!(harness.song_bytes(&SongId::new("Hymn")).is_some());
    assert!(harness.song_bytes(&SongId::new("Renamed")).is_none());
    assert_eq!(text(harness.set_bytes("Plain")), plain());
    let Some(Problem::PlanIncomplete { failure, .. }) = harness.state().problems.last().cloned() else { panic!("a problem") };
    assert_eq!(failure.failed, PlanFile::Set(SetId::new("Sunday")));
}
