//! Custom and image items through the engine seam: open the fixture set, select the item, then
//! lay out / draw what the engine put into the state.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SetId;
use serviceflow_core::state::State;
use serviceflow_core::style::Captions;
use serviceflow_render::{Renderer, Size};
use serviceflow_testkit::EngineHarness;
use std::sync::Arc;

fn item(set: &str, index: usize) -> Arc<State> {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSet(SetId::new(set)));
    harness.send(Command::SelectItem(index));
    harness.state()
}

fn layout(state: &State, size: Size) -> String {
    let open = state.song.as_ref().expect("the item opened");
    let renderer = Renderer::new();
    open.slides
        .iter()
        .enumerate()
        .map(|(i, slide)| {
            let resolved = state.open_resolved(i).unwrap();
            let style = resolved.style(&state.styles);
            let captions = Captions::for_song(&open.song, &style);
            let frame = renderer.layout_with_images(slide, &captions, &style, &state.images, size);
            format!("# slide {} ({})\n{}", i + 1, slide.section, frame.snapshot())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn custom_slides_render_with_title_subtitle_and_body_at_1080p() {
    insta::assert_snapshot!(layout(&item("Notices and Pictures", 0), Size { width: 1920, height: 1080 }));
}

#[test]
fn image_slides_lay_out_their_picture_and_no_text() {
    insta::assert_snapshot!(layout(&item("Notices and Pictures", 1), Size { width: 1920, height: 1080 }));
}

fn pixel(state: &State, index: usize, x: u32, y: u32) -> [u8; 4] {
    let open = state.song.as_ref().unwrap();
    let resolved = state.open_resolved(index).unwrap();
    let style = resolved.style(&state.styles);
    let captions = Captions::for_song(&open.song, &style);
    let image = Renderer::new().render_with_images(&open.slides[index], &captions, &style, &state.images, Size { width: 160, height: 90 });
    image.pixel(x, y)
}

#[test]
fn an_image_is_drawn_and_a_missing_one_leaves_the_plain_background() {
    let state = item("Notices and Pictures", 1);
    let drawn = pixel(&state, 0, 80, 45);
    assert!(drawn[0] > 150 && drawn[1] < 80 && drawn[2] < 80, "the red picture fills the screen: {drawn:?}");
    let missing = pixel(&state, 1, 80, 45);
    assert_eq!(missing, [0, 0, 0, 255], "the default style's black background");
}
