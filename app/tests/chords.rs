//! Chord toggles in the control window (#24). Which lines get chords is tested through the
//! engine and the layout (`crates/core/tests/chords.rs`, `crates/render/tests/chords.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<Arc<State>>>;

fn harness() -> (Harness<'static>, Seen) {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("Just A Closer Walk With Thee")));
    while !driver.is_idle() {
        driver.pump();
    }
    driver.send(Command::GoLive { index: 0 });
    let mut window = ControlWindow::new(driver);
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let harness = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    (harness, seen)
}

#[test]
fn each_output_has_its_own_chord_toggle_and_both_start_off() {
    let (mut harness, seen) = harness();
    harness.run();
    let chords = seen.borrow().output.chords;
    assert!(!chords.window && !chords.monitor);
    assert!(seen.borrow().output.live.as_ref().unwrap().slide.has_chords(), "the live slide has chords; outputs decide whether to draw them");

    harness.get_by_label("Chords on output").click();
    harness.run();
    let chords = seen.borrow().output.chords;
    assert!(chords.window && !chords.monitor);

    harness.get_by_label("Chords on monitor").click();
    harness.run();
    harness.get_by_label("Chords on output").click();
    harness.run();
    let chords = seen.borrow().output.chords;
    assert!(!chords.window && chords.monitor);
}

#[test]
fn control_window_thumbnails_are_drawn_from_slides_that_carry_their_chords() {
    let (mut harness, seen) = harness();
    harness.run();
    let state = seen.borrow();
    let open = state.song.as_ref().unwrap();
    assert!(open.slides.iter().all(|slide| slide.has_chords()));
    let textures = harness.ctx.tex_manager();
    assert_eq!(textures.read().allocated().filter(|(_, meta)| meta.name.starts_with("slide thumbnail")).count(), open.slides.len());
}
