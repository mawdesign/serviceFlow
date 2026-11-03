//! Font resolution and substitution (#9): system font, then a bundled font with the same
//! measurements, then Noto by script.

use std::sync::Arc;

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::song::Slide;
use serviceflow_render::{Fonts, Frame, Renderer, Size, Style, Substitution, SystemFonts};
use serviceflow_testkit::EngineHarness;

const HD: Size = Size { width: 1920, height: 1080 };

fn slide(lines: &[&str]) -> Slide {
    Slide { section: "V1".into(), lines: lines.iter().map(|l| l.to_string()).collect(), ..Slide::default() }
}

fn style_with(font: &str) -> Style {
    let mut style = Style::default();
    style.body.text.font = font.to_owned();
    style
}

fn family_of(fonts: &Fonts, name: &str) -> String {
    fonts.family(fonts.resolve(name))
}

fn lines(frame: &Frame) -> Vec<&str> {
    frame.boxes[0].lines.iter().map(|l| l.text.as_str()).collect()
}

fn notdef_count(frame: &Frame) -> usize {
    frame.boxes.iter().flat_map(|b| &b.lines).flat_map(|l| &l.runs).flat_map(|r| &r.glyphs).filter(|g| g.id == 0).count()
}

/// Pretends these families are installed, with the given font file.
struct Installed(&'static [&'static str], &'static [u8]);

impl SystemFonts for Installed {
    fn load(&self, family: &str) -> Option<(Arc<[u8]>, u32)> {
        self.0.iter().any(|f| f.eq_ignore_ascii_case(family)).then(|| (Arc::from(self.1), 0))
    }
}

#[test]
fn a_family_the_bundle_has_resolves_to_itself_without_a_substitution() {
    let fonts = Fonts::builtin();
    for name in ["Liberation Sans", "liberation serif", "Carlito", "Noto Sans"] {
        assert_eq!(family_of(&fonts, name).to_lowercase(), name.to_lowercase());
    }
    assert!(fonts.take_substitutions().is_empty());
}

#[test]
fn a_missing_font_gets_the_bundled_font_with_the_same_measurements() {
    let fonts = Fonts::builtin();
    assert_eq!(family_of(&fonts, "Calibri"), "Carlito");
    assert_eq!(family_of(&fonts, "Arial"), "Liberation Sans");
    assert_eq!(family_of(&fonts, "Helvetica"), "Liberation Sans");
    assert_eq!(family_of(&fonts, "Times New Roman"), "Liberation Serif");
    assert_eq!(family_of(&fonts, "times"), "Liberation Serif");
    assert_eq!(fonts.take_substitutions().len(), 5);
}

#[test]
fn any_other_missing_font_falls_back_to_noto() {
    let fonts = Fonts::builtin();
    assert_eq!(family_of(&fonts, "Zapfino Imaginary"), "Noto Sans");
    assert_eq!(fonts.take_substitutions(), [Substitution { requested: "Zapfino Imaginary".into(), used: "Noto Sans".into() }]);
}

#[test]
fn a_substitution_is_reported_once_and_can_be_asked_for_without_rendering() {
    let fonts = Fonts::builtin();
    assert_eq!(fonts.substitution("Calibri"), Some(Substitution { requested: "Calibri".into(), used: "Carlito".into() }));
    assert_eq!(fonts.substitution("Carlito"), None);
    fonts.resolve("calibri");
    fonts.resolve("Calibri");
    assert_eq!(fonts.take_substitutions().len(), 1);
    assert!(fonts.take_substitutions().is_empty());
}

#[test]
fn an_installed_font_comes_before_the_bundled_twin() {
    let fonts = Fonts::with_system_fonts(Arc::new(Installed(&["Calibri"], include_bytes!("../fonts/NotoSans-Regular.ttf"))));
    assert_eq!(family_of(&fonts, "Calibri"), "Calibri");
    assert!(fonts.take_substitutions().is_empty());
    // A family that is not installed still goes down the list.
    assert_eq!(family_of(&fonts, "Arial"), "Liberation Sans");
}

#[test]
fn a_style_naming_calibri_breaks_lines_as_with_carlito() {
    let renderer = Renderer::new();
    let long = "When peace like a river attendeth my way, when sorrows like sea billows roll, whatever my lot, thou hast taught me to say";
    let calibri = renderer.layout(&slide(&[long]), &style_with("Calibri"), HD);
    let carlito = renderer.layout(&slide(&[long]), &style_with("Carlito"), HD);
    assert!(lines(&calibri).len() >= 2);
    assert_eq!(calibri.snapshot(), carlito.snapshot());
    insta::assert_snapshot!(calibri.snapshot());
}

#[test]
fn rendering_with_a_missing_font_reports_the_substitution() {
    let renderer = Renderer::new();
    renderer.layout(&slide(&["Hello"]), &style_with("Calibri"), HD);
    renderer.layout(&slide(&["Hello again"]), &style_with("Calibri"), HD);
    assert_eq!(renderer.fonts().take_substitutions(), [Substitution { requested: "Calibri".into(), used: "Carlito".into() }]);
}

#[test]
fn chinese_text_renders_in_a_latin_style_without_missing_glyph_boxes() {
    let renderer = Renderer::new();
    let frame = renderer.layout(&slide(&["有一件礼物 你收到没有", "There is a gift"]), &style_with("Calibri"), HD);
    assert_eq!(notdef_count(&frame), 0);
    let families: Vec<String> = frame.boxes[0].lines[0].runs.iter().map(|r| renderer.fonts().family(r.font)).collect();
    assert!(families.contains(&"Noto Sans SC".to_string()), "{families:?}");
    // The Latin line stays in the styled font.
    assert!(frame.boxes[0].lines[1].runs.iter().all(|r| renderer.fonts().family(r.font) == "Carlito"));
}

#[test]
fn mixed_chinese_and_english_in_one_line_uses_both_fonts_in_order() {
    let renderer = Renderer::new();
    let japanese_and_korean = renderer.layout(&slide(&["主は生きておられる", "주님은 살아 계십니다", "祂賜下祂獨生愛子"]), &Style::default(), HD);
    assert_eq!(notdef_count(&japanese_and_korean), 0);
    let frame = renderer.layout(&slide(&["Jesus 耶稣 loves"]), &Style::default(), HD);
    let line = &frame.boxes[0].lines[0];
    let families: Vec<String> = line.runs.iter().map(|r| renderer.fonts().family(r.font)).collect();
    assert_eq!(families, ["Liberation Sans", "Noto Sans SC", "Liberation Sans"]);
    assert_eq!(notdef_count(&frame), 0);
    // Glyphs advance left to right across the font changes.
    let xs: Vec<f32> = line.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
    assert!(xs.windows(2).all(|w| w[0] <= w[1]), "{xs:?}");
}

#[test]
fn right_to_left_scripts_render_with_their_noto_font() {
    let renderer = Renderer::new();
    // Noto Sans (the last-resort font) has neither script.
    let frame = renderer.layout(&slide(&["שלום עולם", "مرحبا بالعالم"]), &style_with("Not Installed Anywhere"), HD);
    assert_eq!(notdef_count(&frame), 0);
    let hebrew = renderer.fonts().family(frame.boxes[0].lines[0].runs[0].font);
    let arabic = renderer.fonts().family(frame.boxes[0].lines[1].runs[0].font);
    assert_eq!((hebrew.as_str(), arabic.as_str()), ("Noto Sans Hebrew", "Noto Sans Arabic"));
}

fn fixture_frames(song: &str, style: &Style) -> (Renderer, Vec<Frame>) {
    let mut harness = EngineHarness::with_fixtures();
    harness.send(Command::OpenSong(SongId::new(song)));
    let state = harness.state();
    let open = state.song.as_ref().expect("the song opened");
    let renderer = Renderer::new();
    let frames = open.slides.iter().map(|s| renderer.layout(s, style, HD)).collect();
    (renderer, frames)
}

#[test]
fn the_cjk_fixture_songs_have_no_missing_glyphs_in_any_style() {
    for song in ["一件礼物 A Gift", "How Deep the Fathers Love + Chinese"] {
        for font in ["Liberation Sans", "Calibri", "Times New Roman", "Not Installed Anywhere"] {
            let (_, frames) = fixture_frames(song, &style_with(font));
            assert!(!frames.is_empty(), "{song}");
            for (i, frame) in frames.iter().enumerate() {
                let missing: Vec<&str> = frame
                    .boxes
                    .iter()
                    .flat_map(|b| &b.lines)
                    .filter(|l| l.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id == 0))
                    .map(|l| l.text.as_str())
                    .collect();
                assert!(missing.is_empty(), "{song} slide {} in {font}: {missing:?}", i + 1);
            }
        }
    }
}

#[test]
fn every_bundled_font_is_open_source_licensed() {
    let bundled = Fonts::bundled();
    assert!(bundled.len() >= 7);
    for font in bundled {
        assert!(font.licence_id == "OFL-1.1" || font.licence_id == "Apache-2.0", "{}", font.family);
        let text = font.licence_text.to_uppercase();
        let expected = if font.licence_id == "OFL-1.1" { "SIL OPEN FONT LICENSE" } else { "APACHE LICENSE" };
        assert!(text.contains(expected), "{} licence text", font.family);
    }
}
