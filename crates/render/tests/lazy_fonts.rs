//! Lazy CJK faces (#26): the browser build fetches the large CJK faces only when displayed text
//! needs them, and draws blank placeholders until they arrive. `Fonts::builtin_lazy` behaves like
//! the browser build natively.

use std::sync::Arc;

use serviceflow_core::song::Slide;
use serviceflow_render::{Fonts, Frame, Renderer, Size, Style};

const HD: Size = Size { width: 1920, height: 1080 };

fn slide(lines: &[&str]) -> Slide {
    Slide { section: "V1".into(), lines: lines.iter().map(|l| l.to_string()).collect(), ..Slide::default() }
}

fn families(fonts: &Fonts, frame: &Frame) -> Vec<String> {
    let mut out: Vec<String> = frame.boxes.iter().flat_map(|b| &b.lines).flat_map(|l| &l.runs).map(|r| fonts.family(r.font)).collect();
    out.dedup();
    out
}

fn font_file(name: &str) -> Arc<[u8]> {
    Arc::from(std::fs::read(format!("{}/fonts/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap())
}

fn ink(renderer: &Renderer, slide: &Slide) -> usize {
    let image = renderer.render(slide, &Style::default(), Size { width: 480, height: 270 });
    // White text on black: count lit pixels.
    image.pixels.chunks(4).filter(|p| p[0] > 128).count()
}

#[test]
fn the_cjk_faces_are_the_lazy_ones() {
    let lazy: Vec<&str> = Fonts::bundled().iter().filter(|b| b.lazy).map(|b| b.file).collect();
    assert_eq!(lazy, ["NotoSansSC-Regular.otf", "NotoSansKR-Hangul-Subset.otf"]);
}

#[test]
fn latin_text_never_asks_for_a_lazy_face() {
    let fonts = Fonts::builtin_lazy();
    let renderer = Renderer::with_fonts(fonts.clone());
    renderer.layout(&slide(&["Amazing grace, how sweet the sound", "Ἀμήν, שלום, سلام"]), &Style::default(), HD);
    assert!(fonts.take_wanted().is_empty());
    assert!(!fonts.waiting());
}

#[test]
fn chinese_text_asks_once_for_the_cjk_face_and_shows_blank_placeholders_meanwhile() {
    let fonts = Fonts::builtin_lazy();
    let renderer = Renderer::with_fonts(fonts.clone());
    let chinese = slide(&["一件礼物 A Gift"]);
    let frame = renderer.layout(&chinese, &Style::default(), HD);
    assert!(fonts.waiting());
    assert_eq!(fonts.take_wanted().iter().map(|b| b.family).collect::<Vec<_>>(), ["Noto Sans SC"]);
    assert!(fonts.take_wanted().is_empty(), "asked for once");
    // The placeholders are drawn with the style's face, as blank one-em glyphs: no missing-glyph boxes.
    assert_eq!(families(&fonts, &frame), ["Liberation Sans"]);
    assert!(frame.boxes[0].lines[0].runs.iter().flat_map(|r| &r.glyphs).all(|g| g.id != 0));
    let with_placeholders = ink(&renderer, &chinese);
    assert_eq!(with_placeholders, ink(&renderer, &slide(&["     A Gift"])), "the four hanzi draw nothing");

    let before = fonts.generation();
    assert!(fonts.provide("NotoSansSC-Regular.otf", font_file("NotoSansSC-Regular.otf")));
    assert!(fonts.generation() > before, "pictures drawn before can be told apart");
    assert!(!fonts.waiting());
    let frame = renderer.layout(&chinese, &Style::default(), HD);
    assert_eq!(families(&fonts, &frame), ["Noto Sans SC", "Liberation Sans"]);
    assert!(ink(&renderer, &chinese) > with_placeholders);
    // Laid out as the full native build does.
    assert_eq!(frame.snapshot(), Renderer::new().layout(&chinese, &Style::default(), HD).snapshot());
}

#[test]
fn hangul_asks_for_the_korean_face() {
    let fonts = Fonts::builtin_lazy();
    let renderer = Renderer::with_fonts(fonts.clone());
    renderer.layout(&slide(&["주 하나님 지으신 모든 세계"]), &Style::default(), HD);
    assert_eq!(fonts.take_wanted().iter().map(|b| b.family).collect::<Vec<_>>(), ["Noto Sans KR"]);
}

#[test]
fn a_style_naming_a_lazy_face_uses_noto_sans_until_it_arrives() {
    let fonts = Fonts::builtin_lazy();
    assert_eq!(fonts.family(fonts.resolve("Noto Sans SC")), "Noto Sans");
    assert!(fonts.substitution("Noto Sans SC").is_none(), "not a substitution: it is on its way");
    assert_eq!(fonts.take_wanted().len(), 1);
    fonts.provide("NotoSansSC-Regular.otf", font_file("NotoSansSC-Regular.otf"));
    assert_eq!(fonts.family(fonts.resolve("Noto Sans SC")), "Noto Sans SC");
}

#[test]
fn a_face_that_cannot_be_fetched_stops_the_wait_and_keeps_its_placeholders() {
    let fonts = Fonts::builtin_lazy();
    let renderer = Renderer::with_fonts(fonts.clone());
    renderer.layout(&slide(&["礼物"]), &Style::default(), HD);
    let wanted = fonts.take_wanted();
    assert!(!fonts.provide(wanted[0].file, Arc::from(&b"not a font"[..])));
    fonts.font_failed(wanted[0].file);
    assert!(!fonts.waiting());
    assert_eq!(fonts.failed().len(), 1);
    renderer.layout(&slide(&["礼物"]), &Style::default(), HD);
    assert!(!fonts.waiting() && fonts.take_wanted().is_empty(), "not asked for again");
}
