//! Seam 1: importing OpenSong's styles through the engine.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

const MANUAL: Command = Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual };
const FIRST_RUN: Command = Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun };

#[test]
fn before_an_import_the_built_in_style_applies() {
    let harness = EngineHarness::with_fixtures();
    assert_eq!(*harness.state().styles, Default::default());
}

#[test]
fn importing_reads_the_fixture_styles_into_the_state() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(FIRST_RUN);
    let styles = harness.state().styles.clone();
    assert_eq!(styles.default.body.text.font, "Calibri");
    assert_eq!(styles.default.body.text.size, 108.0);
    assert_eq!(styles.scripture.body.text.size, 102.0);
    assert!(harness.state().problems.is_empty());
}

#[test]
fn importing_again_replaces_the_styles_and_never_writes_opensong_settings() {
    let mut harness = EngineHarness::with_fixtures();
    let before = harness.present_settings().expect("fixture settings");
    harness.send(FIRST_RUN).send(MANUAL);
    assert_eq!(harness.present_settings(), Some(before));
    assert_eq!(harness.state().styles.default.body.text.size, 108.0);
}

#[test]
fn a_re_import_picks_up_changes_made_in_opensong() {
    let xml = |size| format!(r#"<present_settings><default_style><body size="{size}"/></default_style></present_settings>"#);
    let mut harness = EngineHarness::with_present_settings(Some(&xml(20)));
    harness.send(MANUAL);
    assert_eq!(harness.state().styles.default.body.text.size, 60.0);
}

#[test]
fn a_data_root_without_present_settings_is_fine_on_first_run_but_reported_on_request() {
    let mut harness = EngineHarness::with_present_settings(None);
    harness.send(FIRST_RUN);
    assert!(harness.state().problems.is_empty());
    assert_eq!(*harness.state().styles, Default::default());
    harness.send(MANUAL);
    assert!(matches!(harness.state().problems.as_slice(), [Problem::StylesUnreadable { .. }]));
}

#[test]
fn unreadable_settings_are_reported_and_keep_the_current_styles() {
    let mut harness = EngineHarness::with_present_settings(Some("not xml <"));
    harness.send(FIRST_RUN);
    assert!(matches!(harness.state().problems.as_slice(), [Problem::StylesUnreadable { .. }]));
    assert_eq!(*harness.state().styles, Default::default());
}
