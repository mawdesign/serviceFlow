//! Test support for serviceFlow crates. Use only as a dev-dependency (`cargo xtask layers`).

use std::path::PathBuf;

pub mod storage_contract;

/// Run a future to completion on the current thread. Storage and engine futures are not `Send`.
pub use futures::executor::block_on;

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

/// The bytes of a fixture song, by its song filename under `tests/fixtures/opensong/Songs`.
pub fn fixture_song(filename: &str) -> Vec<u8> {
    let path = opensong_data_root().join("Songs").join(filename);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading fixture song {}: {e}", path.display()))
}
