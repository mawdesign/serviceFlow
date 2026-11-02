//! Layout snapshots through the engine seam: open a fixture song with the engine, then lay out
//! the slides it put into the state. Review changes with `cargo insta review` (or accept with
//! `INSTA_UPDATE=always`).

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_render::{Renderer, Size, Style};
use serviceflow_testkit::EngineHarness;

fn song_layout(song: &str, size: Size) -> String {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new(song)));
    let state = harness.state();
    let open = state.song.as_ref().expect("the song opened");
    let renderer = Renderer::new();
    let style = Style::default();
    open.slides
        .iter()
        .enumerate()
        .map(|(i, slide)| format!("# slide {} ({})\n{}", i + 1, slide.section, renderer.layout(slide, &style, size).snapshot()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn it_is_well_with_my_soul_at_1080p() {
    insta::assert_snapshot!(song_layout("It Is Well With My Soul", Size { width: 1920, height: 1080 }));
}

#[test]
fn amazing_grace_as_thumbnails() {
    insta::assert_snapshot!(song_layout("Amazing Grace HOF#50", Size { width: 320, height: 180 }));
}
