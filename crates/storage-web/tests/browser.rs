//! Browser storage in a real (headless) browser: the shared storage contract against both
//! backends, plus zip import/export and outside changes. Run with `cargo xtask browser-test`.
//!
//! Both backends run over a fresh folder in the origin private file system: a test cannot click
//! a folder picker, and a picked folder is the same `FileSystemDirectoryHandle` type.
#![cfg(target_arch = "wasm32")]

use std::cell::Cell;

use futures::StreamExt as _;
use serviceflow_storage::{
    ChangeEvent, ChangeEvents as _, ChangeKind, ChangeStream, Expect, Key, SetId, SetRepository as _, SongId, SongRepository as _,
};
use serviceflow_storage_web::archive::{read_archive, write_archive};
use serviceflow_storage_web::{Backend, ImportError, WebStorage};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static COUNTER: Cell<u32> = const { Cell::new(0) };
}

/// A folder name no other test uses.
fn unique(prefix: &str) -> String {
    let n = COUNTER.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    format!("{prefix}-{}-{n}", js_sys::Date::now() as u64)
}

async fn private() -> WebStorage {
    WebStorage::open_private(&unique("contract")).await.expect("opening a private folder")
}

async fn folder() -> WebStorage {
    WebStorage::from_directory(private().await.directory().clone(), Backend::Folder)
}

serviceflow_testkit::storage_contract!(browser: private());

mod folder_backend {
    use super::*;
    serviceflow_testkit::storage_contract!(browser: folder());
}

const SONG: &[u8] = include_bytes!("../../../tests/fixtures/opensong/Songs/It Is Well With My Soul");

fn data_root() -> Vec<(String, Vec<u8>)> {
    vec![
        (".serviceflow/Sets/Sunday.sidecar".into(), b"version = 1\n".to_vec()),
        ("Sets/Sunday".into(), b"<set name=\"Sunday\"/>".to_vec()),
        ("Settings/PresentSettings".into(), b"<settings/>".to_vec()),
        ("Songs/Hymns/It Is Well With My Soul".into(), SONG.to_vec()),
        ("Songs/Plain".into(), b"<song/>".to_vec()),
    ]
}

async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window().unwrap().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms).unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

/// The next change event, or `None` after `ms`.
async fn next_within(changes: &mut ChangeStream, ms: i32) -> Option<ChangeEvent> {
    let timeout = std::pin::pin!(sleep(ms));
    match futures::future::select(changes.next(), timeout).await {
        futures::future::Either::Left((event, _)) => event,
        futures::future::Either::Right(_) => None,
    }
}

#[wasm_bindgen_test]
async fn an_imported_zip_replaces_browser_storage_and_exports_back_unchanged() {
    let storage = private().await;
    assert!(!storage.looks_like_data_root().await.unwrap());
    storage.write_song(&SongId::new("Left Over"), b"old".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.import_archive(&write_archive(&data_root())).await, Ok(data_root().len()));
    assert!(storage.looks_like_data_root().await.unwrap());
    assert_eq!(storage.list_songs().await.unwrap(), [SongId::new("Hymns/It Is Well With My Soul"), SongId::new("Plain")]);
    assert_eq!(storage.read_song(&SongId::new("Hymns/It Is Well With My Soul")).await.unwrap().bytes, SONG);
    assert_eq!(storage.list_sets().await.unwrap(), [SetId::new("Sunday")]);
    assert_eq!(read_archive(&storage.export_archive().await.unwrap()).unwrap(), data_root());
}

#[wasm_bindgen_test]
async fn a_bad_zip_changes_nothing() {
    let storage = private().await;
    storage.write_song(&SongId::new("Kept"), b"<song/>".to_vec(), Expect::Any).await.unwrap();
    let result = storage.import_archive(b"not a zip").await;
    assert!(matches!(result, Err(ImportError::Archive(_))), "{result:?}");
    assert_eq!(storage.list_songs().await.unwrap(), [SongId::new("Kept")]);
}

#[wasm_bindgen_test]
async fn a_real_folder_is_never_replaced_by_a_zip() {
    let storage = folder().await;
    storage.write_song(&SongId::new("Kept"), b"<song/>".to_vec(), Expect::Any).await.unwrap();
    let result = storage.import_archive(&write_archive(&data_root())).await;
    assert!(matches!(result, Err(ImportError::Storage(_))), "{result:?}");
    assert_eq!(storage.list_songs().await.unwrap(), [SongId::new("Kept")]);
}

#[wasm_bindgen_test]
async fn chromium_swap_files_are_never_songs() {
    let storage = private().await;
    storage.write_song(&SongId::new("Hymns/Song.crswap"), b"partial".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&SongId::new("Hymns/Song"), b"<song/>".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.list_songs().await.unwrap(), [SongId::new("Hymns/Song")]);
}

#[wasm_bindgen_test]
async fn changes_made_by_another_program_are_announced_once() {
    let storage = private().await.with_poll_interval(30);
    // "Another program": a second storage on the same folder, whose writes this one never hears of.
    let other = WebStorage::from_directory(storage.directory().clone(), Backend::Folder);
    let mut changes = storage.changes();
    sleep(100).await;
    let id = SongId::new("Hymns/From Outside");
    other.write_song(&id, b"<song>one</song>".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(next_within(&mut changes, 3000).await, Some(ChangeEvent { key: Key::Song(id.clone()), kind: ChangeKind::Written }));
    assert_eq!(next_within(&mut changes, 300).await, None, "announced once");
    // Our own write is announced at once, and the watcher does not repeat it.
    storage.write_song(&SongId::new("Mine"), b"<song/>".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(next_within(&mut changes, 10).await.map(|e| e.key), Some(Key::Song(SongId::new("Mine"))));
    other.delete_song(&id, Expect::Any).await.unwrap();
    assert_eq!(next_within(&mut changes, 3000).await, Some(ChangeEvent { key: Key::Song(id), kind: ChangeKind::Removed }));
    assert_eq!(next_within(&mut changes, 300).await, None, "nothing else");
}
