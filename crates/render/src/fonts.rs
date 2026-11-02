//! The faces the renderer can use, and shaping with HarfRust.
//!
//! For now there is one built-in face, Liberation Sans (metric-compatible with Arial, SIL OFL 1.1,
//! see `fonts/LiberationSans-LICENSE.txt`), so output is the same on every platform and in the
//! browser. Ticket #9 adds system and bundled fonts behind [`Fonts::resolve`].

use std::sync::Arc;

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use skrifa::MetadataProvider as _;
use skrifa::instance::{LocationRef, Size};

/// The family name of the built-in face.
pub const BUILTIN_FAMILY: &str = "Liberation Sans";

static LIBERATION_SANS: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");

/// A face in a [`Fonts`] collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u16);

struct Face {
    family: String,
    data: Arc<[u8]>,
    shaper: ShaperData,
    units_per_em: f32,
    /// Ascent and descent per em (descent is negative).
    ascent: f32,
    descent: f32,
}

/// The faces available to the layout.
pub struct Fonts {
    faces: Vec<Face>,
}

/// One shaped glyph, in pixels at the size it was shaped for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shaped {
    pub id: u32,
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
}

impl Fonts {
    /// Just the built-in face.
    pub fn builtin() -> Self {
        let mut fonts = Fonts { faces: Vec::new() };
        fonts.add(BUILTIN_FAMILY, Arc::from(LIBERATION_SANS)).expect("the built-in font parses");
        fonts
    }

    /// Adds a face from font file bytes; `None` if they are not a font.
    pub fn add(&mut self, family: &str, data: Arc<[u8]>) -> Option<FontId> {
        let font = FontRef::new(&data).ok()?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(metrics.units_per_em);
        let face = Face {
            family: family.to_owned(),
            shaper: ShaperData::new(&font),
            units_per_em: upem,
            ascent: metrics.ascent / upem,
            descent: metrics.descent / upem,
            data: data.clone(),
        };
        self.faces.push(face);
        Some(FontId(u16::try_from(self.faces.len() - 1).ok()?))
    }

    /// The face for a family name: an exact (case-insensitive) match, else the first face.
    pub fn resolve(&self, family: &str) -> FontId {
        let index = self.faces.iter().position(|f| f.family.eq_ignore_ascii_case(family)).unwrap_or(0);
        FontId(index as u16)
    }

    pub fn family(&self, id: FontId) -> &str {
        &self.face(id).family
    }

    fn face(&self, id: FontId) -> &Face {
        &self.faces[usize::from(id.0)]
    }

    pub(crate) fn font_ref(&self, id: FontId) -> FontRef<'_> {
        FontRef::new(&self.face(id).data).expect("checked when added")
    }

    pub(crate) fn units_per_em(&self, id: FontId) -> f32 {
        self.face(id).units_per_em
    }

    /// Ascent and descent, in pixels at `px` (descent is negative).
    pub(crate) fn vertical_metrics(&self, id: FontId, px: f32) -> (f32, f32) {
        let face = self.face(id);
        (face.ascent * px, face.descent * px)
    }

    /// Shapes `text` as one run in one direction, at `px` pixels per em. Glyphs come out in
    /// visual order (left to right on the page), as HarfRust gives them.
    pub(crate) fn shape(&self, id: FontId, text: &str, rtl: bool, px: f32) -> Vec<Shaped> {
        let face = self.face(id);
        let font = self.font_ref(id);
        let shaper = face.shaper.shaper(&font).build();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
        let glyphs = shaper.shape(buffer, ShapeOptions::new());
        let scale = px / face.units_per_em;
        glyphs
            .glyph_infos()
            .iter()
            .zip(glyphs.glyph_positions())
            .map(|(info, pos)| Shaped {
                id: info.glyph_id,
                advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
                y_offset: pos.y_offset as f32 * scale,
            })
            .collect()
    }

    /// The advance width of `text` at `px`, shaped left to right.
    pub(crate) fn measure(&self, id: FontId, text: &str, px: f32) -> f32 {
        self.shape(id, text, false, px).iter().map(|g| g.advance).sum()
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::builtin()
    }
}
