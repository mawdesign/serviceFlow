//! How a slide looks: the input the layout reads besides the slide itself.
//!
//! Lengths are in reference pixels: pixels of a frame [`REFERENCE_HEIGHT`] pixels high. The
//! layout scales them by the output height, so a thumbnail lays out exactly like the full-size
//! output frame, only smaller.

/// The output height that style lengths are given for.
pub const REFERENCE_HEIGHT: f32 = 1080.0;

/// A colour in sRGB with straight (not premultiplied) alpha.
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
    /// Font family name, resolved through [`crate::Fonts`]. Until fonts are configurable (#9)
    /// every name resolves to the built-in face.
    pub font: String,
    /// Font size in reference pixels (the em size, as in CSS `px`).
    pub size: f32,
    pub color: Rgba,
    pub align: Align,
    pub valign: VAlign,
    pub margins: Margins,
}

/// A slide style. Only the body box for now; more boxes (title, translation, chords) are added
/// next to `body` as they are needed.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub background: Rgba,
    pub body: TextStyle,
}

impl Default for Style {
    /// The basic built-in style: white centred text on black.
    fn default() -> Self {
        Style {
            background: Rgba::BLACK,
            body: TextStyle {
                font: crate::fonts::BUILTIN_FAMILY.to_owned(),
                size: 72.0,
                color: Rgba::WHITE,
                align: Align::Center,
                valign: VAlign::Middle,
                margins: Margins::all(54.0),
            },
        }
    }
}
