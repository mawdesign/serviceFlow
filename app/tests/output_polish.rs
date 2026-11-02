//! Output polish through the shell: the logo from OpenSong's settings, the L key and button, and
//! the initial output monitor. The rules themselves are tested through the engine
//! (`crates/core/tests/output_settings.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow::monitors::{MonitorInfo, Monitors};
use serviceflow_core::command::Command;
use serviceflow_core::state::{OutputMode, State};
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

type Seen = Rc<RefCell<Arc<State>>>;

fn harness() -> (Harness<'static>, Seen) {
    let driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    let mut window = ControlWindow::new(driver);
    let seen: Seen = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    (harness, seen)
}

fn mode(seen: &Seen) -> OutputMode {
    seen.borrow().output.mode
}

#[test]
fn the_window_starts_in_logo_mode_with_the_imported_logo() {
    let (mut harness, seen) = harness();
    harness.run();
    assert_eq!(mode(&seen), OutputMode::Logo, "PresentSettings initial_mode is L");
    assert!(seen.borrow().output_settings.logo.is_some());
}

#[test]
fn the_l_key_and_the_logo_button_toggle_logo_mode() {
    let (mut harness, seen) = harness();
    harness.run();
    harness.key_press(Key::L);
    harness.run();
    assert_eq!(mode(&seen), OutputMode::Normal);
    harness.get_by_label("Logo").click();
    harness.run();
    assert_eq!(mode(&seen), OutputMode::Logo);
}

#[test]
fn the_logo_is_painted_into_the_output_monitor() {
    let (mut harness, _seen) = harness();
    harness.run();
    let textures = harness.ctx.tex_manager();
    let manager = textures.read();
    assert!(manager.allocated().any(|(_, meta)| meta.name == "output monitor"), "the mirror drew logo mode");
}

#[test]
fn the_opensong_monitor_is_used_until_the_operator_chooses() {
    let monitors: Monitors = Monitors::default();
    *monitors.lock().unwrap() = vec![
        MonitorInfo { name: Some("A".into()), position: [0, 0], size: [1920, 1080] },
        MonitorInfo { name: Some("B".into()), position: [1920, 0], size: [1920, 1080] },
        MonitorInfo { name: Some("C".into()), position: [3840, 0], size: [1280, 1024] },
    ];
    let driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    let mut window = ControlWindow::new(driver).with_monitors(monitors);
    let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).allow_missing_glyphs().build_ui(move |ui| window.ui(ui));
    harness.run();
    // PresentSettings monitors present="2": the second screen (index 1).
    harness.get_by_value("2: B (1920x1080)");
}

#[test]
fn a_command_to_toggle_the_logo_is_a_live_action() {
    assert!(Command::ToggleLogo.is_live_action());
}
