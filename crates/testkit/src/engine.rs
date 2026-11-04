//! Seam 1: the engine harness. A real engine, runtime and codec over in-memory storage.
//!
//! ```ignore
//! let mut harness = EngineHarness::with_fixtures();
//! harness.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
//! assert_eq!(harness.state().song.as_ref().unwrap().slides.len(), 12);
//! ```
//!
//! [`send`](EngineHarness::send) applies the command and runs every effect it causes to
//! completion, so the snapshot afterwards is final. Inspect what was written through
//! [`storage`](EngineHarness::storage).

use std::sync::Arc;

use serviceflow_core::command::Command;
use serviceflow_core::state::State;
use serviceflow_runtime::Driver;
use serviceflow_storage::{
    AssetRepository as _, DocumentId, Expect, SetRepository as _, SettingsId, SettingsRepository as _, SidecarRepository as _, SongId,
    SongRepository as _,
};
use serviceflow_storage_memory::MemoryStorage;

pub struct EngineHarness {
    driver: Driver<MemoryStorage>,
}

impl EngineHarness {
    /// An engine over a copy of the OpenSong fixtures data root (`tests/fixtures/opensong`).
    pub fn with_fixtures() -> Self {
        let storage = MemoryStorage::from_data_root(&crate::opensong_data_root()).expect("seeding storage from the OpenSong fixtures");
        Self::with_storage(storage)
    }

    /// An engine over storage holding only these songs, given as (song id, file contents).
    pub fn with_songs(songs: &[(&str, &str)]) -> Self {
        let storage = MemoryStorage::new();
        for (id, xml) in songs {
            crate::block_on(storage.write_song(&SongId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a song");
        }
        Self::with_storage(storage)
    }

    /// An engine over storage holding only these songs and sets, given as (id, file contents).
    pub fn with_songs_and_sets(songs: &[(&str, &str)], sets: &[(&str, &str)]) -> Self {
        let storage = MemoryStorage::new();
        for (id, xml) in songs {
            crate::block_on(storage.write_song(&SongId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a song");
        }
        for (id, xml) in sets {
            crate::block_on(storage.write_set(&serviceflow_storage::SetId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a set");
        }
        Self::with_storage(storage)
    }

    /// An engine over storage holding only these PresentSettings bytes (`None`: no settings).
    pub fn with_present_settings(bytes: Option<&str>) -> Self {
        let storage = MemoryStorage::new();
        if let Some(bytes) = bytes {
            crate::block_on(storage.write_settings(&SettingsId("PresentSettings".into()), bytes.as_bytes().to_vec(), Expect::Any))
                .expect("seeding settings");
        }
        Self::with_storage(storage)
    }

    /// An engine over storage holding these songs (id, contents) and settings files (filename
    /// under `Settings/`, contents).
    pub fn with_files(songs: &[(&str, &str)], settings: &[(&str, &str)]) -> Self {
        let storage = MemoryStorage::new();
        for (id, xml) in songs {
            crate::block_on(storage.write_song(&SongId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a song");
        }
        for (name, xml) in settings {
            crate::block_on(storage.write_settings(&SettingsId((*name).into()), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding settings");
        }
        Self::with_storage(storage)
    }

    /// An engine over storage holding these songs, sets, settings files (filename under
    /// `Settings/`) and background images (path under `Backgrounds/`, bytes).
    pub fn with_data(songs: &[(&str, &str)], sets: &[(&str, &str)], settings: &[(&str, &str)], backgrounds: &[(&str, &[u8])]) -> Self {
        let storage = MemoryStorage::new();
        for (id, xml) in songs {
            crate::block_on(storage.write_song(&SongId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a song");
        }
        for (id, xml) in sets {
            crate::block_on(storage.write_set(&serviceflow_storage::SetId::new(*id), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding a set");
        }
        for (name, xml) in settings {
            crate::block_on(storage.write_settings(&SettingsId((*name).into()), xml.as_bytes().to_vec(), Expect::Any)).expect("seeding settings");
        }
        for (name, bytes) in backgrounds {
            crate::block_on(storage.write_asset(&serviceflow_storage::AssetId((*name).into()), bytes.to_vec(), Expect::Any))
                .expect("seeding a background");
        }
        Self::with_storage(storage)
    }

    /// The bytes of `Settings/PresentSettings` in storage, to check that nothing wrote them.
    pub fn present_settings(&self) -> Option<Vec<u8>> {
        crate::block_on(self.storage().read_settings(&SettingsId("PresentSettings".into()))).ok().map(|s| s.bytes)
    }

    /// The bytes of a song file in storage (`None`: there is none).
    pub fn song_bytes(&self, id: &SongId) -> Option<Vec<u8>> {
        crate::block_on(self.storage().read_song(id)).ok().map(|s| s.bytes)
    }

    /// Every song id in storage.
    pub fn song_ids(&self) -> Vec<SongId> {
        crate::block_on(self.storage().list_songs()).expect("listing songs")
    }

    /// Write a song file behind the engine's back (another program or instance changing it).
    pub fn write_song_file(&self, id: &SongId, bytes: &[u8]) {
        crate::block_on(self.storage().write_song(id, bytes.to_vec(), Expect::Any)).expect("writing a song");
    }

    /// Remove a song file behind the engine's back.
    pub fn remove_song_file(&self, id: &SongId) {
        self.storage().remove(&serviceflow_storage::Key::Song(id.clone()));
    }

    /// Remove a set file behind the engine's back.
    pub fn remove_set_file(&self, id: &serviceflow_storage::SetId) {
        self.storage().remove(&serviceflow_storage::Key::Set(id.clone()));
    }

    /// Let the engine see what storage announced since the last command (changes made behind its
    /// back) and run everything that causes until idle.
    pub fn settle(&mut self) -> &mut Self {
        self.driver.pump();
        assert!(self.driver.is_idle(), "in-memory storage finished without the engine becoming idle");
        self
    }

    /// The bytes of a set file in storage (`None`: there is none).
    pub fn set_bytes(&self, set: &str) -> Option<Vec<u8>> {
        crate::block_on(self.storage().read_set(&serviceflow_storage::SetId::new(set))).ok().map(|s| s.bytes)
    }

    /// The bytes of a set's sidecar in storage (`None`: there is none).
    pub fn sidecar_bytes(&self, set: &str) -> Option<Vec<u8>> {
        let owner = DocumentId::Set(serviceflow_storage::SetId::new(set));
        crate::block_on(self.storage().read_sidecar(&owner)).ok().flatten().map(|s| s.bytes)
    }

    /// Write a set's sidecar behind the engine's back (a hand edit, a sync, or damage).
    pub fn write_sidecar_file(&self, set: &str, bytes: &[u8]) {
        let owner = DocumentId::Set(serviceflow_storage::SetId::new(set));
        crate::block_on(self.storage().write_sidecar(&owner, bytes.to_vec(), Expect::Any)).expect("writing a sidecar");
    }

    /// Quit and start again over the same storage: a new engine with empty state, as after
    /// restarting the app.
    pub fn restart(self) -> Self {
        Self::with_storage(self.driver.into_storage().expect("no effect is running"))
    }

    /// Write a set file behind the engine's back.
    pub fn write_set_file(&self, id: &str, bytes: &[u8]) {
        crate::block_on(self.storage().write_set(&serviceflow_storage::SetId::new(id), bytes.to_vec(), Expect::Any)).expect("writing a set");
    }

    pub fn with_storage(storage: MemoryStorage) -> Self {
        Self { driver: Driver::new(storage) }
    }

    /// Send a command and run everything it causes until the engine is idle.
    pub fn send(&mut self, command: Command) -> &mut Self {
        self.driver.send(command);
        self.driver.pump();
        assert!(self.driver.is_idle(), "in-memory storage finished without the engine becoming idle");
        self
    }

    /// The current state snapshot.
    pub fn state(&self) -> Arc<State> {
        self.driver.snapshot()
    }

    /// The storage, to check what was written.
    pub fn storage(&self) -> &MemoryStorage {
        self.driver.storage()
    }
}
