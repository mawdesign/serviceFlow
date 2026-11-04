//! Seam 1, the engine: custom and image items advancing by themselves (`seconds`, `loop`,
//! `transition`), driven by a fake clock.

use std::time::Duration;

use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::set::ItemTransition;
use serviceflow_core::state::OutputMode;
use serviceflow_testkit::EngineHarness;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn item(name: &str, attributes: &str) -> String {
    format!(
        r#"<slide_group name="{name}" type="custom" {attributes}><slides><slide><body>{name}1</body></slide><slide><body>{name}2</body></slide><slide><body>{name}3</body></slide></slides></slide_group>"#
    )
}

fn open(items: &[String], settings: &[(&str, &str)]) -> EngineHarness {
    let set = format!(r#"<set name="S"><slide_groups>{}</slide_groups></set>"#, items.join("\n"));
    let mut harness = EngineHarness::with_data(&[("Song", "<song><lyrics>[V1]\n one</lyrics></song>")], &[("S", &set)], settings, &[]);
    harness.send(Command::ImportOpenSongSettings { trigger: serviceflow_core::command::ImportTrigger::FirstRun });
    harness.send(Command::OpenSet(SetId::new("S")));
    harness.send(Command::SelectItem(0));
    harness
}

fn shown(h: &EngineHarness) -> String {
    h.state().output.live.as_ref().map_or("(nothing)".to_owned(), |l| l.slide.lines.join("/"))
}

#[test]
fn slides_advance_after_the_seconds_and_stop_on_the_last() {
    let mut h = open(&[item("W", r#"seconds="10""#)], &[]);
    h.send(Command::GoLive { index: 0 });
    assert_eq!(h.state().auto_advance_due(), Some(secs(10)));
    h.advance(secs(9));
    assert_eq!(shown(&h), "W1", "not yet");
    h.advance(secs(1));
    assert_eq!(shown(&h), "W2");
    h.advance(secs(10));
    assert_eq!(shown(&h), "W3");
    assert_eq!(h.state().auto_advance_due(), None, "stopped on the last slide");
    h.advance(secs(100));
    assert_eq!(shown(&h), "W3", "and stays there");
}

#[test]
fn with_loop_it_goes_back_to_the_first_and_keeps_going() {
    let mut h = open(&[item("W", r#"seconds="5" loop="true""#)], &[]);
    h.send(Command::GoLive { index: 0 });
    let mut seen = Vec::new();
    for _ in 0..7 {
        h.advance(secs(5));
        seen.push(shown(&h));
    }
    assert_eq!(seen, ["W2", "W3", "W1", "W2", "W3", "W1", "W2"]);
}

#[test]
fn a_long_pause_catches_up_slide_by_slide() {
    let mut h = open(&[item("W", r#"seconds="5" loop="true""#)], &[]);
    h.send(Command::GoLive { index: 0 });
    h.advance(secs(5 * 4));
    assert_eq!(shown(&h), "W2", "four steps from W1 around three slides");
    assert_eq!(h.state().auto_advance_due(), Some(secs(25)));
}

#[test]
fn the_end_of_a_non_looping_item_does_not_run_into_the_next_item() {
    let mut h = open(&[item("W", r#"seconds="5""#), item("X", "")], &[]);
    h.send(Command::GoLive { index: 0 });
    h.advance(secs(500));
    assert_eq!(shown(&h), "W3");
    assert_eq!(h.state().set.as_ref().unwrap().selected, Some(0));
}

#[test]
fn an_item_without_seconds_never_advances() {
    let mut h = open(&[item("W", "")], &[]);
    h.send(Command::GoLive { index: 0 });
    assert_eq!(h.state().auto_advance_due(), None);
    h.advance(secs(1000));
    assert_eq!(shown(&h), "W1");
}

#[test]
fn manual_navigation_restarts_the_timer() {
    let mut h = open(&[item("W", r#"seconds="10""#)], &[]);
    h.send(Command::GoLive { index: 0 });
    h.advance(secs(8));
    h.send(Command::NextSlide);
    assert_eq!(shown(&h), "W2");
    h.advance(secs(8));
    assert_eq!(shown(&h), "W2", "ten seconds are counted from the manual step");
    h.advance(secs(2));
    assert_eq!(shown(&h), "W3");
    h.send(Command::GoLive { index: 0 });
    h.advance(secs(10));
    assert_eq!(shown(&h), "W2");
}

#[test]
fn blank_black_and_logo_cancel_it_and_showing_the_slide_again_restarts_it() {
    for toggle in [Command::ToggleBlank, Command::ToggleBlack, Command::ToggleLogo] {
        let mut h = open(&[item("W", r#"seconds="10""#)], &[]);
        h.send(Command::GoLive { index: 0 });
        h.advance(secs(5));
        h.send(toggle.clone());
        assert_eq!(h.state().auto_advance_due(), None, "{toggle:?}");
        h.advance(secs(100));
        assert_eq!(shown(&h), "W1", "{toggle:?}: nothing moved behind the blank");
        h.send(toggle);
        assert_eq!(h.state().output.mode, OutputMode::Normal);
        h.advance(secs(9));
        assert_eq!(shown(&h), "W1");
        h.advance(secs(1));
        assert_eq!(shown(&h), "W2", "ten seconds after it came back");
    }
}

#[test]
fn leaving_the_item_cancels_it() {
    let mut h = open(&[item("W", r#"seconds="10""#), item("X", "")], &[]);
    h.send(Command::GoLive { index: 0 });
    // Browsing to another item while W is live: W stops (it is no longer the open item).
    h.send(Command::SelectItem(1));
    h.advance(secs(10));
    assert_eq!(shown(&h), "W1");
    assert_eq!(h.state().auto_advance_due(), None);
    // Going live in X (no seconds) leaves nothing scheduled.
    h.send(Command::GoLive { index: 0 });
    assert_eq!(h.state().auto_advance_due(), None);
    h.advance(secs(1000));
    assert_eq!(shown(&h), "X1");
}

#[test]
fn stepping_into_an_item_starts_its_timer() {
    let mut h = open(&[item("X", ""), item("W", r#"seconds="3""#)], &[]);
    h.send(Command::GoLive { index: 2 });
    h.send(Command::NextSlide);
    assert_eq!(shown(&h), "W1");
    h.advance(secs(3));
    assert_eq!(shown(&h), "W2");
}

#[test]
fn the_item_decides_whether_its_slides_fade() {
    let transitions = r#"<present_settings><style transition="true" transition_time="400"/></present_settings>"#;
    let off = r#"<present_settings><style transition="false" transition_time="400"/></present_settings>"#;
    let items = [item("Def", ""), item("On", r#"transition="1""#), item("Off", r#"transition="2""#)];
    let fade = |settings: &str, index: usize| {
        let mut h = open(&items, &[("PresentSettings", settings)]);
        h.send(Command::SelectItem(index)).send(Command::GoLive { index: 0 });
        h.state().fade_ms()
    };
    assert_eq!(fade(transitions, 0), Some(400));
    assert_eq!(fade(transitions, 1), Some(400));
    assert_eq!(fade(transitions, 2), None, "transition 2 = no fade even when the output fades");
    assert_eq!(fade(off, 0), None);
    assert_eq!(fade(off, 1), Some(400), "transition 1 = fade even when the output does not");
}

#[test]
fn the_attributes_are_read_and_a_set_edit_changes_them() {
    let mut h = open(&[item("W", r#"seconds="10" loop="true" transition="2" link="x""#)], &[]);
    let set = h.state().set.as_ref().unwrap().set.clone();
    let serviceflow_core::set::ItemKind::Custom(custom) = &set.items[0].kind else { panic!("a custom item") };
    assert_eq!((custom.advance.seconds, custom.advance.looping, custom.advance.transition), (10, true, ItemTransition::Off));
    use serviceflow_core::set_edit::{ItemField, SetEdit};
    for (field, text) in [(ItemField::Seconds, "4"), (ItemField::Loop, "false"), (ItemField::Transition, "on")] {
        h.send(Command::EditSet(SetEdit::Field { item: 0, field, text: text.to_owned() }));
    }
    h.send(Command::SaveSet);
    let saved = String::from_utf8(h.set_bytes("S").unwrap()).unwrap();
    assert!(saved.contains(r#"seconds="4""#) && saved.contains(r#"loop="false""#) && saved.contains(r#"transition="1""#), "{saved}");
    assert!(saved.contains(r#"link="x""#), "link is kept, untouched: {saved}");
    // Undo is for edits only: the live actions around it left no trace.
    h.send(Command::GoLive { index: 0 }).send(Command::UndoSet);
    assert_eq!(h.saved_set("S").unwrap().items.len(), 1);
}
