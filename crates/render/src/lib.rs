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

use std::sync::{Arc, Mutex};

use serviceflow_core::output::Logo;
use serviceflow_core::resolve::Images;
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Captions, ImageSource};

pub use draw::{Image, MAX_SIDE};
pub use fonts::{Bundled, FontId, Fonts, Substitution, SystemFonts};
pub use layout::{BoxRole, Effects, Fit, Frame, FrameImage, Glyph, GlyphRun, LINE_PITCH, Line, LineRole, Rect, Size, TextBox};
pub use logo::Picture;
pub use serviceflow_core::style::{Align, BUILTIN_FAMILY, Margins, REFERENCE_HEIGHT, Rgba, Style, TextStyle, VAlign};

/// Lays out and draws slides with a set of fonts. Keeps a cache of glyph outlines, so reuse one
/// renderer.
#[derive(Default)]
pub struct Renderer {
    fonts: Fonts,
    outlines: draw::Outlines,
    logos: logo::LogoCache,
    pictures: Mutex<Vec<(u64, Option<Arc<Picture>>)>>,
}

impl Renderer {
    /// A renderer with the built-in font.
    pub fn new() -> Self {
        Self::with_fonts(Fonts::builtin())
    }

    pub fn with_fonts(fonts: Fonts) -> Self {
        Renderer { fonts, outlines: draw::Outlines::default(), logos: logo::LogoCache::default(), pictures: Mutex::default() }
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
        self.layout_with_images(slide, captions, style, &Images::default(), size)
    }

    /// Lays out `slide` with its title and subtitle, and the style's background image: an
    /// embedded one, or a file of `Backgrounds/` that `images` holds. An image that is not
    /// there (yet) or cannot be decoded is left out, and then `background_as_text` hides nothing.
    pub fn layout_with_images(&self, slide: &Slide, captions: &Captions, style: &Style, images: &Images, size: Size) -> Frame {
        let picture = style.background.image.as_ref().and_then(|source| self.picture(source, images));
        layout::layout(&self.fonts, slide, captions, style, size, picture)
    }

    /// The decoded background image, cached by content (the last few are kept).
    fn picture(&self, source: &ImageSource, images: &Images) -> Option<Arc<Picture>> {
        let (key, decode): (u64, Box<dyn Fn() -> Option<Picture> + '_>) = match source {
            ImageSource::File(name) => {
                let bytes = images.bytes(name)?;
                (fnv(bytes), Box::new(move || logo::decode_bytes(bytes)))
            }
            ImageSource::Embedded(text) => (fnv(text.as_bytes()), Box::new(move || logo::decode_bytes(&logo::base64_decode(text)?))),
        };
        let mut cache = self.pictures.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, picture)) = cache.iter().find(|(k, _)| *k == key) {
            return picture.clone();
        }
        let picture = decode().map(Arc::new);
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((key, picture.clone()));
        picture
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
        let mut image = self.render(&Slide::default(), style, size);
        if let Some(decoded) = self.logos.get(logo) {
            logo::composite(&mut image, &decoded);
        }
        image
    }

    /// Lays out and draws `slide` with its title and subtitle.
    pub fn render_captioned(&self, slide: &Slide, captions: &Captions, style: &Style, size: Size) -> Image {
        self.draw(&self.layout_captioned(slide, captions, style, size))
    }

    /// Lays out and draws `slide` with its title, subtitle and background image.
    pub fn render_with_images(&self, slide: &Slide, captions: &Captions, style: &Style, images: &Images, size: Size) -> Image {
        self.draw(&self.layout_with_images(slide, captions, style, images, size))
    }
}

/// FNV-1a, to tell images apart in the cache.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}
