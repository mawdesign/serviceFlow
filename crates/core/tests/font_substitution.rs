//! Seam 1, the engine: a font that had to be substituted is reported in the state, once.

use serviceflow_core::command::Command;
use serviceflow_core::state::Problem;
use serviceflow_testkit::EngineHarness;

fn substituted(requested: &str, used: &str) -> Command {
    Command::FontSubstituted { requested: requested.into(), used: used.into() }
}

#[test]
fn a_substituted_font_becomes_a_problem() {
    let mut harness = EngineHarness::with_songs(&[]);
    harness.send(substituted("Calibri", "Carlito"));
    assert_eq!(harness.state().problems, [Problem::FontSubstituted { requested: "Calibri".into(), used: "Carlito".into() }]);
}

#[test]
fn reporting_the_same_substitution_again_changes_nothing() {
    let mut harness = EngineHarness::with_songs(&[]);
    harness.send(substituted("Calibri", "Carlito"));
    harness.send(substituted("Calibri", "Carlito"));
    harness.send(substituted("Arial", "Liberation Sans"));
    assert_eq!(harness.state().problems.len(), 2);
}
