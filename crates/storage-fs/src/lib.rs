//! Desktop filesystem storage: an OpenSong data root on disk, with crash-safe writes.
//!
//! Layout (the OpenSong data root, unchanged): `Songs/`, `Sets/`, `Settings/`, `Backgrounds/`.
//! serviceFlow-only sidecars (ADR-0003) live in `.serviceflow/Songs/<id>.sidecar` and
//! `.serviceflow/Sets/<id>.sidecar` at the data root, a dot folder OpenSong never reads.
//!
//! * Every visible file in `Songs/` is a song, with or without an extension. Names starting with
//!   `.` or `_` (files and folders) are ignored, as OpenSong does. Subfolders are categories.
//! * Writes go through [`craft_atomic`]: a staging file beside the target (dot-prefixed, so
//!   ignored) is renamed over it, so a crash leaves the old file or the new one, never half.
//! * Reads return the hash of the bytes read; a write with [`Expect::Hash`] compares it with the
//!   file as it is now and reports [`StorageError::Conflict`] if someone else changed it.
//!   (The check and the rename are not one atomic step: a change landing between them is lost.
//!   OpenSong has no locking either, so this narrows the window rather than closing it.)
//! * Renames (moves between categories too) are one `rename` of the file, so a crash leaves it
//!   under one name or the other. The target must not exist; as with hashes, that is checked
//!   just before the rename, not in the same step. A rename that changes only letter case is
//!   allowed on case-insensitive disks, where the "target" is the file itself.
//! * File IO is blocking `std::fs` inside `async fn`s: the futures complete on first poll. That
//!   suits one operator on a local disk; move it to a thread if a slow share ever matters.
//! * [`ChangeEvents::changes`] announces this storage's own writes at once, and changes made by
//!   anything else (OpenSong, another serviceFlow, a sync tool) once the folder has been quiet
//!   for a poll. Watching is a background thread that scans `Songs/ Sets/ Settings/ Backgrounds/`
//!   every [`DEFAULT_POLL`] and compares (modified time, size); it starts when the first
//!   subscriber appears and stops when the storage is dropped. Polling, not OS notifications,
//!   because it behaves the same on every platform and on the network shares and sync folders
//!   church data roots live on. A change is announced when two successive scans agree on it
//!   (the debounce), so a file being written in pieces is announced once, when finished. A
//!   rename shows as a removal plus a creation. Edits that keep both size and modified time
//!   (filesystems with coarse timestamps) are missed.

use std::any::Any;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use futures::StreamExt as _;
use futures::channel::mpsc;
use serviceflow_storage::{
    AssetId, AssetRepository, ChangeEvent, ChangeEvents, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, Key, Result, SetId,
    SetRepository, SettingsId, SettingsRepository, SidecarRepository, SongId, SongRepository, StorageError, Stored, check_path, expect_holds,
    is_ignored_path, key_path,
};

/// How often the data root is scanned for changes made by other programs.
pub const DEFAULT_POLL: Duration = Duration::from_secs(1);

/// What a scan knows about a file: enough to notice that someone changed it.
type Stat = (Option<SystemTime>, u64);

/// State shared with the watcher thread.
#[derive(Default)]
struct Shared {
    subscribers: Mutex<Vec<mpsc::UnboundedSender<ChangeEvent>>>,
    /// What has been announced, per file, once the watcher runs. Own writes are recorded here
    /// too so the watcher does not announce them a second time.
    announced: Mutex<Option<BTreeMap<Key, Stat>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Storage over an OpenSong data root on disk.
pub struct FsStorage {
    root: PathBuf,
    shared: Arc<Shared>,
    poll: Duration,
    /// Something to drop together with the storage (a temporary folder in tests).
    _guard: Option<Box<dyn Any>>,
}

impl FsStorage {
    /// Use `root` as the data root. It must be an existing folder; it need not have a `Songs/`
    /// folder yet (that is an empty library). See [`FsStorage::looks_like_data_root`].
    pub fn open(root: &Path) -> std::io::Result<Self> {
        if !root.is_dir() {
            return Err(std::io::Error::new(ErrorKind::NotFound, format!("{} is not a folder", root.display())));
        }
        Ok(Self { root: root.to_path_buf(), shared: Arc::default(), poll: DEFAULT_POLL, _guard: None })
    }

    /// Scan for changes made by other programs this often (default [`DEFAULT_POLL`]). Set it
    /// before the first call to [`changes`](ChangeEvents::changes). A change is announced after
    /// two scans, so it takes up to twice this long to arrive.
    pub fn with_poll_interval(mut self, poll: Duration) -> Self {
        self.poll = poll;
        self
    }

    /// Drop `guard` when this storage is dropped. For tests that own a temporary folder.
    pub fn keeping_alive(mut self, guard: impl Any) -> Self {
        self._guard = Some(Box::new(guard));
        self
    }

    /// `true` when `path` looks like an OpenSong data root: it holds a `Songs/` folder.
    pub fn looks_like_data_root(path: &Path) -> bool {
        path.join("Songs").is_dir()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self, key: &Key) -> Result<PathBuf> {
        let path = key_path(key);
        check_path(path)?;
        let relative = match key {
            Key::Song(_) => Path::new("Songs").join(path),
            Key::Set(_) => Path::new("Sets").join(path),
            Key::Asset(_) => Path::new("Backgrounds").join(path),
            Key::Settings(_) => Path::new("Settings").join(path),
            Key::Sidecar(DocumentId::Song(_)) => Path::new(".serviceflow/Songs").join(format!("{path}.sidecar")),
            Key::Sidecar(DocumentId::Set(_)) => Path::new(".serviceflow/Sets").join(format!("{path}.sidecar")),
        };
        Ok(self.root.join(relative))
    }

    fn read(&self, key: Key) -> Result<Stored> {
        match self.read_if_exists(&key)? {
            Some(stored) => Ok(stored),
            None => Err(StorageError::NotFound(key)),
        }
    }

    fn read_if_exists(&self, key: &Key) -> Result<Option<Stored>> {
        match std::fs::read(self.file(key)?) {
            Ok(bytes) => Ok(Some(Stored::new(bytes))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error(&e)),
        }
    }

    fn write(&self, key: Key, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        let path = self.file(&key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error(&e))?;
        }
        let hash = ContentHash::of(&bytes);
        match expect {
            Expect::Any => craft_atomic::atomic_write(&path, &bytes).map_err(|e| io_error(&e))?,
            // `write_new` never replaces an existing file, so this is race-free.
            Expect::Absent => craft_atomic::write_new(&path, &bytes).map_err(|e| match e.kind() {
                ErrorKind::AlreadyExists => StorageError::Conflict(key.clone()),
                _ => io_error(&e),
            })?,
            Expect::Hash(expected) => {
                if self.read_if_exists(&key)?.map(|s| s.hash) != Some(expected) {
                    return Err(StorageError::Conflict(key));
                }
                craft_atomic::atomic_write(&path, &bytes).map_err(|e| io_error(&e))?;
            }
        }
        self.record(&key, std::fs::metadata(&path).ok().map(|m| (m.modified().ok(), m.len())));
        self.announce(ChangeEvent { key, kind: ChangeKind::Written });
        Ok(hash)
    }

    fn delete(&self, key: Key, expect: Expect) -> Result<()> {
        let path = self.file(&key)?;
        let Some(current) = self.read_if_exists(&key)? else { return Err(StorageError::NotFound(key)) };
        if !expect_holds(expect, Some(current.hash)) {
            return Err(StorageError::Conflict(key));
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Err(StorageError::NotFound(key)),
            Err(e) => return Err(io_error(&e)),
        }
        self.record(&key, None);
        self.announce(ChangeEvent { key, kind: ChangeKind::Removed });
        Ok(())
    }

    fn rename(&self, from: Key, to: Key, expect: Expect) -> Result<ContentHash> {
        let source = self.file(&from)?;
        let target = self.file(&to)?;
        let Some(current) = self.read_if_exists(&from)? else { return Err(StorageError::NotFound(from)) };
        if !expect_holds(expect, Some(current.hash)) {
            return Err(StorageError::Conflict(from));
        }
        let same_file = key_path(&from).to_lowercase() == key_path(&to).to_lowercase();
        if !same_file && std::fs::symlink_metadata(&target).is_ok() {
            return Err(StorageError::Conflict(to));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error(&e))?;
        }
        std::fs::rename(&source, &target).map_err(|e| io_error(&e))?;
        self.record(&from, None);
        self.record(&to, std::fs::metadata(&target).ok().map(|m| (m.modified().ok(), m.len())));
        self.announce(ChangeEvent { key: from, kind: ChangeKind::Removed });
        self.announce(ChangeEvent { key: to, kind: ChangeKind::Written });
        Ok(current.hash)
    }

    /// Every visible file under `folder`, as `/`-separated paths relative to it, sorted.
    fn list(&self, folder: &str) -> Result<Vec<String>> {
        let mut found = Vec::new();
        collect(&self.root.join(folder), "", &mut |path, _| found.push(path)).map_err(|e| io_error(&e))?;
        found.sort();
        Ok(found)
    }

    fn announce(&self, event: ChangeEvent) {
        announce(&self.shared, event);
    }

    /// Tell the watcher what this storage just did to `key` (`None`: removed), so it does not
    /// announce it a second time.
    fn record(&self, key: &Key, stat: Option<Stat>) {
        if let Some(announced) = lock(&self.shared.announced).as_mut() {
            match stat {
                Some(stat) => announced.insert(key.clone(), stat),
                None => announced.remove(key),
            };
        }
    }

    /// Start the watcher thread if it is not running, with the folder as it is now as the
    /// baseline: only later changes are announced.
    fn ensure_watcher(&self) {
        let mut announced = lock(&self.shared.announced);
        if announced.is_some() {
            return;
        }
        let baseline = scan(&self.root).unwrap_or_default();
        *announced = Some(baseline.clone());
        let (root, shared, poll) = (self.root.clone(), Arc::downgrade(&self.shared), self.poll);
        // A failure to start a thread leaves the storage working without watching.
        let _ = std::thread::Builder::new().name("serviceflow-watch".into()).spawn(move || watch(root, shared, poll, baseline));
    }
}

fn announce(shared: &Shared, event: ChangeEvent) {
    lock(&shared.subscribers).retain(|s| s.unbounded_send(event.clone()).is_ok());
}

/// Every watched file under the data root with its [`Stat`]. `None` when the root or a watched
/// folder cannot be read right now (unmounted share, folder being replaced): that is not a mass
/// deletion of songs.
fn scan(root: &Path) -> Option<BTreeMap<Key, Stat>> {
    if !root.is_dir() {
        return None;
    }
    type MakeKey = fn(String) -> Key;
    let folders: [(&str, MakeKey); 4] = [
        ("Songs", |p| Key::Song(SongId(p))),
        ("Sets", |p| Key::Set(SetId(p))),
        ("Settings", |p| Key::Settings(SettingsId(p))),
        ("Backgrounds", |p| Key::Asset(AssetId(p))),
    ];
    let mut found = BTreeMap::new();
    for (folder, key) in folders {
        collect(&root.join(folder), "", &mut |path, meta| {
            found.insert(key(path), (meta.modified().ok(), meta.len()));
        })
        .ok()?;
    }
    Some(found)
}

/// The watcher thread: scan every `poll`; announce a difference from what was announced once the
/// next scan shows the same thing (the file has stopped changing). Ends when the storage is dropped.
fn watch(root: PathBuf, shared: Weak<Shared>, poll: Duration, mut previous: BTreeMap<Key, Stat>) {
    loop {
        std::thread::sleep(poll);
        let Some(shared) = shared.upgrade() else { return };
        let Some(current) = scan(&root) else { continue };
        let mut events = Vec::new();
        {
            let mut guard = lock(&shared.announced);
            let announced = guard.get_or_insert_with(BTreeMap::new);
            let keys: std::collections::BTreeSet<Key> = current.keys().chain(announced.keys()).cloned().collect();
            for key in keys {
                let (now, then, before) = (current.get(&key), previous.get(&key), announced.get(&key));
                if now == before || now != then {
                    continue;
                }
                events.push(ChangeEvent { key, kind: if now.is_some() { ChangeKind::Written } else { ChangeKind::Removed } });
            }
            for event in &events {
                match current.get(&event.key) {
                    Some(stat) => announced.insert(event.key.clone(), *stat),
                    None => announced.remove(&event.key),
                };
            }
        }
        for event in events {
            announce(&shared, event);
        }
        previous = current;
    }
}

fn io_error(e: &std::io::Error) -> StorageError {
    StorageError::Other(e.to_string())
}

/// Visible files below `dir`. A missing folder holds nothing. Names that are not UTF-8 cannot be
/// ids and are skipped, like unreadable entries are not (those fail the listing).
fn collect(dir: &Path, prefix: &str, out: &mut dyn FnMut(String, std::fs::Metadata)) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else { continue };
        if is_ignored_path(&name) {
            continue;
        }
        let path = format!("{prefix}{name}");
        // `metadata` follows symlinks, as OpenSong does.
        match std::fs::metadata(entry.path()) {
            Ok(m) if m.is_dir() => collect(&entry.path(), &format!("{path}/"), out)?,
            Ok(m) if m.is_file() => out(path, m),
            _ => {}
        }
    }
    Ok(())
}

impl SongRepository for FsStorage {
    async fn list_songs(&self) -> Result<Vec<SongId>> {
        Ok(self.list("Songs")?.into_iter().map(SongId).collect())
    }

    async fn read_song(&self, id: &SongId) -> Result<Stored> {
        self.read(Key::Song(id.clone()))
    }

    async fn write_song(&self, id: &SongId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Song(id.clone()), bytes, expect)
    }

    async fn delete_song(&self, id: &SongId, expect: Expect) -> Result<()> {
        self.delete(Key::Song(id.clone()), expect)
    }

    async fn rename_song(&self, from: &SongId, to: &SongId, expect: Expect) -> Result<ContentHash> {
        self.rename(Key::Song(from.clone()), Key::Song(to.clone()), expect)
    }
}

impl SetRepository for FsStorage {
    async fn list_sets(&self) -> Result<Vec<SetId>> {
        Ok(self.list("Sets")?.into_iter().map(SetId).collect())
    }

    async fn read_set(&self, id: &SetId) -> Result<Stored> {
        self.read(Key::Set(id.clone()))
    }

    async fn write_set(&self, id: &SetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Set(id.clone()), bytes, expect)
    }

    async fn delete_set(&self, id: &SetId, expect: Expect) -> Result<()> {
        self.delete(Key::Set(id.clone()), expect)
    }

    async fn rename_set(&self, from: &SetId, to: &SetId, expect: Expect) -> Result<ContentHash> {
        self.rename(Key::Set(from.clone()), Key::Set(to.clone()), expect)
    }
}

impl AssetRepository for FsStorage {
    async fn list_assets(&self) -> Result<Vec<AssetId>> {
        Ok(self.list("Backgrounds")?.into_iter().map(AssetId).collect())
    }

    async fn read_asset(&self, id: &AssetId) -> Result<Stored> {
        self.read(Key::Asset(id.clone()))
    }

    async fn write_asset(&self, id: &AssetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Asset(id.clone()), bytes, expect)
    }
}

impl SidecarRepository for FsStorage {
    async fn read_sidecar(&self, owner: &DocumentId) -> Result<Option<Stored>> {
        self.read_if_exists(&Key::Sidecar(owner.clone()))
    }

    async fn write_sidecar(&self, owner: &DocumentId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Sidecar(owner.clone()), bytes, expect)
    }
}

impl SettingsRepository for FsStorage {
    async fn read_settings(&self, id: &SettingsId) -> Result<Stored> {
        self.read(Key::Settings(id.clone()))
    }

    async fn write_settings(&self, id: &SettingsId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.write(Key::Settings(id.clone()), bytes, expect)
    }
}

impl ChangeEvents for FsStorage {
    fn changes(&self) -> ChangeStream {
        let (sender, receiver) = mpsc::unbounded();
        lock(&self.shared.subscribers).push(sender);
        self.ensure_watcher();
        receiver.boxed_local()
    }
}
