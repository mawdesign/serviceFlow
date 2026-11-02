//! Drawing a laid-out [`Frame`] with vello_cpu. Glyphs are filled as outline paths (from skrifa),
//! which is what later outlines and shadows (#8) stroke and blur.

use std::collections::HashMap;
use std::sync::Mutex;

use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider as _};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, BezPath, Join, Rect, Stroke};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::fonts::{FontId, Fonts};
use crate::layout::Frame;
use serviceflow_core::style::Rgba;

/// The largest frame side vello_cpu can draw.
pub const MAX_SIDE: u32 = u16::MAX as u32;

/// A drawn frame: `width * height` pixels, row by row, as **premultiplied** RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// The premultiplied RGBA of the pixel at (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }
}

/// Glyph outlines in font units, by face and glyph.
#[derive(Default)]
pub(crate) struct Outlines(Mutex<HashMap<(FontId, u32), BezPath>>);

/// Shadow offset as a fraction of the font size.
const SHADOW_OFFSET: f32 = 0.06;

fn color(c: Rgba) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8(c.r, c.g, c.b, c.a)
}

pub(crate) fn draw(fonts: &Fonts, outlines: &Outlines, frame: &Frame) -> Image {
    let width = frame.size.width.clamp(1, MAX_SIDE) as u16;
    let height = frame.size.height.clamp(1, MAX_SIDE) as u16;
    let mut ctx = RenderContext::new(width, height);
    ctx.set_paint(color(frame.background));
    ctx.fill_rect(&Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));

    let mut cache = outlines.0.lock().unwrap_or_else(|e| e.into_inner());
    for line in frame.boxes.iter().flat_map(|b| &b.lines) {
        for run in &line.runs {
            let px = run.font_size;
            let scale = f64::from(px / fonts.units_per_em(run.font));
            let fx = run.effects;
            if let Some(fill) = fx.fill {
                let (ascent, descent) = fonts.vertical_metrics(run.font, px);
                ctx.set_transform(Affine::IDENTITY);
                ctx.set_paint(color(fill));
                let (x, top) = (f64::from(line.x), f64::from(line.baseline - ascent));
                ctx.fill_rect(&Rect::new(x, top, x + f64::from(line.width), f64::from(line.baseline - descent)));
            }
            // The shadow is the glyphs again, a little down and right.
            if let Some(shadow) = fx.shadow {
                let d = f64::from((px * SHADOW_OFFSET).max(1.0));
                ctx.set_paint(color(shadow));
                for g in &run.glyphs {
                    let path = cache.entry((run.font, g.id)).or_insert_with(|| outline(fonts, run.font, g.id));
                    ctx.set_transform(Affine::translate((f64::from(g.x) + d, f64::from(g.y) + d)) * Affine::scale_non_uniform(scale, -scale));
                    ctx.fill_path(path);
                }
            }
            // The outline is a stroke twice as wide as asked, covered by the fill, so it grows
            // outwards only.
            if let Some((outline_color, thickness)) = fx.outline {
                ctx.set_paint(color(outline_color));
                ctx.set_stroke(Stroke::new(f64::from(thickness) * 2.0 / scale).with_join(Join::Round));
                for g in &run.glyphs {
                    let path = cache.entry((run.font, g.id)).or_insert_with(|| outline(fonts, run.font, g.id));
                    ctx.set_transform(Affine::translate((f64::from(g.x), f64::from(g.y))) * Affine::scale_non_uniform(scale, -scale));
                    ctx.stroke_path(path);
                }
            }
            ctx.set_paint(color(run.color));
            for g in &run.glyphs {
                let path = cache.entry((run.font, g.id)).or_insert_with(|| outline(fonts, run.font, g.id));
                ctx.set_transform(Affine::translate((f64::from(g.x), f64::from(g.y))) * Affine::scale_non_uniform(scale, -scale));
                ctx.fill_path(path);
            }
            if fx.underline
                && let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last())
            {
                let (x0, x1) = (first.x.min(last.x), line.x + line.width);
                let y = f64::from(first.y + px * 0.12);
                ctx.set_transform(Affine::IDENTITY);
                ctx.set_paint(color(run.color));
                ctx.fill_rect(&Rect::new(f64::from(x0), y, f64::from(x1), y + f64::from((px * 0.06).max(1.0))));
            }
        }
    }
    drop(cache);

    ctx.flush();
    let mut pixmap = Pixmap::new(width, height);
    ctx.render(&mut pixmap, &mut Resources::new());
    Image { width: u32::from(width), height: u32::from(height), pixels: pixmap.data_as_u8_slice().to_vec() }
}

fn outline(fonts: &Fonts, font: FontId, glyph: u32) -> BezPath {
    let face = fonts.face(font);
    let font_ref = face.font_ref();
    let mut pen = Pen(BezPath::new());
    if let Some(g) = font_ref.outline_glyphs().get(GlyphId::new(glyph)) {
        let _ = g.draw(DrawSettings::unhinted(FontSize::unscaled(), LocationRef::default()), &mut pen);
    }
    pen.0
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to((f64::from(cx0), f64::from(cy0)), (f64::from(x), f64::from(y)));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to((f64::from(cx0), f64::from(cy0)), (f64::from(cx1), f64::from(cy1)), (f64::from(x), f64::from(y)));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
