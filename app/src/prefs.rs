//! The operator's preferences that the shell remembers between sessions. The native `main` stores
//! them with craft-settings (`config.toml`); the browser build does not persist them yet.

use serde::{Deserialize, Serialize};

use crate::monitors::MonitorChoice;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub output: OutputPrefs,
    pub library: LibraryPrefs,
    /// The operator's changes to the default shortcuts.
    #[serde(with = "craft_keymap::keymap::overrides_list")]
    pub shortcuts: craft_keymap::Overrides,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryPrefs {
    /// The OpenSong data root last chosen by the operator.
    pub data_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputPrefs {
    /// The monitor the output window goes full screen on.
    pub monitor: Option<MonitorChoice>,
}

#[cfg(not(target_arch = "wasm32"))]
impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "serviceflow";
    const SCHEMA_VERSION: u32 = 1;

    fn migrate(_from: u32, _doc: &mut craft_settings::DocumentMut) -> Result<(), craft_settings::MigrateError> {
        Ok(())
    }
}
