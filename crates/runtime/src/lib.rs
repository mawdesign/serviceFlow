//! The runtime: owns the [`Engine`] and a storage, performs the engine's [`Effect`]s, and feeds
//! their outcomes back in as commands (ADR-0001, ADR-0004).
//!
//! It is executor-agnostic and single-threaded: effects run as local futures on a
//! [`LocalPool`] that [`Driver::pump`] advances without blocking. The app pumps once per frame;
//! tests pump until idle. Nothing here blocks, so the same code runs in the browser.

use std::rc::Rc;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::executor::{LocalPool, LocalSpawner};
use futures::task::LocalSpawnExt as _;
use serviceflow_core::command::{Command, Effect, SettingsFault};
use serviceflow_core::engine::Engine;
use serviceflow_core::song::Song;
use serviceflow_core::state::State;
use serviceflow_storage::{Expect, SongId, Storage};

/// Runs the engine against a storage.
pub struct Driver<S> {
    engine: Engine,
    storage: Rc<S>,
    pool: LocalPool,
    spawner: LocalSpawner,
    results: mpsc::UnboundedReceiver<Command>,
    results_sender: mpsc::UnboundedSender<Command>,
    in_flight: usize,
}

impl<S: Storage + 'static> Driver<S> {
    pub fn new(storage: S) -> Self {
        let pool = LocalPool::new();
        let spawner = pool.spawner();
        let (results_sender, results) = mpsc::unbounded();
        Self { engine: Engine::new(), storage: Rc::new(storage), pool, spawner, results, results_sender, in_flight: 0 }
    }

    /// The current state snapshot.
    pub fn snapshot(&self) -> Arc<State> {
        self.engine.snapshot()
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Apply a command now and start the IO it asks for. Results arrive on later [`pump`]s.
    ///
    /// [`pump`]: Driver::pump
    pub fn send(&mut self, command: Command) {
        for effect in self.engine.apply(command) {
            self.start(effect);
        }
    }

    /// Advance in-flight effects as far as they can go without waiting, applying every result
    /// command (and starting any effects those ask for). Never blocks.
    pub fn pump(&mut self) {
        loop {
            self.pool.run_until_stalled();
            let mut progressed = false;
            while let Ok(command) = self.results.try_recv() {
                self.in_flight -= 1;
                progressed = true;
                self.send(command);
            }
            if !progressed {
                break;
            }
        }
    }

    /// `true` when no effect is still waiting on storage.
    pub fn is_idle(&self) -> bool {
        self.in_flight == 0
    }

    fn start(&mut self, effect: Effect) {
        let storage = Rc::clone(&self.storage);
        let sender = self.results_sender.clone();
        self.in_flight += 1;
        let task = async move {
            let command = perform(&*storage, effect).await;
            // The receiver lives as long as the driver, which owns the pool running this task.
            let _ = sender.unbounded_send(command);
        };
        if self.spawner.spawn_local(task).is_err() {
            // Only possible once the pool is gone, which cannot happen while `self` exists.
            self.in_flight -= 1;
        }
    }
}

/// Read and parse one file of `Settings/`; a failure says which file and whether it was missing.
async fn read_settings_file<S: Storage, T>(
    storage: &S,
    file: &str,
    parse: fn(&[u8]) -> Result<T, serviceflow_opensong::CodecError>,
) -> Result<T, SettingsFault> {
    let fault = |error: String, missing| SettingsFault { file: file.to_owned(), error, missing };
    match storage.read_settings(&serviceflow_storage::SettingsId(file.to_owned())).await {
        Ok(stored) => parse(&stored.bytes).map_err(|e| fault(e.to_string(), false)),
        Err(e) => Err(fault(e.to_string(), matches!(e, serviceflow_storage::StorageError::NotFound(_)))),
    }
}

/// How many copy names a save-as-copy tries before giving up.
const COPY_NAMES: usize = 100;

/// Write a song in canonical form (ADR-0002) and read the written bytes back, so the engine holds
/// exactly what the file now says. A copy goes to the first copy name not taken.
async fn save_song<S: Storage>(storage: &S, id: SongId, song: &Song, expect: Expect, as_copy: bool) -> Command {
    use serviceflow_storage::StorageError;
    let tags_kept = serviceflow_core::canonical::canonicalize(song).tags_kept;
    let bytes = serviceflow_opensong::write_song(song);
    let saved = |saved_as: SongId, hash| match serviceflow_opensong::read_song(&bytes) {
        Ok(read) => Command::SongSaved { id: id.clone(), saved_as, song: Arc::new(read), hash, tags_kept },
        Err(e) => Command::SongSaveFailed { id: id.clone(), error: format!("written but unreadable: {e}") },
    };
    if as_copy {
        for n in 1..=COPY_NAMES {
            let copy = id.copy(n);
            match storage.write_song(&copy, bytes.clone(), Expect::Absent).await {
                Ok(hash) => return saved(copy, hash),
                Err(StorageError::Conflict(_)) => continue,
                Err(e) => return Command::SongSaveFailed { id, error: e.to_string() },
            }
        }
        return Command::SongSaveFailed { id, error: "no free copy name".to_owned() };
    }
    match storage.write_song(&id, bytes.clone(), expect).await {
        Ok(hash) => saved(id.clone(), hash),
        // Changed or removed since it was read: the operator decides.
        Err(StorageError::Conflict(_) | StorageError::NotFound(_)) => Command::SongSaveConflict { id },
        Err(e) => Command::SongSaveFailed { id, error: e.to_string() },
    }
}

/// Perform one effect and return the command that reports its outcome.
pub async fn perform<S: Storage>(storage: &S, effect: Effect) -> Command {
    match effect {
        Effect::ListLibrary { generation } => match storage.list_songs().await {
            Ok(songs) => Command::LibraryListed { generation, songs },
            Err(e) => Command::LibraryListFailed { generation, error: e.to_string() },
        },
        Effect::IndexSongs { generation, ids } => {
            let mut songs = Vec::with_capacity(ids.len());
            for id in ids {
                let song = match storage.read_song(&id).await {
                    Ok(stored) => serviceflow_opensong::read_song(&stored.bytes).ok().map(Arc::new),
                    Err(_) => None,
                };
                songs.push((id, song));
            }
            Command::SongsIndexed { generation, songs }
        }
        Effect::ReadOpenSongStyles(trigger) => {
            let id = serviceflow_storage::SettingsId("PresentSettings".to_owned());
            match storage.read_settings(&id).await {
                Ok(stored) => match serviceflow_opensong::read_present_settings(&stored.bytes) {
                    Ok(styles) => Command::StylesImported(Arc::new(styles)),
                    Err(e) => Command::StylesImportFailed { trigger, error: e.to_string(), missing: false },
                },
                Err(e) => Command::StylesImportFailed {
                    trigger,
                    error: e.to_string(),
                    missing: matches!(e, serviceflow_storage::StorageError::NotFound(_)),
                },
            }
        }
        Effect::ListSets => match storage.list_sets().await {
            Ok(sets) => Command::SetsListed(sets),
            Err(e) => Command::SetsListFailed { error: e.to_string() },
        },
        Effect::LoadSet(id) => {
            let parsed = match storage.read_set(&id).await {
                Ok(stored) => serviceflow_opensong::read_set(&stored.bytes).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match parsed {
                Ok(set) => Command::SetLoaded { id, set: Arc::new(set) },
                Err(error) => Command::SetLoadFailed { id, error },
            }
        }
        Effect::CheckSetSongs { set, ids } => {
            // If the library cannot be listed, report nothing missing rather than red everywhere.
            let missing = match storage.list_songs().await {
                Ok(songs) => ids.into_iter().filter(|id| !songs.contains(id)).collect(),
                Err(_) => Vec::new(),
            };
            Command::SongsChecked { set, missing }
        }
        Effect::ReadOpenSongSettings(trigger) => {
            let mut faults = Vec::new();
            let mut output = None;
            let mut colors = None;
            match read_settings_file(storage, "PresentSettings", serviceflow_opensong::read_output_settings).await {
                Ok(settings) => output = Some(Arc::new(settings)),
                Err(fault) => faults.push(fault),
            }
            match read_settings_file(storage, "MainSettings", serviceflow_opensong::read_slide_colors).await {
                Ok(settings) => colors = Some(Arc::new(settings)),
                Err(fault) => faults.push(fault),
            }
            Command::SettingsImported { trigger, output, colors, faults }
        }
        Effect::ListBackgrounds => {
            const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "bmp"];
            let mut files: Vec<String> = match storage.list_assets().await {
                Ok(ids) => ids.into_iter().map(|id| id.0).collect(),
                Err(_) => Vec::new(),
            };
            files.retain(|f| f.rsplit_once('.').is_some_and(|(_, ext)| PICTURES.iter().any(|p| ext.eq_ignore_ascii_case(p))));
            Command::BackgroundsListed(files)
        }
        Effect::LoadImage(name) => {
            let bytes = match storage.read_asset(&serviceflow_storage::AssetId(name.clone())).await {
                Ok(stored) => Some(Arc::<[u8]>::from(stored.bytes)),
                Err(_) => None,
            };
            Command::ImageLoaded { name, bytes }
        }
        Effect::WriteSong { id, song, expect, as_copy } => save_song(storage, id, &song, expect, as_copy).await,
        Effect::LoadSong(id) => {
            let parsed = match storage.read_song(&id).await {
                Ok(stored) => serviceflow_opensong::read_song(&stored.bytes).map(|song| (song, stored.hash)).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match parsed {
                Ok((song, hash)) => Command::SongLoaded { id, song: Arc::new(song), hash },
                Err(error) => Command::SongLoadFailed { id, error },
            }
        }
    }
}
