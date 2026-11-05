//! The library's style sidecar (ADR-0003): the default style as the operator edited it, in TOML.
//!
//! It holds the whole style in serviceFlow's own units (reference pixels at 1080 high), including
//! the extras OpenSong has no place for. A file without a `[default]` table means "no edits":
//! OpenSong's default style applies.

use serde::{Deserialize, Serialize};
use serviceflow_core::style::{
    Align, Background, BodyStyle, ImagePosition, ImageSource, Margins, Rgba, Style, SubtitleField, SubtitleStyle, TextStyle, TitleStyle, VAlign,
};

use crate::SidecarError;

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<StyleFile>,
}

#[derive(Serialize, Deserialize)]
struct StyleFile {
    title: TextFile,
    include_verse: bool,
    subtitle: TextFile,
    descriptive: bool,
    source: Vec<String>,
    body: TextFile,
    auto_scale: bool,
    highlight_chorus: bool,
    multilanguage_size: u32,
    multilanguage_color: String,
    blank_is_slide_change: bool,
    background: BackgroundFile,
    translation_italic: bool,
    pair_gap: f32,
    outline_thickness: f32,
    chord_size: u32,
    chord_color: String,
}

#[derive(Serialize, Deserialize)]
struct TextFile {
    enabled: bool,
    font: String,
    size: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    color: String,
    outline: bool,
    outline_color: String,
    shadow: bool,
    shadow_color: String,
    fill: bool,
    fill_color: String,
    align: String,
    valign: String,
    margins: [f32; 4],
}

#[derive(Serialize, Deserialize)]
struct BackgroundFile {
    color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedded: Option<String>,
    position: String,
}

fn colour(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r, c.g, c.b, c.a)
}

fn parse_colour(text: &str) -> Result<Rgba, SidecarError> {
    let bad = || SidecarError(format!("not a colour: {text:?}"));
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if hex.len() != 8 || !hex.is_ascii() {
        return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? })
}

fn text_file(t: &TextStyle) -> TextFile {
    TextFile {
        enabled: t.enabled,
        font: t.font.clone(),
        size: t.size,
        bold: t.bold,
        italic: t.italic,
        underline: t.underline,
        color: colour(t.color),
        outline: t.outline,
        outline_color: colour(t.outline_color),
        shadow: t.shadow,
        shadow_color: colour(t.shadow_color),
        fill: t.fill,
        fill_color: colour(t.fill_color),
        align: match t.align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
        .to_owned(),
        valign: match t.valign {
            VAlign::Top => "top",
            VAlign::Middle => "middle",
            VAlign::Bottom => "bottom",
        }
        .to_owned(),
        margins: [t.margins.left, t.margins.top, t.margins.right, t.margins.bottom],
    }
}

fn text_style(t: TextFile) -> Result<TextStyle, SidecarError> {
    Ok(TextStyle {
        enabled: t.enabled,
        font: t.font,
        size: t.size,
        bold: t.bold,
        italic: t.italic,
        underline: t.underline,
        color: parse_colour(&t.color)?,
        outline: t.outline,
        outline_color: parse_colour(&t.outline_color)?,
        shadow: t.shadow,
        shadow_color: parse_colour(&t.shadow_color)?,
        fill: t.fill,
        fill_color: parse_colour(&t.fill_color)?,
        align: match t.align.as_str() {
            "left" => Align::Left,
            "right" => Align::Right,
            _ => Align::Center,
        },
        valign: match t.valign.as_str() {
            "top" => VAlign::Top,
            "bottom" => VAlign::Bottom,
            _ => VAlign::Middle,
        },
        margins: Margins { left: t.margins[0], top: t.margins[1], right: t.margins[2], bottom: t.margins[3] },
    })
}

fn field_name(f: SubtitleField) -> &'static str {
    match f {
        SubtitleField::Copyright => "copyright",
        SubtitleField::HymnNumber => "hymn_number",
        SubtitleField::Author => "author",
        SubtitleField::Ccli => "ccli",
    }
}

fn position_name(p: ImagePosition) -> &'static str {
    match p {
        ImagePosition::Stretch => "stretch",
        ImagePosition::Center => "center",
        ImagePosition::Tile => "tile",
        ImagePosition::Cover => "cover",
    }
}

/// Read the library's style sidecar: the edited default style, or `None` when it holds none.
pub fn read_styles_sidecar(bytes: &[u8]) -> Result<Option<Style>, SidecarError> {
    let text = std::str::from_utf8(bytes).map_err(|e| SidecarError(format!("not UTF-8: {e}")))?;
    let file: File = toml::from_str(text).map_err(|e| SidecarError(e.message().to_owned()))?;
    let Some(s) = file.default else { return Ok(None) };
    let source = s
        .source
        .iter()
        .filter_map(|name| {
            [SubtitleField::Copyright, SubtitleField::HymnNumber, SubtitleField::Author, SubtitleField::Ccli]
                .into_iter()
                .find(|f| field_name(*f) == name)
        })
        .collect();
    let image = match (s.background.file, s.background.embedded) {
        (Some(file), _) => Some(ImageSource::File(file)),
        (None, Some(data)) => Some(ImageSource::Embedded(data)),
        (None, None) => None,
    };
    let default = Style::default();
    Ok(Some(Style {
        title: TitleStyle { text: text_style(s.title)?, include_verse: s.include_verse },
        subtitle: SubtitleStyle { text: text_style(s.subtitle)?, descriptive: s.descriptive, source },
        body: BodyStyle {
            text: text_style(s.body)?,
            auto_scale: s.auto_scale,
            highlight_chorus: s.highlight_chorus,
            multilanguage_size: s.multilanguage_size,
            multilanguage_color: parse_colour(&s.multilanguage_color)?,
            blank_is_slide_change: s.blank_is_slide_change,
        },
        background: Background {
            color: parse_colour(&s.background.color)?,
            image,
            position: [ImagePosition::Stretch, ImagePosition::Center, ImagePosition::Tile, ImagePosition::Cover]
                .into_iter()
                .find(|p| position_name(*p) == s.background.position)
                .unwrap_or(default.background.position),
            ..default.background
        },
        translation_italic: s.translation_italic,
        pair_gap: s.pair_gap,
        outline_thickness: s.outline_thickness,
        chord_size: s.chord_size,
        chord_color: parse_colour(&s.chord_color)?,
    }))
}

/// Write the library's style sidecar. `None` writes that there are no edits. Writing is stable
/// (same style, same bytes).
pub fn write_styles_sidecar(style: Option<&Style>) -> Vec<u8> {
    let default = style.map(|s| StyleFile {
        title: text_file(&s.title.text),
        include_verse: s.title.include_verse,
        subtitle: text_file(&s.subtitle.text),
        descriptive: s.subtitle.descriptive,
        source: s.subtitle.source.iter().map(|f| field_name(*f).to_owned()).collect(),
        body: text_file(&s.body.text),
        auto_scale: s.body.auto_scale,
        highlight_chorus: s.body.highlight_chorus,
        multilanguage_size: s.body.multilanguage_size,
        multilanguage_color: colour(s.body.multilanguage_color),
        blank_is_slide_change: s.body.blank_is_slide_change,
        background: BackgroundFile {
            color: colour(s.background.color),
            file: match &s.background.image {
                Some(ImageSource::File(f)) => Some(f.clone()),
                _ => None,
            },
            embedded: match &s.background.image {
                Some(ImageSource::Embedded(d)) => Some(d.clone()),
                _ => None,
            },
            position: position_name(s.background.position).to_owned(),
        },
        translation_italic: s.translation_italic,
        pair_gap: s.pair_gap,
        outline_thickness: s.outline_thickness,
        chord_size: s.chord_size,
        chord_color: colour(s.chord_color),
    });
    let file = File { version: VERSION, default };
    let mut text = toml::to_string(&file).unwrap_or_else(|_| format!("version = {VERSION}\n"));
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.into_bytes()
}
