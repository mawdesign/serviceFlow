//! The laid-out frame: everything the drawing step needs, and what snapshot tests inspect.
//!
//! All positions are absolute, in output pixels, with the origin at the frame's top left and y
//! growing downwards.

use std::fmt::Write as _;
use std::ops::Range;

use craft_text::bidi::{Direction, Paragraph};
use craft_text::linebreak::{Item, Params, Rag, break_paragraph};
use craft_text::segment::{Options, line_breaks};
use serviceflow_core::song::Slide;
use serviceflow_core::style::{Align, Captions, REFERENCE_HEIGHT, Rgba, Style, TextStyle, VAlign};

use crate::fonts::{FontId, Fonts};

/// Distance between baselines, as a multiple of the font size.
pub const LINE_PITCH: f32 = 1.2;

/// An output size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A slide laid out for one output size.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub size: Size,
    pub background: Rgba,
    pub boxes: Vec<TextBox>,
}

/// What a text box holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoxRole {
    /// The slide's lyrics.
    Body,
    /// The song title.
    Title,
    /// The song subtitle (author and so on).
    Subtitle,
}

/// A box of lines placed on the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub role: BoxRole,
    /// The area the lines are laid out in (inside the margins).
    pub rect: Rect,
    pub lines: Vec<Line>,
}

/// What a line holds. Lines of different roles may have different sizes and styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineRole {
    Lyric,
    Title,
    Subtitle,
}

/// One line of text as placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub role: LineRole,
    /// The line's text in logical order, without trailing spaces.
    pub text: String,
    pub font_size: f32,
    /// Left edge of the line's ink advance box.
    pub x: f32,
    pub baseline: f32,
    /// Total advance width.
    pub width: f32,
    /// Runs in visual order, left to right.
    pub runs: Vec<GlyphRun>,
}

impl Line {
    pub fn glyph_count(&self) -> usize {
        self.runs.iter().map(|r| r.glyphs.len()).sum()
    }
}

/// Glyphs with one font, size and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontId,
    pub font_size: f32,
    pub color: Rgba,
    pub rtl: bool,
    pub effects: Effects,
    pub glyphs: Vec<Glyph>,
}

/// What is drawn around and behind a run's glyphs. Lengths are output pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    /// Outline colour and thickness (the stroke grows this far outside the glyph).
    pub outline: Option<(Rgba, f32)>,
    /// Drop shadow colour. Its offset is derived from the font size at draw time.
    pub shadow: Option<Rgba>,
    /// A solid box behind the line.
    pub fill: Option<Rgba>,
    pub underline: bool,
}

/// A glyph's origin (on the baseline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

impl Frame {
    /// A stable text dump for snapshot tests: boxes, lines and glyph positions to 0.1 px.
    pub fn snapshot(&self) -> String {
        let mut out = String::new();
        let r = |v: f32| format!("{:.1}", v);
        let _ = writeln!(out, "frame {}x{} background {:?}", self.size.width, self.size.height, self.background);
        for b in &self.boxes {
            let _ = writeln!(out, "box {:?} at ({}, {}) size {}x{}", b.role, r(b.rect.x), r(b.rect.y), r(b.rect.width), r(b.rect.height));
            for l in &b.lines {
                let _ = writeln!(
                    out,
                    "  line {:?} {:?} size {} x {} baseline {} width {}",
                    l.role,
                    l.text,
                    r(l.font_size),
                    r(l.x),
                    r(l.baseline),
                    r(l.width)
                );
                for run in &l.runs {
                    let glyphs: Vec<String> = run.glyphs.iter().map(|g| format!("{}@{}", g.id, r(g.x))).collect();
                    let fx = if run.effects == Effects::default() { String::new() } else { format!(" fx {:?}", run.effects) };
                    let _ = writeln!(out, "    run font {} rtl {} color {:?}{}: {}", run.font.0, run.rtl, run.color, fx, glyphs.join(" "));
                }
            }
        }
        out
    }
}

/// Lays out `slide` with `style` for a frame of `size`.
///
/// The title and subtitle boxes are placed first, stacked from the top or the bottom of the frame
/// as their vertical alignment says; the body gets what is left. With auto-scale on, the body's
/// text shrinks (never grows) until it fits.
pub(crate) fn layout(fonts: &Fonts, slide: &Slide, captions: &Captions, style: &Style, size: Size) -> Frame {
    let scale = size.height as f32 / REFERENCE_HEIGHT;
    let (w, h) = (size.width as f32, size.height as f32);
    let thickness = style.outline_thickness * scale;

    let caption_boxes: [(BoxRole, LineRole, &TextStyle, &str); 2] = [
        (BoxRole::Title, LineRole::Title, &style.title.text, &captions.title),
        (BoxRole::Subtitle, LineRole::Subtitle, &style.subtitle.text, &captions.subtitle),
    ];
    // Shape each shown caption; its block height decides where it goes.
    let mut shown: Vec<(BoxRole, &TextStyle, Vec<Line>, f32)> = Vec::new();
    for (role, line_role, text_style, text) in caption_boxes {
        if !text_style.enabled || text.trim().is_empty() {
            continue;
        }
        let m = text_style.margins;
        let width = (w - (m.left + m.right) * scale).max(0.0);
        let lines = shape(fonts, line_role, std::slice::from_ref(&text.to_owned()), text_style, text_style.size * scale, width, thickness);
        let block = lines.len() as f32 * text_style.size * scale * LINE_PITCH;
        shown.push((role, text_style, lines, block));
    }

    let (mut top, mut bottom) = (0.0_f32, h);
    let mut placed: Vec<TextBox> = Vec::new();
    let count = shown.len();
    // Top-aligned boxes stack downwards in order; bottom-aligned ones stack upwards, so the
    // last one (the subtitle) sits lowest.
    for i in 0..count {
        let (role, st, lines, block) = &mut shown[i];
        if st.valign == VAlign::Bottom {
            continue;
        }
        let m = st.margins;
        let y = top + m.top * scale;
        top = y + *block + m.bottom * scale;
        placed.push(finish(fonts, *role, st, std::mem::take(lines), rect_at(m.left * scale, y, w - (m.left + m.right) * scale, *block), scale));
    }
    for i in (0..count).rev() {
        let (role, st, lines, block) = &mut shown[i];
        if st.valign != VAlign::Bottom {
            continue;
        }
        let m = st.margins;
        let y = bottom - m.bottom * scale - *block;
        bottom = y - m.top * scale;
        placed.push(finish(fonts, *role, st, std::mem::take(lines), rect_at(m.left * scale, y, w - (m.left + m.right) * scale, *block), scale));
    }
    placed.sort_by_key(|b| b.role != BoxRole::Title);

    // The body: what remains between the captions, inside its own margins.
    let m = style.body.text.margins;
    let top = top.max(m.top * scale);
    let bottom = bottom.min(h - m.bottom * scale);
    let rect = rect_at(m.left * scale, top, w - (m.left + m.right) * scale, (bottom - top).max(0.0));
    placed.push(body_box(fonts, &slide.lines, style, rect, scale, thickness));
    Frame { size, background: style.background.color, boxes: placed }
}

fn rect_at(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect { x, y, width: width.max(0.0), height: height.max(0.0) }
}

/// The smallest factor auto-scale will shrink text to.
const MIN_AUTO_SCALE: f32 = 0.1;

/// Lay the body's lines out in `rect`, shrinking them to fit when the style auto-scales.
fn body_box(fonts: &Fonts, lines: &[String], style: &Style, rect: Rect, scale: f32, thickness: f32) -> TextBox {
    let text = &style.body.text;
    let at = |factor: f32| shape(fonts, LineRole::Lyric, lines, text, text.size * scale * factor, rect.width, thickness);
    let fits = |shaped: &[Line], factor: f32| {
        let height = shaped.len() as f32 * text.size * scale * factor * LINE_PITCH;
        height <= rect.height + 0.01 && shaped.iter().all(|l| l.width <= rect.width + 0.5)
    };
    let mut factor = 1.0_f32;
    let mut shaped = at(factor);
    if style.body.auto_scale && !fits(&shaped, factor) {
        // Binary search for the largest factor that fits (DeckCraft's 12 steps).
        let (mut lo, mut hi) = (MIN_AUTO_SCALE, 1.0_f32);
        let mut best = None;
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            let candidate = at(mid);
            if fits(&candidate, mid) {
                best = Some((mid, candidate));
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (factor, shaped) = best.unwrap_or_else(|| (MIN_AUTO_SCALE, at(MIN_AUTO_SCALE)));
    }
    place(fonts, &mut shaped, text, rect, text.size * scale * factor);
    TextBox { role: BoxRole::Body, rect, lines: shaped }
}

/// Place a caption's lines in a rect that is exactly as tall as they are.
fn finish(fonts: &Fonts, role: BoxRole, style: &TextStyle, mut lines: Vec<Line>, rect: Rect, scale: f32) -> TextBox {
    place(fonts, &mut lines, style, rect, style.size * scale);
    TextBox { role, rect, lines }
}

/// Breaks and shapes lines at `px`, relative to each line's own origin. `|` forces a break.
fn shape(fonts: &Fonts, role: LineRole, lines: &[String], style: &TextStyle, px: f32, measure: f32, thickness: f32) -> Vec<Line> {
    let font = fonts.resolve(&style.font);
    let effects = Effects {
        outline: style.outline.then_some((style.outline_color, thickness)),
        shadow: style.shadow.then_some(style.shadow_color),
        fill: style.fill.then_some(style.fill_color),
        underline: style.underline,
    };
    let mut placed = Vec::new();
    for paragraph in lines.iter().flat_map(|l| l.split('|')) {
        for range in break_lines(fonts, font, paragraph, px, measure) {
            placed.push(place_line(fonts, font, paragraph, range, px, style.color, role, effects));
        }
    }
    placed
}

/// Put shaped lines in `rect`: the block by the vertical alignment, each line by the horizontal.
fn place(fonts: &Fonts, placed: &mut [Line], style: &TextStyle, rect: Rect, px: f32) {
    let font = fonts.resolve(&style.font);
    let pitch = px * LINE_PITCH;
    let block = pitch * placed.len() as f32;
    let top = match style.valign {
        VAlign::Top => rect.y,
        VAlign::Middle => rect.y + (rect.height - block) / 2.0,
        VAlign::Bottom => rect.y + rect.height - block,
    };
    let (ascent, descent) = fonts.vertical_metrics(font, px);
    let lead = (pitch - (ascent - descent)) / 2.0;
    for (i, line) in placed.iter_mut().enumerate() {
        let baseline = top + pitch * i as f32 + lead + ascent;
        let x = match style.align {
            Align::Left => rect.x,
            Align::Center => rect.x + (rect.width - line.width) / 2.0,
            Align::Right => rect.x + rect.width - line.width,
        };
        line.baseline = baseline;
        line.x = x;
        for run in &mut line.runs {
            for g in &mut run.glyphs {
                g.x += x;
                g.y += baseline;
            }
        }
    }
}

/// Breaks one paragraph into lines no wider than `measure` (except single words that are
/// wider), using craft-text's break opportunities and ragged line breaker. Returns byte ranges
/// without trailing spaces. An empty paragraph is one empty line.
fn break_lines(fonts: &Fonts, font: FontId, text: &str, px: f32, measure: f32) -> Vec<Range<usize>> {
    if text.trim().is_empty() {
        return std::iter::once(0..0).collect();
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    for b in line_breaks(text, &Options::default()) {
        if b.offset > start {
            pieces.push(start..b.offset);
            start = b.offset;
        }
    }
    if start < text.len() {
        pieces.push(start..text.len());
    }

    // Items and the byte range each covers.
    let mut items = Vec::new();
    let mut spans: Vec<Range<usize>> = Vec::new();
    let last = pieces.len() - 1;
    for (i, piece) in pieces.into_iter().enumerate() {
        let content_end = piece.start + text[piece.clone()].trim_end().len();
        if content_end > piece.start {
            items.push(Item::boxed(fonts.measure(font, &text[piece.start..content_end], px).into()));
            spans.push(piece.start..content_end);
        }
        if content_end < piece.end {
            items.push(Item::glue(fonts.measure(font, &text[content_end..piece.end], px).into(), 0.0, 0.0));
            spans.push(content_end..piece.end);
        } else if i != last {
            items.push(Item::penalty(0.0, 0.0, false));
            spans.push(piece.end..piece.end);
        }
    }
    items.push(Item::fill());
    items.push(Item::forced());
    spans.extend([text.len()..text.len(), text.len()..text.len()]);

    let ragged = Params { rag: Some(Rag::default()), ..Params::default() };
    let measure = f64::from(measure);
    let breaks = break_paragraph(&items, &|_| measure, &ragged, &[1.0, f64::INFINITY]);
    breaks
        .lines
        .iter()
        .map(|line| {
            let start = spans[line.start].start;
            let end = spans.get(line.end).map_or(text.len(), |s| s.start).max(start);
            start..start + text[start..end].trim_end().len()
        })
        .filter(|r| !r.is_empty())
        .collect()
}

/// Shapes one line in visual order, relative to its own origin (x 0 at the left of the line,
/// y 0 on the baseline).
#[allow(clippy::too_many_arguments)]
fn place_line(fonts: &Fonts, font: FontId, paragraph: &str, range: Range<usize>, px: f32, color: Rgba, role: LineRole, effects: Effects) -> Line {
    let bidi = Paragraph::new(paragraph, Direction::Auto);
    let mut pen = 0.0;
    let mut runs = Vec::new();
    for run in bidi.visual_runs(range.clone()) {
        let rtl = run.is_rtl();
        let glyphs = fonts
            .shape(font, &paragraph[run.range], rtl, px)
            .into_iter()
            .map(|g| {
                let glyph = Glyph { id: g.id, x: pen + g.x_offset, y: -g.y_offset };
                pen += g.advance;
                glyph
            })
            .collect();
        runs.push(GlyphRun { font, font_size: px, color, rtl, effects, glyphs });
    }
    Line { role, text: paragraph[range].to_owned(), font_size: px, x: 0.0, baseline: 0.0, width: pen, runs }
}
