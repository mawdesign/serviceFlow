//! Seam 1, the engine: browsing the library and searching it.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_testkit::EngineHarness;

const SONGS: &[(&str, &str)] = &[
    (
        "Hymns/Amazing Grace HOF#50",
        "<song><title>Amazing Grace</title><author>John Newton</author><hymn_number>12</hymn_number><lyrics>[V1]\n how sweet the sound</lyrics></song>",
    ),
    ("Hymns/Other HOF#500", "<song><title>Other</title><lyrics>[V1]\n nothing</lyrics></song>"),
    (
        "Worship/Oceans",
        "<song><title>Oceans (Where Feet May Fail)</title><aka>Spirit Lead Me</aka><author>Hillsong</author><ccli>6428767</ccli><theme>Worship: Intimacy</theme><hymn_number>77</hymn_number><lyrics>[V1]\n You call me out upon the waters</lyrics></song>",
    ),
];

fn harness() -> EngineHarness {
    let mut h = EngineHarness::with_songs(SONGS);
    h.send(Command::RefreshLibrary);
    h
}

fn find(h: &mut EngineHarness, query: &str) -> Vec<String> {
    h.send(Command::Search(query.to_string()));
    h.state().library.results.iter().map(|id| id.to_string()).collect()
}

#[test]
fn the_library_lists_every_song_with_titles_once_indexed() {
    let h = harness();
    let state = h.state();
    let lib = &state.library;
    assert_eq!(lib.songs.len(), 3);
    assert!(!lib.indexing);
    assert_eq!(lib.title(&SongId::new("Hymns/Amazing Grace HOF#50")), "Amazing Grace");
    assert_eq!(lib.title(&SongId::new("Worship/Oceans")), "Oceans (Where Feet May Fail)");
}

#[test]
fn the_fixture_library_lists_its_songs() {
    let mut h = EngineHarness::with_fixtures();
    h.send(Command::RefreshLibrary);
    assert_eq!(h.state().library.songs.len(), 6);
}

#[test]
fn search_covers_each_field() {
    let mut h = harness();
    let oceans = vec!["Worship/Oceans".to_string()];
    assert_eq!(find(&mut h, "oceans"), oceans, "title");
    assert_eq!(find(&mut h, "spirit lead"), oceans, "aka");
    assert_eq!(find(&mut h, "hillsong"), oceans, "author");
    assert_eq!(find(&mut h, "6428767"), oceans, "ccli");
    assert_eq!(find(&mut h, "intimacy"), oceans, "theme");
    assert_eq!(find(&mut h, "upon the WATERS"), oceans, "lyrics, any case");
    assert_eq!(find(&mut h, "sweet sound"), vec!["Hymns/Amazing Grace HOF#50"], "lyrics words");
    assert_eq!(find(&mut h, "Other HOF"), vec!["Hymns/Other HOF#500"], "filename");
    assert_eq!(find(&mut h, "nomatchatall"), Vec::<String>::new());
}

#[test]
fn a_blank_query_has_no_results() {
    let mut h = harness();
    assert!(!find(&mut h, "oceans").is_empty());
    assert!(find(&mut h, "  ").is_empty());
}

#[test]
fn hash_n_matches_the_hymn_number_exactly() {
    let mut h = harness();
    assert_eq!(find(&mut h, "#77"), vec!["Worship/Oceans"]);
    assert_eq!(find(&mut h, "#12"), vec!["Hymns/Amazing Grace HOF#50"]);
    assert_eq!(find(&mut h, "#7"), Vec::<String>::new(), "not a prefix of 77");
}

#[test]
fn hash_n_also_matches_filenames_containing_it() {
    let mut h = harness();
    assert_eq!(find(&mut h, "#50"), vec!["Hymns/Amazing Grace HOF#50"], "not HOF#500");
    assert_eq!(find(&mut h, "#500"), vec!["Hymns/Other HOF#500"]);
}

#[test]
fn hash_n_finds_the_fixture_song() {
    let mut h = EngineHarness::with_fixtures();
    h.send(Command::RefreshLibrary).send(Command::Search("#50".into()));
    assert_eq!(h.state().library.results, vec![SongId::new("Amazing Grace HOF#50")]);
}

#[test]
fn title_matches_rank_before_lyric_matches() {
    let mut h = EngineHarness::with_songs(&[
        ("A Lyric", "<song><title>Zed</title><lyrics>[V1]\n grace is here</lyrics></song>"),
        ("B Title", "<song><title>Grace</title><lyrics>[V1]\n x</lyrics></song>"),
    ]);
    h.send(Command::RefreshLibrary);
    assert_eq!(find(&mut h, "grace"), vec!["B Title", "A Lyric"]);
}

#[test]
fn songs_are_searchable_by_filename_before_indexing_finishes() {
    // Listing is done but no song has been read yet: the library is browsable and findable.
    let mut engine = serviceflow_core::engine::Engine::new();
    let effects = engine.apply(Command::RefreshLibrary);
    assert_eq!(effects.len(), 1);
    let generation = engine.snapshot().library.generation;
    let ids = vec![SongId::new("Hymns/Amazing Grace HOF#50"), SongId::new("Other")];
    engine.apply(Command::LibraryListed { generation, songs: ids });
    let state = engine.snapshot();
    assert!(state.library.indexing);
    assert_eq!(state.library.songs.len(), 2);
    engine.apply(Command::Search("#50".into()));
    assert_eq!(engine.snapshot().library.results, vec![SongId::new("Hymns/Amazing Grace HOF#50")]);
}

#[test]
fn results_of_an_older_refresh_are_dropped() {
    let mut engine = serviceflow_core::engine::Engine::new();
    engine.apply(Command::RefreshLibrary);
    engine.apply(Command::RefreshLibrary);
    engine.apply(Command::LibraryListed { generation: 1, songs: vec![SongId::new("Old")] });
    assert!(engine.snapshot().library.songs.is_empty());
}

#[test]
fn opening_a_song_from_search_results_loads_it() {
    let mut h = harness();
    let first = {
        h.send(Command::Search("oceans".into()));
        h.state().library.results[0].clone()
    };
    h.send(Command::OpenSong(first));
    let state = h.state();
    assert_eq!(state.song.as_ref().unwrap().song.title, "Oceans (Where Feet May Fail)");
}
