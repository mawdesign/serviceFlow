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
mod logo;

use serviceflow_core::output::Logo;
use serviceflow_core::song::Slide;
use serviceflow_core::style::Captions;

pub use draw::{Image, MAX_SIDE};
pub use fonts::{Bundled, FontId, Fonts, Substitution, SystemFonts};
pub use layout::{BoxRole, Effects, Frame, Glyph, GlyphRun, LINE_PITCH, Line, LineRole, Rect, Size, TextBox};
pub use serviceflow_core::style::{Align, BUILTIN_FAMILY, Margins, REFERENCE_HEIGHT, Rgba, Style, TextStyle, VAlign};

/// Lays out and draws slides with a set of fonts. Keeps a cache of glyph outlines, so reuse one
/// renderer.
#[derive(Default)]
pub struct Renderer {
    fonts: Fonts,
    outlines: draw::Outlines,
    logos: logo::LogoCache,
}

impl Renderer {
    /// A renderer with the built-in font.
    pub fn new() -> Self {
        Self::with_fonts(Fonts::builtin())
    }

    pub fn with_fonts(fonts: Fonts) -> Self {
        Renderer { fonts, outlines: draw::Outlines::default(), logos: logo::LogoCache::default() }
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// Lays out `slide` with `style` for an output of `size` pixels, without title or subtitle.
    pub fn layout(&self, slide: &Slide, style: &Style, size: Size) -> Frame {
        self.layout_captioned(slide, &Captions::default(), style, size)
    }

    /// Lays out `slide` with its title and subtitle (`captions`).
    pub fn layout_captioned(&self, slide: &Slide, captions: &Captions, style: &Style, size: Size) -> Frame {
        layout::layout(&self.fonts, slide, captions, style, size)
    }

    /// Draws a laid-out frame.
    pub fn draw(&self, frame: &Frame) -> Image {
        draw::draw(&self.fonts, &self.outlines, frame)
    }

    /// Lays out and draws `slide`.
    pub fn render(&self, slide: &Slide, style: &Style, size: Size) -> Image {
        self.draw(&self.layout(slide, style, size))
    }

    /// Draws logo mode: `logo` centred over the style's background, shrunk to fit but never
    /// enlarged. A logo that cannot be decoded leaves just the background.
    pub fn render_logo(&self, logo: &Logo, style: &Style, size: Size) -> Image {
        let mut image = self.render(&Slide { section: String::new(), lines: Vec::new() }, style, size);
        if let Some(decoded) = self.logos.get(logo) {
            logo::composite(&mut image, &decoded);
        }
        image
    }

    /// Lays out and draws `slide` with its title and subtitle.
    pub fn render_captioned(&self, slide: &Slide, captions: &Captions, style: &Style, size: Size) -> Image {
        self.draw(&self.layout_captioned(slide, captions, style, size))
    }
}
