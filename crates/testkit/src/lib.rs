//! Test support for serviceFlow crates. Use only as a dev-dependency (`cargo xtask layers`).

use std::path::PathBuf;

/// The workspace `tests/fixtures` folder, located relative to this crate's manifest so tests
/// work from any crate and any working directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// An OpenSong data root (`Songs/`, `Settings/`, …) made of real OpenSong data:
/// `tests/fixtures/opensong`.
pub fn opensong_data_root() -> PathBuf {
    fixtures_dir().join("opensong")
}
