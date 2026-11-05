//! [`WebStorage`]: the storage traits over a browser `FileSystemDirectoryHandle`.
//!
//! - **Writes** go through `createWritable()`, which writes a swap file and replaces the file when
//!   the stream closes, so a reader never sees half a file. Chromium names the swap file
//!   `<name>.crswap` beside the target in a real folder; those are never listed or watched.
//! - **[`Expect`]** is checked by reading the file first. Between that read and the write another
//!   program could change it (the same small window as the filesystem storage). Two writes from
//!   this page can interleave too, as the browser runs them on one event loop; the engine never
//!   writes one document twice at once.
//! - **Rename** is a copy then a delete (the Folder backend has no portable move). A crash between
//!   the two leaves the document under both names, never under neither.
//! - **Changes from outside** (OpenSong on the same folder, another tab) are found by polling:
//!   every [`DEFAULT_POLL_MS`] the folders are scanned for (modified time, size). A difference is
//!   announced once two scans agree on it, as in the filesystem storage. Polling starts at the
//!   first [`ChangeEvents::changes`] call and stops when the storage is dropped. A write of ours
//!   that a scan sees before we recorded it may be announced a second time; the engine ignores a
//!   reload whose hash it already has.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use futures::StreamExt as _;
use futures::channel::mpsc;
use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use serviceflow_storage::{
    AssetId, AssetRepository, ChangeEvent, ChangeEvents, ChangeKind, ChangeStream, ContentHash, DocumentId, Expect, Key, Result, SetId,
    SetRepository, SettingsId, SettingsRepository, SidecarRepository, SongId, SongRepository, StorageError, Stored, check_path, expect_holds,
    key_path,
};
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, DomException, File, FileSystemDirectoryHandle as Dir, FileSystemFileHandle as FileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemRemoveOptions, FileSystemWritableFileStream, StorageManager,
};

use crate::archive::{self, ArchiveError};

/// How often, in milliseconds, the folders are scanned for changes made by other programs.
pub const DEFAULT_POLL_MS: u32 = 2000;

/// The top-level folders the watcher scans, and the kind of document each holds.
const WATCHED: [(&str, fn(String) -> Key); 4] = [
    ("Songs", |p| Key::Song(SongId(p))),
    ("Sets", |p| Key::Set(SetId(p))),
    ("Settings", |p| Key::Settings(SettingsId(p))),
    ("Backgrounds", |p| Key::Asset(AssetId(p))),
];

/// Where a [`WebStorage`]'s folder lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A real folder on disk, chosen with the File System Access API (Chromium).
    Folder,
    /// The browser's origin private file system: the files live in the browser.
    Browser,
}

/// Whether this browser can open a real folder (`showDirectoryPicker`, Chromium only).
pub fn folder_access_supported() -> bool {
    Reflect::has(&js_sys::global(), &JsValue::from_str("showDirectoryPicker")).unwrap_or(false)
}

/// Why a zip file could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Archive(ArchiveError),
    Storage(StorageError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Archive(e) => e.fmt(f),
            ImportError::Storage(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ImportError {}

/// Storage in a browser directory handle laid out as an OpenSong data root. Not `Send`: browser
/// objects belong to their thread.
pub struct WebStorage {
    inner: Rc<Inner>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Meta {
    modified: f64,
    size: f64,
}

type Snapshot = BTreeMap<Key, Meta>;

struct Inner {
    root: Dir,
    backend: Backend,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<ChangeEvent>>>,
    /// What the folders held when last compared; `None` until the watcher's first scan.
    baseline: RefCell<Option<Snapshot>>,
    watching: Cell<bool>,
    poll_ms: Cell<u32>,
}

fn js_error(error: JsValue) -> StorageError {
    let text = if let Some(dom) = error.dyn_ref::<DomException>() {
        format!("{}: {}", dom.name(), dom.message())
    } else if let Some(e) = error.dyn_ref::<js_sys::Error>() {
        String::from(e.message())
    } else {
        format!("{error:?}")
    };
    StorageError::Other(text)
}

fn dom_name(error: &JsValue) -> Option<String> {
    error.dyn_ref::<DomException>().map(DomException::name)
}

/// The browser's way of saying "no such file or folder" (a file where a folder was expected, or
/// the other way round, counts too).
fn is_missing(error: &JsValue) -> bool {
    matches!(dom_name(error).as_deref(), Some("NotFoundError" | "TypeMismatchError"))
}

async fn wait(promise: Promise) -> std::result::Result<JsValue, JsValue> {
    JsFuture::from(promise).await
}

/// Chromium's swap file for a `createWritable()` in progress in a real folder.
fn is_swap_file(name: &str) -> bool {
    name.ends_with(".crswap")
}

/// The file path of a document under the root.
fn key_file(key: &Key) -> Result<String> {
    check_path(key_path(key))?;
    Ok(match key {
        Key::Song(SongId(p)) => format!("Songs/{p}"),
        Key::Set(SetId(p)) => format!("Sets/{p}"),
        Key::Asset(AssetId(p)) => format!("Backgrounds/{p}"),
        Key::Settings(SettingsId(p)) => format!("Settings/{p}"),
        Key::Sidecar(DocumentId::Song(SongId(p))) => format!(".serviceflow/Songs/{p}.sidecar"),
        Key::Sidecar(DocumentId::Set(SetId(p))) => format!(".serviceflow/Sets/{p}.sidecar"),
    })
}

async fn child_dir(dir: &Dir, name: &str, create: bool) -> Result<Option<Dir>> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(create);
    match wait(dir.get_directory_handle_with_options(name, &options)).await {
        Ok(handle) => Ok(Some(handle.unchecked_into())),
        Err(e) if is_missing(&e) => Ok(None),
        Err(e) => Err(js_error(e)),
    }
}

async fn child_file(dir: &Dir, name: &str, create: bool) -> Result<Option<FileHandle>> {
    let options = FileSystemGetFileOptions::new();
    options.set_create(create);
    match wait(dir.get_file_handle_with_options(name, &options)).await {
        Ok(handle) => Ok(Some(handle.unchecked_into())),
        Err(e) if is_missing(&e) => Ok(None),
        Err(e) => Err(js_error(e)),
    }
}

async fn file_of(handle: &FileHandle) -> Result<Option<File>> {
    match wait(handle.get_file()).await {
        Ok(file) => Ok(Some(file.unchecked_into())),
        Err(e) if is_missing(&e) => Ok(None),
        Err(e) => Err(js_error(e)),
    }
}

async fn bytes_of(blob: &Blob) -> Result<Vec<u8>> {
    let buffer = wait(blob.array_buffer()).await.map_err(js_error)?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

fn meta_of(file: &File) -> Meta {
    Meta { modified: file.last_modified(), size: file.size() }
}

/// The entries of a folder: (name, handle).
async fn entries(dir: &Dir) -> Result<Vec<(String, FileSystemHandle)>> {
    let iterator = dir.values();
    let mut out = Vec::new();
    loop {
        let next = wait(iterator.next().map_err(js_error)?).await.map_err(js_error)?;
        if Reflect::get(&next, &JsValue::from_str("done")).map_err(js_error)?.as_bool().unwrap_or(true) {
            return Ok(out);
        }
        let handle: FileSystemHandle = Reflect::get(&next, &JsValue::from_str("value")).map_err(js_error)?.unchecked_into();
        out.push((handle.name(), handle));
    }
}

/// Every file under `dir`, as (`/`-separated path relative to `dir`, handle). Names starting
/// with `.` or `_` (and everything under them) are skipped unless `hidden`; swap files always are.
async fn walk(dir: Dir, hidden: bool) -> Result<Vec<(String, FileHandle)>> {
    let mut out = Vec::new();
    let mut folders = vec![(dir, String::new())];
    while let Some((folder, prefix)) = folders.pop() {
        for (name, handle) in entries(&folder).await? {
            // A name no id can hold (a colon or backslash, see `check_path`) is not listed:
            // everything listed can be read.
            if is_swap_file(&name) || (!hidden && (name.starts_with(['.', '_']) || check_path(&name).is_err())) {
                continue;
            }
            let path = format!("{prefix}{name}");
            match handle.kind() {
                FileSystemHandleKind::File => out.push((path, handle.unchecked_into())),
                FileSystemHandleKind::Directory => folders.push((handle.unchecked_into(), format!("{path}/"))),
                _ => {}
            }
        }
    }
    Ok(out)
}

async fn sleep(ms: u32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        if let Ok(set_timeout) = Reflect::get(&global, &JsValue::from_str("setTimeout")).and_then(|f| f.dyn_into::<Function>().map_err(JsValue::from))
        {
            let _ = set_timeout.call2(&global, &resolve, &JsValue::from(ms));
        }
    });
    let _ = JsFuture::from(promise).await;
}

fn storage_manager() -> Result<StorageManager> {
    let navigator = Reflect::get(&js_sys::global(), &JsValue::from_str("navigator")).map_err(js_error)?;
    let storage = Reflect::get(&navigator, &JsValue::from_str("storage")).map_err(js_error)?;
    if storage.is_undefined() {
        return Err(StorageError::Other("this browser has no file storage".into()));
    }
    Ok(storage.unchecked_into())
}

impl WebStorage {
    /// Storage on a directory handle, from either backend.
    pub fn from_directory(root: Dir, backend: Backend) -> Self {
        Self {
            inner: Rc::new(Inner {
                root,
                backend,
                subscribers: RefCell::default(),
                baseline: RefCell::default(),
                watching: Cell::new(false),
                poll_ms: Cell::new(DEFAULT_POLL_MS),
            }),
        }
    }

    /// Ask the operator for a folder (`showDirectoryPicker`, read and write). Must run soon after
    /// a click or key press. `Ok(None)` when they cancel.
    pub async fn pick_folder() -> Result<Option<Self>> {
        let global = js_sys::global();
        let picker = Reflect::get(&global, &JsValue::from_str("showDirectoryPicker"))
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or_else(|| StorageError::Other("this browser cannot open folders".into()))?;
        let options = Object::new();
        Reflect::set(&options, &JsValue::from_str("mode"), &JsValue::from_str("readwrite")).map_err(js_error)?;
        // The picker starts where it was last used for this id.
        Reflect::set(&options, &JsValue::from_str("id"), &JsValue::from_str("serviceflow-data-root")).map_err(js_error)?;
        let promise: Promise = picker.call1(&global, &options).map_err(js_error)?.unchecked_into();
        match wait(promise).await {
            Ok(handle) => Ok(Some(Self::from_directory(handle.unchecked_into(), Backend::Folder))),
            Err(e) if dom_name(&e).as_deref() == Some("AbortError") => Ok(None),
            Err(e) => Err(js_error(e)),
        }
    }

    /// The folder `name` in the browser's private file system, made if missing.
    pub async fn open_private(name: &str) -> Result<Self> {
        let root: Dir = wait(storage_manager()?.get_directory()).await.map_err(js_error)?.unchecked_into();
        let dir = child_dir(&root, name, true).await?.ok_or_else(|| StorageError::Other(format!("cannot make the folder {name:?}")))?;
        Ok(Self::from_directory(dir, Backend::Browser))
    }

    /// Ask the browser not to evict this site's files when space runs low. Returns whether it
    /// agreed (browsers decide by their own rules; Firefox may ask the operator).
    pub async fn request_persistence() -> bool {
        let Ok(manager) = storage_manager() else { return false };
        let Ok(promise) = manager.persist() else { return false };
        wait(promise).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn backend(&self) -> Backend {
        self.inner.backend
    }

    /// The folder's name (for the Folder backend, the name the operator picked).
    pub fn name(&self) -> String {
        self.inner.root.name()
    }

    /// The directory handle, for tests that change files behind the storage's back.
    pub fn directory(&self) -> &Dir {
        &self.inner.root
    }

    /// Scan for outside changes this often. Call before the first [`ChangeEvents::changes`].
    pub fn with_poll_interval(self, ms: u32) -> Self {
        self.inner.poll_ms.set(ms);
        self
    }

    /// Whether the folder has a `Songs` folder, as an OpenSong data root does.
    pub async fn looks_like_data_root(&self) -> Result<bool> {
        Ok(child_dir(&self.inner.root, "Songs", false).await?.is_some())
    }

    /// Replace everything in a Browser-backend storage with the data root packed in `zip` (see
    /// [`archive::read_archive`]). Nothing is removed if the zip cannot be read. Returns the number
    /// of files written. Refused on a real folder, which this would empty.
    ///
    /// Subscribers are not told about each file: reopen the library afterwards.
    pub async fn import_archive(&self, zip: &[u8]) -> std::result::Result<usize, ImportError> {
        if self.inner.backend != Backend::Browser {
            return Err(ImportError::Storage(StorageError::Other("a zip file can only be imported into browser storage".into())));
        }
        let files = archive::read_archive(zip).map_err(ImportError::Archive)?;
        self.clear().await.map_err(ImportError::Storage)?;
        for (path, bytes) in &files {
            self.inner.write_path(path, bytes).await.map_err(ImportError::Storage)?;
        }
        *self.inner.baseline.borrow_mut() = None;
        Ok(files.len())
    }

    /// Every file under the root (sidecars included) packed as a zip ([`archive::write_archive`]).
    pub async fn export_archive(&self) -> Result<Vec<u8>> {
        let mut files = Vec::new();
        for (path, handle) in walk(self.inner.root.clone(), true).await? {
            if let Some(file) = file_of(&handle).await? {
                files.push((path, bytes_of(&file).await?));
            }
        }
        Ok(archive::write_archive(&files))
    }

    /// Remove everything under the root.
    async fn clear(&self) -> Result<()> {
        let options = FileSystemRemoveOptions::new();
        options.set_recursive(true);
        for (name, _) in entries(&self.inner.root).await? {
            match wait(self.inner.root.remove_entry_with_options(&name, &options)).await {
                Ok(_) => {}
                Err(e) if is_missing(&e) => {}
                Err(e) => return Err(js_error(e)),
            }
        }
        Ok(())
    }
}

impl Inner {
    /// The folder holding `path` and the file's name, making folders when `create`.
    async fn parent<'p>(&self, path: &'p str, create: bool) -> Result<Option<(Dir, &'p str)>> {
        let (folders, name) = path.rsplit_once('/').map_or(("", path), |(f, n)| (f, n));
        let mut dir = self.root.clone();
        for part in folders.split('/').filter(|p| !p.is_empty()) {
            match child_dir(&dir, part, create).await? {
                Some(next) => dir = next,
                None => return Ok(None),
            }
        }
        Ok(Some((dir, name)))
    }

    async fn read_path(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some((dir, name)) = self.parent(path, false).await? else { return Ok(None) };
        let Some(handle) = child_file(&dir, name, false).await? else { return Ok(None) };
        match file_of(&handle).await? {
            Some(file) => Ok(Some(bytes_of(&file).await?)),
            None => Ok(None),
        }
    }

    async fn write_path(&self, path: &str, bytes: &[u8]) -> Result<FileHandle> {
        let (dir, name) = self.parent(path, true).await?.ok_or_else(|| StorageError::Other(format!("cannot make the folder for {path:?}")))?;
        let handle = child_file(&dir, name, true).await?.ok_or_else(|| StorageError::Other(format!("cannot make {path:?}")))?;
        let stream: FileSystemWritableFileStream = wait(handle.create_writable()).await.map_err(js_error)?.unchecked_into();
        // A copy in JS memory: a view of wasm memory could move while the write is pending.
        let data = Uint8Array::from(bytes);
        let written = match stream.write_with_js_u8_array(&data) {
            Ok(promise) => wait(promise).await,
            Err(e) => Err(e),
        };
        match written {
            Ok(_) => {
                wait(stream.close()).await.map_err(js_error)?;
                Ok(handle)
            }
            Err(e) => {
                let _ = wait(stream.abort()).await;
                Err(js_error(e))
            }
        }
    }

    /// Remove a file; false if it was not there.
    async fn remove_path(&self, path: &str) -> Result<bool> {
        let Some((dir, name)) = self.parent(path, false).await? else { return Ok(false) };
        match wait(dir.remove_entry(name)).await {
            Ok(_) => Ok(true),
            Err(e) if is_missing(&e) => Ok(false),
            Err(e) => Err(js_error(e)),
        }
    }

    async fn read(&self, key: Key) -> Result<Stored> {
        let path = key_file(&key)?;
        self.read_path(&path).await?.map(Stored::new).ok_or(StorageError::NotFound(key))
    }

    async fn write(&self, key: Key, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        let path = key_file(&key)?;
        if expect != Expect::Any {
            let current = self.read_path(&path).await?.map(|b| ContentHash::of(&b));
            if !expect_holds(expect, current) {
                return Err(StorageError::Conflict(key));
            }
        }
        let handle = self.write_path(&path, &bytes).await?;
        self.record_written(&key, &handle).await;
        self.announce(ChangeEvent { key, kind: ChangeKind::Written });
        Ok(ContentHash::of(&bytes))
    }

    async fn delete(&self, key: Key, expect: Expect) -> Result<()> {
        let path = key_file(&key)?;
        let Some(bytes) = self.read_path(&path).await? else { return Err(StorageError::NotFound(key)) };
        if !expect_holds(expect, Some(ContentHash::of(&bytes))) {
            return Err(StorageError::Conflict(key));
        }
        if !self.remove_path(&path).await? {
            return Err(StorageError::NotFound(key));
        }
        self.record_removed(&key);
        self.announce(ChangeEvent { key, kind: ChangeKind::Removed });
        Ok(())
    }

    async fn rename(&self, from: Key, to: Key, expect: Expect) -> Result<ContentHash> {
        let from_path = key_file(&from)?;
        let to_path = key_file(&to)?;
        let Some(bytes) = self.read_path(&from_path).await? else { return Err(StorageError::NotFound(from)) };
        let hash = ContentHash::of(&bytes);
        if !expect_holds(expect, Some(hash)) {
            return Err(StorageError::Conflict(from));
        }
        if from_path == to_path {
            return Ok(hash);
        }
        // A real folder may ignore case, so "song" would find "Song": a case-only rename is not
        // a clash, and must not write the new name before removing the old one.
        let case_only = from_path.eq_ignore_ascii_case(&to_path);
        if !case_only && self.read_path(&to_path).await?.is_some() {
            return Err(StorageError::Conflict(to));
        }
        if case_only {
            self.remove_path(&from_path).await?;
            self.write_path(&to_path, &bytes).await?;
        } else {
            self.write_path(&to_path, &bytes).await?;
            self.remove_path(&from_path).await?;
        }
        self.record_removed(&from);
        if let Some((dir, name)) = self.parent(&to_path, false).await?
            && let Some(handle) = child_file(&dir, name, false).await?
        {
            self.record_written(&to, &handle).await;
        }
        self.announce(ChangeEvent { key: from, kind: ChangeKind::Removed });
        self.announce(ChangeEvent { key: to, kind: ChangeKind::Written });
        Ok(hash)
    }

    async fn list<T>(&self, folder: &str, make: fn(String) -> T) -> Result<Vec<T>> {
        let Some(dir) = child_dir(&self.root, folder, false).await? else { return Ok(Vec::new()) };
        let mut paths: Vec<String> = walk(dir, false).await?.into_iter().map(|(path, _)| path).collect();
        paths.sort();
        Ok(paths.into_iter().map(make).collect())
    }

    fn announce(&self, event: ChangeEvent) {
        self.subscribers.borrow_mut().retain(|s| s.unbounded_send(event.clone()).is_ok());
    }

    /// Our own write is not news for the watcher.
    async fn record_written(&self, key: &Key, handle: &FileHandle) {
        if self.baseline.borrow().is_none() || matches!(key, Key::Sidecar(_)) {
            return;
        }
        if let Ok(Some(file)) = file_of(handle).await
            && let Some(baseline) = self.baseline.borrow_mut().as_mut()
        {
            baseline.insert(key.clone(), meta_of(&file));
        }
    }

    fn record_removed(&self, key: &Key) {
        if let Some(baseline) = self.baseline.borrow_mut().as_mut() {
            baseline.remove(key);
        }
    }

    /// (modified time, size) of every watched document.
    async fn scan(&self) -> Result<Snapshot> {
        let mut snapshot = Snapshot::new();
        for (folder, make) in WATCHED {
            let Some(dir) = child_dir(&self.root, folder, false).await? else { continue };
            for (path, handle) in walk(dir, false).await? {
                if let Some(file) = file_of(&handle).await? {
                    snapshot.insert(make(path), meta_of(&file));
                }
            }
        }
        Ok(snapshot)
    }

    /// Announce what differs from the baseline and looked the same in the previous scan too.
    fn compare(&self, scan: &Snapshot, previous: Option<&Snapshot>) {
        let mut events = Vec::new();
        {
            let mut baseline = self.baseline.borrow_mut();
            let Some(base) = baseline.as_mut() else {
                *baseline = Some(scan.clone());
                return;
            };
            let keys: BTreeSet<Key> = base.keys().chain(scan.keys()).cloned().collect();
            for key in keys {
                let now = scan.get(&key);
                let settled = previous.map(|p| p.get(&key)) == Some(now);
                if base.get(&key) == now || !settled {
                    continue;
                }
                match now {
                    Some(meta) => {
                        base.insert(key.clone(), *meta);
                        events.push(ChangeEvent { key, kind: ChangeKind::Written });
                    }
                    None => {
                        base.remove(&key);
                        events.push(ChangeEvent { key, kind: ChangeKind::Removed });
                    }
                }
            }
        }
        for event in events {
            self.announce(event);
        }
    }
}

/// The polling loop; ends once the storage is dropped.
async fn watch(weak: Weak<Inner>) {
    let mut previous: Option<Snapshot> = None;
    loop {
        if let Some(inner) = weak.upgrade() {
            match inner.scan().await {
                Ok(scan) => {
                    inner.compare(&scan, previous.as_ref());
                    previous = Some(scan);
                }
                // A folder that went away or lost permission: try again later, announce nothing.
                Err(_) => previous = None,
            }
        }
        let Some(ms) = weak.upgrade().map(|inner| inner.poll_ms.get()) else { return };
        sleep(ms).await;
    }
}

impl SongRepository for WebStorage {
    async fn list_songs(&self) -> Result<Vec<SongId>> {
        self.inner.list("Songs", SongId).await
    }

    async fn read_song(&self, id: &SongId) -> Result<Stored> {
        self.inner.read(Key::Song(id.clone())).await
    }

    async fn write_song(&self, id: &SongId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.inner.write(Key::Song(id.clone()), bytes, expect).await
    }

    async fn delete_song(&self, id: &SongId, expect: Expect) -> Result<()> {
        self.inner.delete(Key::Song(id.clone()), expect).await
    }

    async fn rename_song(&self, from: &SongId, to: &SongId, expect: Expect) -> Result<ContentHash> {
        self.inner.rename(Key::Song(from.clone()), Key::Song(to.clone()), expect).await
    }
}

impl SetRepository for WebStorage {
    async fn list_sets(&self) -> Result<Vec<SetId>> {
        self.inner.list("Sets", SetId).await
    }

    async fn read_set(&self, id: &SetId) -> Result<Stored> {
        self.inner.read(Key::Set(id.clone())).await
    }

    async fn write_set(&self, id: &SetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.inner.write(Key::Set(id.clone()), bytes, expect).await
    }

    async fn delete_set(&self, id: &SetId, expect: Expect) -> Result<()> {
        self.inner.delete(Key::Set(id.clone()), expect).await
    }

    async fn rename_set(&self, from: &SetId, to: &SetId, expect: Expect) -> Result<ContentHash> {
        self.inner.rename(Key::Set(from.clone()), Key::Set(to.clone()), expect).await
    }
}

impl AssetRepository for WebStorage {
    async fn list_assets(&self) -> Result<Vec<AssetId>> {
        self.inner.list("Backgrounds", AssetId).await
    }

    async fn read_asset(&self, id: &AssetId) -> Result<Stored> {
        self.inner.read(Key::Asset(id.clone())).await
    }

    async fn write_asset(&self, id: &AssetId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.inner.write(Key::Asset(id.clone()), bytes, expect).await
    }
}

impl SidecarRepository for WebStorage {
    async fn read_sidecar(&self, owner: &DocumentId) -> Result<Option<Stored>> {
        let path = key_file(&Key::Sidecar(owner.clone()))?;
        Ok(self.inner.read_path(&path).await?.map(Stored::new))
    }

    async fn write_sidecar(&self, owner: &DocumentId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.inner.write(Key::Sidecar(owner.clone()), bytes, expect).await
    }
}

impl SettingsRepository for WebStorage {
    async fn read_settings(&self, id: &SettingsId) -> Result<Stored> {
        self.inner.read(Key::Settings(id.clone())).await
    }

    async fn write_settings(&self, id: &SettingsId, bytes: Vec<u8>, expect: Expect) -> Result<ContentHash> {
        self.inner.write(Key::Settings(id.clone()), bytes, expect).await
    }
}

impl serviceflow_storage::ArchiveImport for WebStorage {
    async fn import_archive(&self, zip: &[u8]) -> Result<usize> {
        WebStorage::import_archive(self, zip).await.map_err(|e| match e {
            ImportError::Storage(e) => e,
            ImportError::Archive(e) => StorageError::Other(e.to_string()),
        })
    }
}

impl ChangeEvents for WebStorage {
    fn changes(&self) -> ChangeStream {
        let (sender, receiver) = mpsc::unbounded();
        self.inner.subscribers.borrow_mut().push(sender);
        if !self.inner.watching.replace(true) {
            wasm_bindgen_futures::spawn_local(watch(Rc::downgrade(&self.inner)));
        }
        receiver.boxed_local()
    }
}
