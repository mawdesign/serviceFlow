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
use serviceflow_storage::{ChangeEvent, ChangeKind, ContentHash, Expect, Key, SongId, Storage, StorageError};

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
