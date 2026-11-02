//! Title, subtitle, auto-scale and text effects, through the engine seam where a song is needed.

use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::ids::SongId;
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Captions, Rgba, Style, VAlign};
use serviceflow_render::{BoxRole, Renderer, Size};
use serviceflow_testkit::EngineHarness;

const HD: Size = Size { width: 1920, height: 1080 };

fn slide(lines: &[&str]) -> Slide {
    Slide { section: "V1".into(), lines: lines.iter().map(|l| l.to_string()).collect(), ..Slide::default() }
}

fn captions() -> Captions {
    Captions { title: "Amazing Grace".into(), subtitle: "John Newton".into() }
}

#[test]
fn title_sits_at_the_top_and_subtitle_at_the_bottom_and_the_body_gets_the_rest() {
    let mut style = Style::default();
    style.title.text.valign = VAlign::Top;
    style.subtitle.text.valign = VAlign::Bottom;
    let frame = Renderer::new().layout_captioned(&slide(&["Hello"]), &captions(), &style, HD);

    let roles: Vec<_> = frame.boxes.iter().map(|b| b.role).collect();
    assert_eq!(roles, [BoxRole::Title, BoxRole::Subtitle, BoxRole::Body]);
    let [title, subtitle, body] = [&frame.boxes[0], &frame.boxes[1], &frame.boxes[2]];
    assert!(title.rect.y < 100.0);
    assert!(subtitle.rect.y + subtitle.rect.height > 1000.0);
    assert!(body.rect.y >= title.rect.y + title.rect.height - 0.01);
    assert!(body.rect.y + body.rect.height <= subtitle.rect.y + 0.01);
    assert_eq!(title.lines[0].text, "Amazing Grace");
}

#[test]
fn a_bottom_title_stacks_above_the_bottom_subtitle() {
    let mut style = Style::default();
    style.title.text.valign = VAlign::Bottom;
    style.subtitle.text.valign = VAlign::Bottom;
    let frame = Renderer::new().layout_captioned(&slide(&["Hello"]), &captions(), &style, HD);
    let (title, subtitle) = (&frame.boxes[0], &frame.boxes[1]);
    assert!(title.rect.y + title.rect.height <= subtitle.rect.y + 0.01);
}

#[test]
fn disabled_or_empty_captions_draw_no_box() {
    let mut style = Style::default();
    style.title.text.enabled = false;
    let frame = Renderer::new().layout_captioned(&slide(&["Hello"]), &captions(), &style, HD);
    assert_eq!(frame.boxes.iter().map(|b| b.role).collect::<Vec<_>>(), [BoxRole::Subtitle, BoxRole::Body]);
    let frame = Renderer::new().layout_captioned(&slide(&["Hello"]), &Captions::default(), &Style::default(), HD);
    assert_eq!(frame.boxes.len(), 1);
}

#[test]
fn auto_scale_shrinks_text_until_it_fits_and_never_grows_it() {
    let lines: Vec<String> = (0..14).map(|i| format!("Line number {i} of a very long verse")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let renderer = Renderer::new();

    let mut style = Style::default();
    style.body.auto_scale = false;
    let unscaled = renderer.layout(&slide(&refs), &style, HD);
    style.body.auto_scale = true;
    let scaled = renderer.layout(&slide(&refs), &style, HD);

    let (u, s) = (&unscaled.boxes[0], &scaled.boxes[0]);
    assert!(u.lines.len() as f32 * u.lines[0].font_size * 1.2 > u.rect.height, "the test text must overflow");
    assert!(s.lines[0].font_size < u.lines[0].font_size);
    assert!(s.lines.len() as f32 * s.lines[0].font_size * 1.2 <= s.rect.height + 0.5);
    assert!(s.lines.iter().all(|l| l.width <= s.rect.width + 0.5));

    // Text that already fits is left alone.
    let short = renderer.layout(&slide(&["Hello"]), &style, HD);
    assert_eq!(short.boxes[0].lines[0].font_size, 72.0);
}

#[test]
fn auto_scale_shrinks_a_word_wider_than_the_screen() {
    let mut style = Style::default();
    style.body.auto_scale = true;
    let frame = Renderer::new().layout(&slide(&["Supercalifragilisticexpialidocious Supercalifragilisticexpialidocious"]), &style, HD);
    assert!(frame.boxes[0].lines.iter().all(|l| l.width <= frame.boxes[0].rect.width + 0.5));
}

#[test]
fn outline_shadow_and_fill_are_carried_by_every_run_and_drawn() {
    let renderer = Renderer::new();
    let mut style = Style::default();
    style.body.text.color = Rgba::rgb(255, 255, 255);
    style.body.text.outline = true;
    style.body.text.outline_color = Rgba::rgb(255, 0, 0);
    style.body.text.fill = true;
    style.body.text.fill_color = Rgba::rgb(0, 0, 255);
    let size = Size { width: 1280, height: 720 };
    let frame = renderer.layout(&slide(&["IIIIIIII"]), &style, size);
    let fx = frame.boxes[0].lines[0].runs[0].effects;
    assert_eq!(fx.outline.map(|o| o.0), Some(Rgba::rgb(255, 0, 0)));
    // Thickness 2 at the 1080 reference, scaled to 720 high.
    assert!((fx.outline.unwrap().1 - 2.0 * 720.0 / 1080.0).abs() < 0.01);

    let image = renderer.draw(&frame);
    let line = &frame.boxes[0].lines[0];
    // A pixel inside the line box but away from glyphs is the fill colour.
    let y = (line.baseline - line.font_size * 0.3) as u32;
    let blue = (line.x as u32..(line.x + line.width) as u32).any(|x| image.pixel(x, y) == [0, 0, 255, 255]);
    let red = (line.x as u32 - 4..(line.x + line.width) as u32 + 4).any(|x| image.pixel(x, y) == [255, 0, 0, 255]);
    let white = (line.x as u32..(line.x + line.width) as u32).any(|x| image.pixel(x, y) == [255, 255, 255, 255]);
    assert!(blue && red && white, "fill {blue} outline {red} text {white}");
}

#[test]
fn a_shadow_adds_ink_below_and_right_of_the_glyphs() {
    let renderer = Renderer::new();
    let mut style = Style::default();
    style.body.text.shadow = true;
    style.body.text.shadow_color = Rgba::rgb(0, 255, 0);
    let size = Size { width: 1280, height: 720 };
    let frame = renderer.layout(&slide(&["IIIIIIII"]), &style, size);
    let image = renderer.draw(&frame);
    let line = &frame.boxes[0].lines[0];
    let y = (line.baseline - line.font_size * 0.3) as u32;
    assert!((0..1280).any(|x| image.pixel(x, y) == [0, 255, 0, 255]));
}

#[test]
fn the_imported_fixture_style_lays_out_a_song_with_title_and_subtitle() {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
    harness.send(Command::OpenSong(SongId::new("It Is Well With My Soul")));
    let state = harness.state();
    let open = state.song.as_ref().expect("song open");
    let style = &state.styles.default;
    let captions = Captions::for_song(&open.song, style);
    assert_eq!(captions.title, "It Is Well With My Soul");
    assert_eq!(captions.subtitle, "Horatio Spafford and Philip Bliss");

    let frame = Renderer::new().layout_captioned(&open.slides[0], &captions, style, HD);
    let roles: Vec<_> = frame.boxes.iter().map(|b| b.role).collect();
    assert_eq!(roles, [BoxRole::Title, BoxRole::Subtitle, BoxRole::Body]);
    let body = &frame.boxes[2];
    assert!(body.lines.iter().all(|l| l.width <= body.rect.width + 0.5));
    insta::assert_snapshot!(frame.snapshot());
}
