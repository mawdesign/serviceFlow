//! How a slide looks (GLOSSARY "Style"): the model the renderer reads and the OpenSong codec
//! fills. It is a superset of OpenSong's style.
//!
//! Lengths are in reference pixels: pixels of a frame [`REFERENCE_HEIGHT`] pixels high. The
//! renderer scales them by the output height, so a thumbnail lays out exactly like the full-size
//! output, only smaller.

use crate::song::Song;

/// The output height that style lengths are given for.
pub const REFERENCE_HEIGHT: f32 = 1080.0;

/// The family of the built-in font.
pub const BUILTIN_FAMILY: &str = "Liberation Sans";

/// A colour in sRGB with straight (not premultiplied) alpha. `a` 255 is opaque. (OpenSong's
/// alpha is inverted; the codec converts.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Rgba { r, g, b, a: 255 }
    }
}

/// Horizontal alignment of each line within its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

/// Vertical alignment of the block of lines within its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Space between the frame edge and a text box, in reference pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margins {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Margins {
    pub const fn all(m: f32) -> Self {
        Margins { left: m, top: m, right: m, bottom: m }
    }
}

/// The look of one text box.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Drawn at all (OpenSong `enabled`).
    pub enabled: bool,
    /// Font family name, resolved through the renderer's fonts.
    pub font: String,
    /// Font size in reference pixels (the em size, as in CSS `px`).
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Rgba,
    pub outline: bool,
    pub outline_color: Rgba,
    pub shadow: bool,
    pub shadow_color: Rgba,
    /// A solid box behind each line of text.
    pub fill: bool,
    pub fill_color: Rgba,
    pub align: Align,
    pub valign: VAlign,
    pub margins: Margins,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            enabled: true,
            font: BUILTIN_FAMILY.to_owned(),
            size: 72.0,
            bold: false,
            italic: false,
            underline: false,
            color: Rgba::WHITE,
            outline: false,
            outline_color: Rgba::BLACK,
            shadow: false,
            shadow_color: Rgba::rgb(128, 128, 128),
            fill: false,
            fill_color: Rgba::BLACK,
            align: Align::Center,
            valign: VAlign::Middle,
            margins: Margins::all(54.0),
        }
    }
}

/// The body box: lyrics, with the settings only it has.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyStyle {
    pub text: TextStyle,
    /// Shrink the text until the slide fits the screen.
    pub auto_scale: bool,
    /// Flip the italic of chorus slides.
    pub highlight_chorus: bool,
    /// Translation line size as a percentage of the (auto-scaled) body size.
    pub multilanguage_size: u32,
    pub multilanguage_color: Rgba,
    /// A blank line in the lyrics starts a new slide. (OpenSong keeps this in the global
    /// `style` element; the import copies it into every style.)
    pub blank_is_slide_change: bool,
}

/// The title box.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleStyle {
    pub text: TextStyle,
    /// Append the verse indicator to the title.
    pub include_verse: bool,
}

/// The subtitle box.
#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleStyle {
    pub text: TextStyle,
    /// Caption each part ("Author: ...").
    pub descriptive: bool,
    /// What the subtitle is made of, in order.
    pub source: Vec<SubtitleField>,
}

/// A song field that can go in the subtitle (OpenSong `song_subtitle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubtitleField {
    Copyright,
    HymnNumber,
    Author,
    Ccli,
}

impl SubtitleField {
    fn of(self, song: &Song) -> &str {
        match self {
            SubtitleField::Copyright => &song.copyright,
            SubtitleField::HymnNumber => &song.hymn_number,
            SubtitleField::Author => &song.author,
            SubtitleField::Ccli => &song.ccli,
        }
    }
}

/// How the background image fills the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImagePosition {
    /// Stretch to the screen.
    Stretch,
    /// Centre at natural size.
    Center,
    /// Tile.
    Tile,
    /// Scale to cover the screen (OpenSong's fallback for any other value).
    #[default]
    Cover,
}

/// Where a background image comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    /// A file under `Backgrounds/`.
    File(String),
    /// Base64 text embedded in the style, as OpenSong stores it.
    Embedded(String),
}

/// What area a song or item background image is fitted to (OpenSong `backgrounds/@resize`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Resize {
    /// The whole screen.
    #[default]
    Screen,
    /// The body text area.
    Body,
    /// Natural size, centred.
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    pub color: Rgba,
    pub image: Option<ImageSource>,
    pub position: ImagePosition,
    /// Song and item backgrounds: the area the image is fitted to. `None`: a style's own image,
    /// which is placed by `position`.
    pub resize: Option<Resize>,
    /// Song and item backgrounds: keep the image's proportions (fit inside the area, centred)
    /// instead of stretching it to the area.
    pub keep_aspect: bool,
    /// Song backgrounds: show the image without the lyrics (OpenSong `background_as_text`).
    pub as_text: bool,
}

/// The full visual specification of a slide.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub title: TitleStyle,
    pub subtitle: SubtitleStyle,
    pub body: BodyStyle,
    pub background: Background,
    /// serviceFlow extra: translation lines flip italic against the body (OpenSong parity).
    pub translation_italic: bool,
    /// serviceFlow extra: gap before each primary line after the first, as a fraction of the
    /// translation line height.
    pub pair_gap: f32,
    /// serviceFlow extra: outline thickness in reference pixels. (OpenSong: global, in the
    /// style/@thickness setting.)
    pub outline_thickness: f32,
    /// serviceFlow extra: chord size as a percentage of the body size (after auto-scale).
    pub chord_size: u32,
    /// serviceFlow extra: chord colour.
    pub chord_color: Rgba,
}

impl Default for Style {
    /// The basic built-in style: white centred text on black, auto-scale off.
    fn default() -> Self {
        let small = TextStyle { size: 40.0, valign: VAlign::Bottom, ..TextStyle::default() };
        Style {
            title: TitleStyle { text: TextStyle { valign: VAlign::Top, size: 54.0, ..small.clone() }, include_verse: false },
            subtitle: SubtitleStyle { text: small, descriptive: false, source: vec![SubtitleField::Author] },
            body: BodyStyle {
                text: TextStyle::default(),
                auto_scale: false,
                highlight_chorus: false,
                multilanguage_size: 70,
                multilanguage_color: Rgba::rgb(220, 220, 220),
                blank_is_slide_change: false,
            },
            background: Background {
                color: Rgba::BLACK,
                image: None,
                position: ImagePosition::Cover,
                resize: None,
                keep_aspect: false,
                as_text: false,
            },
            translation_italic: true,
            pair_gap: 0.2,
            outline_thickness: 2.0,
            chord_size: 60,
            chord_color: Rgba::rgb(255, 200, 0),
        }
    }
}

/// The two styles imported from OpenSong's present settings (GLOSSARY "Default style",
/// "Scripture style").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styles {
    pub default: Style,
    pub scripture: Style,
}

/// The texts of the title and subtitle boxes for a slide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captions {
    pub title: String,
    pub subtitle: String,
}

impl Captions {
    /// The song's title, and its subtitle as `style` selects it (the non-empty fields, joined
    /// with " - ", or with a caption each when the style is descriptive).
    pub fn for_song(song: &Song, style: &Style) -> Captions {
        if let Some(own) = &song.captions {
            return own.clone();
        }
        let parts: Vec<String> = style
            .subtitle
            .source
            .iter()
            .filter(|f| !f.of(song).trim().is_empty())
            .map(|f| {
                let value = f.of(song).trim();
                if style.subtitle.descriptive {
                    let caption = match f {
                        SubtitleField::Copyright => "Copyright",
                        SubtitleField::HymnNumber => "Hymn number",
                        SubtitleField::Author => "Author",
                        SubtitleField::Ccli => "CCLI",
                    };
                    format!("{caption}: {value}")
                } else {
                    value.to_owned()
                }
            })
            .collect();
        Captions { title: song.title.clone(), subtitle: parts.join(" - ") }
    }
}
