//! The slide renderer (ADR-0005): a pure function from slide, style and output size to a laid-out
//! frame, which is then drawn.
//!
//! - [`Renderer::layout`] shapes with HarfRust, breaks lines and orders bidirectional text with
//!   craft-text, and returns a [`Frame`] that tests can inspect without drawing.
//! - [`Renderer::draw`] fills the frame's glyph outlines with vello_cpu into an [`Image`]
//!   (premultiplied RGBA8), which the UI uploads as a texture.
//!
//! No UI toolkit here: the crate runs the same natively and in the browser.

mod draw;
mod fonts;
mod layout;
mod style;

use serviceflow_core::song::Slide;

pub use draw::{Image, MAX_SIDE};
pub use fonts::{BUILTIN_FAMILY, FontId, Fonts};
pub use layout::{BoxRole, Frame, Glyph, GlyphRun, LINE_PITCH, Line, LineRole, Rect, Size, TextBox};
pub use style::{Align, Margins, REFERENCE_HEIGHT, Rgba, Style, TextStyle, VAlign};

/// Lays out and draws slides with a set of fonts. Keeps a cache of glyph outlines, so reuse one
/// renderer.
#[derive(Default)]
pub struct Renderer {
    fonts: Fonts,
    outlines: draw::Outlines,
}

impl Renderer {
    /// A renderer with the built-in font.
    pub fn new() -> Self {
        Self::with_fonts(Fonts::builtin())
    }

    pub fn with_fonts(fonts: Fonts) -> Self {
        Renderer { fonts, outlines: draw::Outlines::default() }
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// Lays out `slide` with `style` for an output of `size` pixels.
    pub fn layout(&self, slide: &Slide, style: &Style, size: Size) -> Frame {
        layout::layout(&self.fonts, slide, style, size)
    }

    /// Draws a laid-out frame.
    pub fn draw(&self, frame: &Frame) -> Image {
        draw::draw(&self.fonts, &self.outlines, frame)
    }

    /// Lays out and draws `slide`.
    pub fn render(&self, slide: &Slide, style: &Style, size: Size) -> Image {
        self.draw(&self.layout(slide, style, size))
    }
}
