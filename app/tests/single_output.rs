//! The browser's single full-screen output (#26), through the shell: with one screen, "Show
//! output" fills the window with the output, keys still drive it, and "Close output" brings the
//! control window back. The browser's full-screen request itself needs a real browser.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<(Arc<State>, bool)>>;

fn harness() -> (Harness<'static>, Seen) {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let mut window = ControlWindow::new(driver).with_single_output();
    let seen: Seen = Rc::new(RefCell::new((window.state(), false)));
    let sink = Rc::clone(&seen);
    let harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = (window.state(), window.output_open());
    });
    (harness, seen)
}

#[test]
fn showing_the_output_fills_the_window_and_closing_it_brings_the_controls_back() {
    let (mut harness, seen) = harness();
    harness.run();
    harness.get_by_label_contains("Whatever my lot").click();
    harness.run();
    harness.get_by_label("Show output").click();
    harness.run();
    assert!(seen.borrow().1, "the output is shown");
    // Only the output: no library, set or slide grid, and no live bar.
    assert!(harness.query_by_label("Library").is_none());
    assert!(harness.query_by_label_contains("Whatever my lot").is_none());
    assert!(harness.query_by_label("Show output").is_none());
    assert!(harness.query_by_label("Next").is_none());

    // The live shortcuts still work while the output fills the window.
    let before = seen.borrow().0.output.live.as_ref().map(|l| l.index);
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(seen.borrow().0.output.live.as_ref().map(|l| l.index), before.map(|i| i + 1));

    // Nothing but the slide until the pointer moves; then the way back shows.
    harness.event(egui::Event::PointerMoved(egui::pos2(600.0, 400.0)));
    harness.run();
    harness.get_by_label("Close output").click();
    harness.run();
    assert!(!seen.borrow().1);
    assert!(harness.query_by_label("Show output").is_some());
}

#[test]
fn the_desktop_keeps_the_controls_while_the_output_window_is_open() {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let mut window = ControlWindow::new(driver);
    let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| window.ui(ui));
    harness.run();
    harness.get_by_label("Show output").click();
    harness.run();
    assert!(harness.query_by_label("Close output").is_some());
    assert!(harness.query_by_label_contains("Whatever my lot").is_some(), "the slide grid stays");
}
