//! The language mode toggle in the control window (#11). The pairing and the toggle's effect on
//! slides are tested through the engine (`crates/core/tests/multilanguage.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::LanguageMode;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<Arc<State>>>;

fn harness() -> (Harness<'static>, Seen) {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("一件礼物 A Gift")));
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

/// Open the language combo box (its value is the current mode) and pick `label`.
fn choose(harness: &mut Harness<'static>, label: &str) {
    harness.get(by().role(Role::ComboBox).value(current(harness))).click();
    harness.run();
    harness.get_by_label(label).click();
}

fn current(harness: &Harness<'static>) -> &'static str {
    ["Both languages", "Primary only", "Translation only"]
        .into_iter()
        .find(|l| harness.query(by().role(Role::ComboBox).value(l)).is_some())
        .expect("the language combo box")
}

#[test]
fn the_language_buttons_switch_what_multilanguage_slides_show() {
    let (mut harness, seen) = harness();
    harness.run();
    assert!(seen.borrow().output.live.as_ref().expect("live").slide.multilanguage);

    choose(&mut harness, "Primary only");
    harness.run();
    assert_eq!(seen.borrow().slide_options.language, LanguageMode::Primary);
    assert_eq!(seen.borrow().output.live.as_ref().unwrap().slide.lines.len(), 4);

    choose(&mut harness, "Translation only");
    harness.run();
    assert_eq!(seen.borrow().output.live.as_ref().unwrap().slide.lines[0], "There is a gift, have you received yet?");

    choose(&mut harness, "Both languages");
    harness.run();
    assert_eq!(seen.borrow().slide_options.language, LanguageMode::Both);
    assert!(seen.borrow().output.live.as_ref().unwrap().slide.multilanguage);
}
