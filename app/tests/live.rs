//! The operator's controls for going live, through the egui shell and the accessibility tree.
//! The logic itself is tested through the engine (`crates/core/tests/live.rs`); these check that
//! clicks and keys become the right commands. The output window needs a display and is not here.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable as _, by};
use serviceflow::ControlWindow;
use serviceflow::monitors::{MonitorChoice, MonitorInfo, Monitors};
use serviceflow::prefs::Prefs;
use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::state::{OutputMode, State};
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

fn window() -> ControlWindow<MemoryStorage> {
    let mut driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    driver.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    ControlWindow::new(driver)
}

type Seen = Rc<RefCell<Arc<State>>>;

fn harness(mut window: ControlWindow<MemoryStorage>) -> (Harness<'static>, Seen) {
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let harness = Harness::new_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    (harness, seen)
}

fn live_index(seen: &Seen) -> Option<usize> {
    seen.borrow().output.live.as_ref().map(|l| l.index)
}

#[test]
fn clicking_a_thumbnail_makes_that_slide_live() {
    let (mut harness, seen) = harness(window());
    harness.run();
    assert_eq!(live_index(&seen), None);
    harness.get_by_label_contains("Whatever my lot").click();
    harness.run();
    assert_eq!(live_index(&seen), Some(1));
}

#[test]
fn arrows_space_and_page_keys_move_through_the_slides() {
    let (mut harness, seen) = harness(window());
    harness.run();
    for (key, expected) in [
        (Key::ArrowRight, 0),
        (Key::Space, 1),
        (Key::PageDown, 2),
        (Key::ArrowDown, 3),
        (Key::ArrowLeft, 2),
        (Key::PageUp, 1),
        (Key::ArrowUp, 0),
        (Key::ArrowUp, 0),
    ] {
        harness.key_press(key);
        harness.run();
        assert_eq!(live_index(&seen), Some(expected), "after {key:?}");
    }
}

#[test]
fn escape_never_clears_the_output() {
    let (mut harness, seen) = harness(window());
    harness.run();
    harness.get_by_label_contains("Whatever my lot").click();
    harness.run();
    harness.key_press(Key::B);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Black);
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(live_index(&seen), Some(1));
    assert_eq!(seen.borrow().output.mode, OutputMode::Black);
}

#[test]
fn the_blank_and_black_buttons_and_keys_toggle_the_output_mode() {
    let (mut harness, seen) = harness(window());
    harness.run();
    harness.get_by_label("Blank").click();
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Blank);
    harness.get_by_label("Black").click();
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Black);
    harness.key_press(Key::B);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Normal);
    harness.key_press(Key::W);
    harness.run();
    assert_eq!(seen.borrow().output.mode, OutputMode::Blank);
}

#[test]
fn the_output_monitor_in_the_control_window_mirrors_what_is_live() {
    let (mut harness, _seen) = harness(window());
    harness.run();
    harness.get_by_label("Output monitor");
    harness.get_by_label_contains("Whatever my lot").click();
    harness.run();
    let textures = harness.ctx.tex_manager();
    assert_eq!(textures.read().allocated().filter(|(_, meta)| meta.name == "output monitor").count(), 1);
}

#[test]
fn choosing_an_output_monitor_is_saved_and_a_saved_choice_is_selected() {
    let monitor = |name: &str, x: i32| MonitorInfo { name: Some(name.into()), position: [x, 0], size: [1920, 1080] };
    let list = vec![monitor("Monitor #1", 0), monitor("Monitor #2", 1920), monitor("Monitor #3", 3840)];
    let saved: Rc<RefCell<Vec<Prefs>>> = Rc::default();
    let sink = Rc::clone(&saved);
    let monitors = Monitors::default();
    *monitors.lock().unwrap() = list.clone();
    let window = window().with_monitors(monitors).with_prefs(Prefs::default(), move |p| sink.borrow_mut().push(p.clone()));
    let (mut harness, _seen) = harness(window);
    harness.run();

    // With nothing remembered the second monitor is suggested.
    let combo = |value: &'static str| by().role(Role::ComboBox).value(value);
    harness.get(combo("2: Monitor #2 (1920x1080)")).click();
    harness.run();
    harness.get_by_label_contains("3: Monitor #3").click();
    harness.run();

    let saved = saved.borrow();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].output.monitor, Some(MonitorChoice::of(&list[2])));
}

#[test]
fn a_remembered_output_monitor_is_shown_as_selected() {
    let info = MonitorInfo { name: Some("Monitor #2".into()), position: [1920, 0], size: [1280, 720] };
    let monitors = Monitors::default();
    *monitors.lock().unwrap() = vec![MonitorInfo { name: Some("Monitor #1".into()), position: [0, 0], size: [1920, 1080] }, info.clone()];
    let mut prefs = Prefs::default();
    prefs.output.monitor = Some(MonitorChoice::of(&info));
    let (mut harness, _seen) = harness(window().with_monitors(monitors).with_prefs(prefs, |_| {}));
    harness.run();
    harness.get(by().role(Role::ComboBox).value("2: Monitor #2 (1280x720)"));
}
