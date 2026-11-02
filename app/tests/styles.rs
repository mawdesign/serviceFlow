//! Styles in the shell: imported on first run, and re-imported by a button. The import itself is
//! tested through the engine (`crates/core/tests/styles.rs`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use serviceflow::ControlWindow;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage_memory::MemoryStorage;
use serviceflow_testkit::opensong_data_root;

#[test]
fn the_styles_are_imported_on_first_run_and_the_button_imports_again() {
    let driver = Driver::new(MemoryStorage::from_data_root(&opensong_data_root()).unwrap());
    let mut window = ControlWindow::new(driver);
    let seen: Rc<RefCell<Arc<State>>> = Rc::new(RefCell::new(window.state()));
    let sink = Rc::clone(&seen);
    let mut harness = Harness::builder().allow_missing_glyphs().build_ui(move |ui| {
        window.ui(ui);
        *sink.borrow_mut() = window.state();
    });
    harness.run();
    let first = seen.borrow().styles.clone();
    assert_eq!(first.default.body.text.font, "Calibri");

    harness.get_by_label("Re-import styles from OpenSong").click();
    harness.run();
    let again = seen.borrow().styles.clone();
    assert_eq!(*again, *first);
    assert!(!Arc::ptr_eq(&again, &first), "the button read the settings again");
    assert!(seen.borrow().problems.is_empty());
}
