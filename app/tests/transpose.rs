//! The key bar in the control window (#25). What transposition does to chords is tested through
//! the engine (`crates/core/tests/transpose.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage::{Expect, SetRepository as _, SongId, SongRepository as _};
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::block_on;

const SONG: &str = "<song><title>Song</title><key>G</key><lyrics>[V1]\n.G C\n la la</lyrics></song>";
const SET: &str = r#"<set name="S"><slide_groups><slide_group name="Song" type="song"/></slide_groups></set>"#;

type Seen = Rc<RefCell<Arc<State>>>;

fn harness() -> (Harness<'static>, Seen) {
    let storage = MemoryStorage::new();
    block_on(storage.write_song(&SongId::new("Song"), SONG.as_bytes().to_vec(), Expect::Any)).unwrap();
    block_on(storage.write_set(&SetId::new("S"), SET.as_bytes().to_vec(), Expect::Any)).unwrap();
    let mut window = ControlWindow::new(Driver::new(storage));
    window.driver().send(Command::OpenSet(SetId::new("S")));
    window.driver().pump();
    window.driver().send(Command::SelectItem(0));
    window.driver().pump();
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let harness = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    (harness, seen)
}

fn chords(seen: &Seen) -> Vec<String> {
    seen.borrow().song.as_ref().unwrap().slides[0].chords[0].iter().map(|c| c.text.clone()).collect()
}

#[test]
fn the_key_bar_transposes_the_selected_item_and_saves_in_the_new_key() {
    let (mut harness, seen) = harness();
    harness.run();
    assert_eq!(chords(&seen), ["G", "C"]);
    assert_eq!(seen.borrow().key_info().unwrap().original, "G");

    harness.get_by_label("+").click();
    harness.run();
    harness.get_by_label("+").click();
    harness.run();
    assert_eq!(chords(&seen), ["A", "D"]);
    assert_eq!(seen.borrow().key_info().unwrap().sounding.as_deref(), Some("A"));

    harness.get_by_label("Save in this key").click();
    for _ in 0..5 {
        harness.run();
    }
    assert_eq!(chords(&seen), ["A", "D"]);
    let info = seen.borrow().key_info().unwrap();
    assert_eq!((info.original.as_str(), info.semitones), ("A", 0));
}
