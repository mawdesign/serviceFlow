//! Seam 3: the storage contract suite. Written once, run against every storage implementation.
//!
//! Instantiate it in an implementation's integration tests with a factory expression that makes a
//! fresh, empty storage:
//!
//! ```ignore
//! serviceflow_testkit::storage_contract!(serviceflow_storage_memory::MemoryStorage::new());
//! ```
//!
//! Each check is a public `async fn` here taking the storage under test; the macro wraps every
//! one in a `#[test]`. To add a check: write the `async fn`, then add its name to the list in
//! [`storage_contract!`](crate::storage_contract).

use futures::StreamExt as _;
use serviceflow_storage::{AssetId, ChangeEvent, ChangeKind, ContentHash, DocumentId, Expect, Key, SetId, SettingsId, SongId, Storage, StorageError};

const SONG: &[u8] = b"<song><title>Contract</title></song>";

pub async fn a_written_song_reads_back_with_the_same_bytes_and_hash(storage: impl Storage) {
    let id = SongId::new("Contract Song");
    let hash = storage.write_song(&id, SONG.to_vec(), Expect::Any).await.unwrap();
    let stored = storage.read_song(&id).await.unwrap();
    assert_eq!(stored.bytes, SONG);
    assert_eq!(stored.hash, hash);
    assert_eq!(hash, ContentHash::of(SONG));
}

pub async fn written_songs_are_listed_sorted_with_their_category(storage: impl Storage) {
    for id in ["Zebra", "Hymns/Amazing Grace", "Abide With Me"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Abide With Me", "Hymns/Amazing Grace", "Zebra"].map(SongId::new));
}

pub async fn reading_a_missing_song_is_not_found(storage: impl Storage) {
    let id = SongId::new("Nowhere");
    assert_eq!(storage.read_song(&id).await, Err(StorageError::NotFound(Key::Song(id))));
}

pub async fn a_write_based_on_a_stale_hash_is_a_conflict_and_changes_nothing(storage: impl Storage) {
    let id = SongId::new("Shared");
    let first = storage.write_song(&id, b"first".to_vec(), Expect::Any).await.unwrap();
    storage.write_song(&id, b"someone else".to_vec(), Expect::Hash(first)).await.unwrap();
    let stale = storage.write_song(&id, b"mine".to_vec(), Expect::Hash(first)).await;
    assert_eq!(stale, Err(StorageError::Conflict(Key::Song(id.clone()))));
    assert_eq!(storage.read_song(&id).await.unwrap().bytes, b"someone else");
}

pub async fn creating_a_song_that_already_exists_is_a_conflict(storage: impl Storage) {
    let id = SongId::new("Taken");
    storage.write_song(&id, SONG.to_vec(), Expect::Absent).await.unwrap();
    assert_eq!(storage.write_song(&id, SONG.to_vec(), Expect::Absent).await, Err(StorageError::Conflict(Key::Song(id))));
}

pub async fn writing_a_song_emits_a_change_event(storage: impl Storage) {
    let mut changes = storage.changes();
    let id = SongId::new("Announced");
    storage.write_song(&id, SONG.to_vec(), Expect::Any).await.unwrap();
    assert_eq!(changes.next().await, Some(ChangeEvent { key: Key::Song(id), kind: ChangeKind::Written }));
}

pub async fn files_with_or_without_an_extension_are_all_songs(storage: impl Storage) {
    for id in ["No Extension", "With.txt", "Hymns/Dotted.name.xml", "Hymns/Plain"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Hymns/Dotted.name.xml", "Hymns/Plain", "No Extension", "With.txt"].map(SongId::new));
}

pub async fn names_starting_with_dot_or_underscore_are_not_songs(storage: impl Storage) {
    for id in [".hidden", "_draft", "Hymns/.DS_Store", "Hymns/_old", "_Archive/Song", ".Trash/Song", "Hymns/Kept", "Kept"] {
        storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await.unwrap();
    }
    let ids = storage.list_songs().await.unwrap();
    assert_eq!(ids, ["Hymns/Kept", "Kept"].map(SongId::new));
}

pub async fn song_ids_cannot_leave_the_songs_folder(storage: impl Storage) {
    for id in ["../escape", "a/../../escape", "/absolute", "a//b", "", "a\\b", "C:evil"] {
        let result = storage.write_song(&SongId::new(id), SONG.to_vec(), Expect::Any).await;
        assert!(matches!(result, Err(StorageError::Other(_))), "{id:?}: {result:?}");
        assert!(matches!(storage.read_song(&SongId::new(id)).await, Err(StorageError::Other(_))), "{id:?}");
    }
    assert_eq!(storage.list_songs().await.unwrap(), []);
}

pub async fn sets_round_trip_and_list_sorted(storage: impl Storage) {
    let hash = storage.write_set(&SetId::new("Sunday"), b"b".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new("Easter"), b"a".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new(".hidden"), b"x".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.list_sets().await.unwrap(), ["Easter", "Sunday"].map(SetId::new));
    let stored = storage.read_set(&SetId::new("Sunday")).await.unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"b".to_vec(), hash));
    let id = SetId::new("Nope");
    assert_eq!(storage.read_set(&id).await, Err(StorageError::NotFound(Key::Set(id))));
}

pub async fn assets_keep_their_folders_and_stale_writes_conflict(storage: impl Storage) {
    let id = AssetId("Slides/bg.png".into());
    let bytes = vec![0u8, 255, 10, 13, 0];
    let first = storage.write_asset(&id, bytes.clone(), Expect::Any).await.unwrap();
    storage.write_asset(&AssetId("a.jpg".into()), vec![1], Expect::Any).await.unwrap();
    assert_eq!(storage.list_assets().await.unwrap(), vec![id.clone(), AssetId("a.jpg".into())]);
    assert_eq!(storage.read_asset(&id).await.unwrap().bytes, bytes);
    storage.write_asset(&id, vec![2], Expect::Hash(first)).await.unwrap();
    assert_eq!(storage.write_asset(&id, vec![3], Expect::Hash(first)).await, Err(StorageError::Conflict(Key::Asset(id))));
}

pub async fn settings_round_trip_and_a_missing_file_is_not_found(storage: impl Storage) {
    let id = SettingsId("PresentSettings".into());
    assert_eq!(storage.read_settings(&id).await, Err(StorageError::NotFound(Key::Settings(id.clone()))));
    let hash = storage.write_settings(&id, b"<settings/>".to_vec(), Expect::Absent).await.unwrap();
    let stored = storage.read_settings(&id).await.unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"<settings/>".to_vec(), hash));
}

pub async fn sidecars_are_optional_and_never_appear_as_songs_or_sets(storage: impl Storage) {
    let song = DocumentId::Song(SongId::new("Hymns/Amazing Grace"));
    let set = DocumentId::Set(SetId::new("Sunday"));
    assert_eq!(storage.read_sidecar(&song).await, Ok(None));
    let hash = storage.write_sidecar(&song, b"{\"key\":2}".to_vec(), Expect::Absent).await.unwrap();
    storage.write_sidecar(&set, b"{}".to_vec(), Expect::Any).await.unwrap();
    let stored = storage.read_sidecar(&song).await.unwrap().unwrap();
    assert_eq!((stored.bytes, stored.hash), (b"{\"key\":2}".to_vec(), hash));
    assert_eq!(storage.write_sidecar(&song, b"x".to_vec(), Expect::Absent).await, Err(StorageError::Conflict(Key::Sidecar(song))));
    assert_eq!(storage.list_songs().await.unwrap(), []);
    assert_eq!(storage.list_sets().await.unwrap(), []);
}

pub async fn a_song_and_a_set_with_the_same_name_are_different_documents(storage: impl Storage) {
    storage.write_song(&SongId::new("Same"), b"song".to_vec(), Expect::Any).await.unwrap();
    storage.write_set(&SetId::new("Same"), b"set".to_vec(), Expect::Any).await.unwrap();
    assert_eq!(storage.read_song(&SongId::new("Same")).await.unwrap().bytes, b"song");
    assert_eq!(storage.read_set(&SetId::new("Same")).await.unwrap().bytes, b"set");
}

/// Generate one `#[test]` per contract check, each given a fresh storage from `$make`.
#[macro_export]
macro_rules! storage_contract {
    ($make:expr) => {
        $crate::storage_contract!(@tests $make;
            a_written_song_reads_back_with_the_same_bytes_and_hash,
            written_songs_are_listed_sorted_with_their_category,
            reading_a_missing_song_is_not_found,
            a_write_based_on_a_stale_hash_is_a_conflict_and_changes_nothing,
            creating_a_song_that_already_exists_is_a_conflict,
            writing_a_song_emits_a_change_event,
            files_with_or_without_an_extension_are_all_songs,
            names_starting_with_dot_or_underscore_are_not_songs,
            song_ids_cannot_leave_the_songs_folder,
            sets_round_trip_and_list_sorted,
            assets_keep_their_folders_and_stale_writes_conflict,
            settings_round_trip_and_a_missing_file_is_not_found,
            sidecars_are_optional_and_never_appear_as_songs_or_sets,
            a_song_and_a_set_with_the_same_name_are_different_documents,
        );
    };
    (@tests $make:expr; $($check:ident),* $(,)?) => {
        mod storage_contract {
            #[allow(unused_imports)]
            use super::*;
            $(
                #[test]
                fn $check() {
                    $crate::block_on($crate::storage_contract::$check($make));
                }
            )*
        }
    };
}
