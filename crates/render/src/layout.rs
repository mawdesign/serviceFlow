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

use crate::fonts::{FontId, Fonts};
use crate::style::{Align, REFERENCE_HEIGHT, Rgba, Style, TextStyle, VAlign};

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
    pub glyphs: Vec<Glyph>,
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
                    let _ = writeln!(out, "    run font {} rtl {} color {:?}: {}", run.font.0, run.rtl, run.color, glyphs.join(" "));
                }
            }
        }
        out
    }
}

/// Lays out `slide` with `style` for a frame of `size`.
pub(crate) fn layout(fonts: &Fonts, slide: &Slide, style: &Style, size: Size) -> Frame {
    let scale = size.height as f32 / REFERENCE_HEIGHT;
    let body = text_box(fonts, BoxRole::Body, LineRole::Lyric, &slide.lines, &style.body, size, scale);
    Frame { size, background: style.background, boxes: vec![body] }
}

fn text_box(fonts: &Fonts, role: BoxRole, line_role: LineRole, lines: &[String], style: &TextStyle, size: Size, scale: f32) -> TextBox {
    let m = style.margins;
    let rect = Rect {
        x: m.left * scale,
        y: m.top * scale,
        width: (size.width as f32 - (m.left + m.right) * scale).max(0.0),
        height: (size.height as f32 - (m.top + m.bottom) * scale).max(0.0),
    };
    let font = fonts.resolve(&style.font);
    let px = style.size * scale;

    // Each lyric line is a paragraph; `|` inside it forces a break.
    let mut placed: Vec<Line> = Vec::new();
    for paragraph in lines.iter().flat_map(|l| l.split('|')) {
        for range in break_lines(fonts, font, paragraph, px, rect.width) {
            placed.push(place_line(fonts, font, paragraph, range, px, style.color, line_role));
        }
    }

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
    TextBox { role, rect, lines: placed }
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
fn place_line(fonts: &Fonts, font: FontId, paragraph: &str, range: Range<usize>, px: f32, color: Rgba, role: LineRole) -> Line {
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
        runs.push(GlyphRun { font, font_size: px, color, rtl, glyphs });
    }
    Line { role, text: paragraph[range].to_owned(), font_size: px, x: 0.0, baseline: 0.0, width: pen, runs }
}
