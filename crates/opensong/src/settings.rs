//! `Settings/PresentSettings` (opensong.md §9, §10, §13): read-only import of the default and
//! scripture styles. serviceFlow never writes OpenSong's settings files.

use std::sync::Arc;

use serviceflow_core::output::{InitialMode, Logo, OutputSettings, SlideColors, SlideKind};
use serviceflow_core::style::{Align, ImagePosition, ImageSource, Margins, Rgba, Style, Styles, SubtitleField, TextStyle, VAlign};

use crate::CodecError;
use crate::xml::{self, Element};

/// OpenSong lays out for a 640 px wide screen; serviceFlow styles are for a 1080 px high one.
/// At 16:9 that is 1920 px wide, so every OpenSong length is multiplied by 3.
const SCALE: f32 = 3.0;
/// OpenSong adds a border of a fiftieth of the screen width (12.8 at 640) inside every margin.
const BORDER: f32 = 640.0 / 50.0;

/// Read the default and scripture styles from PresentSettings. A missing style, element or
/// attribute keeps serviceFlow's default for it.
pub fn read_present_settings(bytes: &[u8]) -> Result<Styles, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "present_settings" {
        return Err(CodecError(format!("not present settings: root element is <{}>", root.name)));
    }
    let global = root.child("style");
    let blank_is_slide_change = global.is_some_and(|g| flag(g, "blank_is_slide_change", false));
    // `style/@thickness`: OpenSong's outline thickness, global to all styles.
    let thickness = global.and_then(|g| number(g, "thickness"));
    let read = |name: &str| {
        let mut style = root.child(name).map_or_else(Style::default, style);
        style.body.blank_is_slide_change = blank_is_slide_change;
        if let Some(t) = thickness {
            style.outline_thickness = t * SCALE;
        }
        style
    };
    Ok(Styles { default: read("default_style"), scripture: read("scripture_style") })
}

/// Read a `<style>`-shaped element (opensong.md §9): a song's or set item's style, or the
/// default and scripture styles.
pub(crate) fn style(e: &Element) -> Style {
    let mut s = Style::default();
    if let Some(t) = e.child("title") {
        text_style(t, &mut s.title.text);
        s.title.include_verse = flag(t, "include_verse", s.title.include_verse);
    }
    if let Some(t) = e.child("subtitle") {
        text_style(t, &mut s.subtitle.text);
        s.subtitle.descriptive = flag(t, "descriptive", s.subtitle.descriptive);
    }
    if let Some(src) = e.child("song_subtitle") {
        s.subtitle.source = src
            .text()
            .split('|')
            .filter_map(|f| match f.trim().to_ascii_lowercase().as_str() {
                "copyright" => Some(SubtitleField::Copyright),
                "hymn_number" => Some(SubtitleField::HymnNumber),
                "author" => Some(SubtitleField::Author),
                "ccli_number" | "ccli" => Some(SubtitleField::Ccli),
                _ => None,
            })
            .collect();
    }
    if let Some(b) = e.child("body") {
        text_style(b, &mut s.body.text);
        s.body.auto_scale = flag(b, "auto_scale", true);
        s.body.highlight_chorus = flag(b, "highlight_chorus", s.body.highlight_chorus);
        // OpenSong treats 0 or missing as 70.
        s.body.multilanguage_size = number(b, "multilanguage_size").filter(|n| *n >= 1.0).map_or(70, |n| n as u32);
        if let Some(c) = colour(b, "multilanguage_color") {
            s.body.multilanguage_color = c;
        }
    }
    if let Some(b) = e.child("background") {
        if let Some(c) = colour(b, "color") {
            s.background.color = c;
        }
        s.background.position = match b.attr("position").and_then(|p| p.trim().parse::<u32>().ok()) {
            Some(1) => ImagePosition::Stretch,
            Some(2) => ImagePosition::Center,
            Some(3) => ImagePosition::Tile,
            _ => ImagePosition::Cover,
        };
        s.background.image = match (b.attr("filename"), b.text().trim()) {
            (Some(f), _) if !f.is_empty() => Some(ImageSource::File(f.to_owned())),
            (_, data) if !data.is_empty() => Some(ImageSource::Embedded(data.split_whitespace().collect())),
            _ => None,
        };
    }
    s
}

fn text_style(e: &Element, t: &mut TextStyle) {
    t.enabled = flag(e, "enabled", true);
    if let Some(font) = e.attr("font").filter(|f| !f.is_empty()) {
        t.font = font.to_owned();
    }
    if let Some(size) = number(e, "size") {
        t.size = size * SCALE;
    }
    t.bold = flag(e, "bold", t.bold);
    t.italic = flag(e, "italic", t.italic);
    t.underline = flag(e, "underline", t.underline);
    t.outline = flag(e, "border", t.outline);
    t.shadow = flag(e, "shadow", t.shadow);
    t.fill = flag(e, "fill", t.fill);
    for (attr, slot) in
        [("color", &mut t.color), ("border_color", &mut t.outline_color), ("shadow_color", &mut t.shadow_color), ("fill_color", &mut t.fill_color)]
    {
        if let Some(c) = colour(e, attr) {
            *slot = c;
        }
    }
    t.align = match e.attr("align").map(str::to_ascii_lowercase).as_deref() {
        Some("left") => Align::Left,
        Some("right") => Align::Right,
        Some("center") => Align::Center,
        _ => t.align,
    };
    t.valign = match e.attr("valign").map(str::to_ascii_lowercase).as_deref() {
        Some("top") => VAlign::Top,
        Some("bottom") => VAlign::Bottom,
        Some("middle") => VAlign::Middle,
        _ => t.valign,
    };
    let margin = |name: &str, old: f32| number(e, name).map_or(old, |m| (m + BORDER) * SCALE);
    t.margins = Margins {
        left: margin("margin-left", t.margins.left),
        right: margin("margin-right", t.margins.right),
        top: margin("margin-top", t.margins.top),
        bottom: margin("margin-bottom", t.margins.bottom),
    };
}

pub(crate) fn flag(e: &Element, name: &str, default: bool) -> bool {
    match e.attr(name).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("true" | "1") => true,
        Some("false" | "0") => false,
        _ => default,
    }
}

fn number(e: &Element, name: &str) -> Option<f32> {
    e.attr(name)?.trim().parse().ok()
}

fn colour(e: &Element, name: &str) -> Option<Rgba> {
    parse_colour(e.attr(name)?)
}

/// `#RRGGBB` or `#RRGGBBAA`, where OpenSong's alpha is inverted (`00` is opaque).
fn parse_colour(text: &str) -> Option<Rgba> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.is_ascii() || !(hex.len() == 6 || hex.len() == 8) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let alpha = if hex.len() == 8 { 255 - byte(6)? } else { 255 };
    Some(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: alpha })
}

/// Read how the output behaves from PresentSettings: logo, initial monitor and mode,
/// transitions and blanks. A missing element or attribute keeps the default.
pub fn read_output_settings(bytes: &[u8]) -> Result<OutputSettings, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "present_settings" {
        return Err(CodecError(format!("not present settings: root element is <{}>", root.name)));
    }
    let mut s = OutputSettings::default();
    if let Some(style) = root.child("style") {
        s.initial_mode = match style.attr("initial_mode").map(|m| m.trim().to_ascii_uppercase()).as_deref() {
            Some("B") => InitialMode::Black,
            Some("H") => InitialMode::Blank,
            Some("L") => InitialMode::Logo,
            _ => InitialMode::Normal,
        };
        s.blanks = flag(style, "blanks", s.blanks);
        s.blank_uses_next = flag(style, "blank_uses_next", s.blank_uses_next);
        s.transition = flag(style, "transition", s.transition);
        // OpenSong treats a time of 0 as 100 ms.
        s.transition_ms = number(style, "transition_time").filter(|t| *t >= 1.0).map_or(100, |t| t as u32);
    }
    // Screen numbers are 1-based.
    // Defaults to on when the element is missing.
    s.song_style_preferred =
        root.child("song_style_preferred").is_none_or(|e| !matches!(e.text().trim().to_ascii_lowercase().as_str(), "false" | "0"));
    s.monitor = root.child("monitors").and_then(|m| number(m, "present")).filter(|n| *n >= 1.0).map(|n| n as usize - 1);
    let data = |name: &str| {
        let text: String = root.child(name).map(Element::text).unwrap_or_default().split_whitespace().collect();
        (!text.is_empty()).then(|| Arc::<str>::from(text))
    };
    s.logo = data("logo").map(|image| Logo { image, mask: data("logo_mask") });
    Ok(s)
}

/// Read the thumbnail colours by slide kind from MainSettings (`slide_style_color`).
pub fn read_slide_colors(bytes: &[u8]) -> Result<SlideColors, CodecError> {
    let root = xml::parse(bytes)?;
    if root.name != "main_settings" {
        return Err(CodecError(format!("not main settings: root element is <{}>", root.name)));
    }
    let mut colors = SlideColors::default();
    if let Some(section) = root.child("slide_style_color") {
        colors.enabled = flag(section, "enabled", true);
        for (name, kind) in [
            ("verse", SlideKind::Verse),
            ("pre-chorus", SlideKind::PreChorus),
            ("chorus", SlideKind::Chorus),
            ("bridge", SlideKind::Bridge),
            ("tag", SlideKind::Tag),
            ("custom", SlideKind::Custom),
            ("scripture", SlideKind::Scripture),
            ("style", SlideKind::Style),
            ("image", SlideKind::Image),
            ("external", SlideKind::External),
            ("blank", SlideKind::Blank),
        ] {
            if let Some(c) = section.child(name).and_then(|e| colour(e, "color")) {
                colors.set(kind, c);
            }
        }
    }
    Ok(colors)
}
