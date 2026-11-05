//! Style edits (GLOSSARY "Style"): what the style editor changes in the default style or in the
//! style a set item pushes, one property at a time.
//!
//! An edit of the default style lives in serviceFlow's own state and sidecar (ADR-0003); an edit
//! of a set item's style is a set edit, saved with the set in OpenSong's units (ADR-0002). Both
//! have undo and redo, and edits made one after another join into one step until
//! [`EndEditGroup`](crate::command::Command::EndEditGroup).
//!
//! Lengths are given in reference pixels and rounded to what OpenSong's file can hold
//! ([`snap_size`], [`snap_margin`]), so the saved style reads back as the editor showed it.

use std::sync::Arc;

use crate::style::{Align, ImagePosition, ImageSource, Rgba, Style, TextStyle, VAlign, snap_margin, snap_size};

/// Which style an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleTarget {
    /// The default style (not written to OpenSong's settings).
    Default,
    /// The style the set item with this index pushes (a style item, or any item with its own).
    Item(usize),
}

/// The part of a style an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StylePart {
    Title,
    Subtitle,
    Body,
    Background,
}

/// A side of a text box's margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

/// One property of a style part. A change the part does not have (a font for the background) is
/// ignored.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleChange {
    /// Text parts: drawn at all.
    Enabled(bool),
    Font(String),
    /// Font size in reference pixels.
    Size(f32),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    /// The text colour, or the background colour.
    Color(Rgba),
    Outline(bool),
    OutlineColor(Rgba),
    Shadow(bool),
    ShadowColor(Rgba),
    Fill(bool),
    FillColor(Rgba),
    Align(Align),
    VAlign(VAlign),
    /// A margin in reference pixels (never below OpenSong's own border).
    Margin(Side, f32),
    /// Body: shrink text to fit.
    AutoScale(bool),
    /// Body: flip italic on chorus slides.
    HighlightChorus(bool),
    /// Background: the image (`None`: no image).
    Image(Option<ImageSource>),
    /// Background: how the image fills the screen.
    Position(ImagePosition),
}

/// One change to a style.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleEdit {
    pub part: StylePart,
    pub change: StyleChange,
}

impl StyleEdit {
    pub fn new(part: StylePart, change: StyleChange) -> Self {
        StyleEdit { part, change }
    }

    /// The style with the edit applied; `None` when the part has no such property.
    pub fn applied(&self, style: &Style) -> Option<Style> {
        let mut style = style.clone();
        match (self.part, &self.change) {
            (StylePart::Background, change) => {
                let background = &mut style.background;
                match change {
                    StyleChange::Color(c) => background.color = *c,
                    StyleChange::Image(image) => background.image = image.clone(),
                    StyleChange::Position(p) => background.position = *p,
                    _ => return None,
                }
            }
            (StylePart::Body, StyleChange::AutoScale(on)) => style.body.auto_scale = *on,
            (StylePart::Body, StyleChange::HighlightChorus(on)) => style.body.highlight_chorus = *on,
            (part, change) => {
                let text = match part {
                    StylePart::Title => &mut style.title.text,
                    StylePart::Subtitle => &mut style.subtitle.text,
                    _ => &mut style.body.text,
                };
                set_text(text, change)?;
            }
        }
        Some(style)
    }
}

fn set_text(text: &mut TextStyle, change: &StyleChange) -> Option<()> {
    match change {
        StyleChange::Enabled(on) => text.enabled = *on,
        StyleChange::Font(font) => text.font = font.clone(),
        StyleChange::Size(size) => text.size = snap_size(size.max(1.0)),
        StyleChange::Bold(on) => text.bold = *on,
        StyleChange::Italic(on) => text.italic = *on,
        StyleChange::Underline(on) => text.underline = *on,
        StyleChange::Color(c) => text.color = *c,
        StyleChange::Outline(on) => text.outline = *on,
        StyleChange::OutlineColor(c) => text.outline_color = *c,
        StyleChange::Shadow(on) => text.shadow = *on,
        StyleChange::ShadowColor(c) => text.shadow_color = *c,
        StyleChange::Fill(on) => text.fill = *on,
        StyleChange::FillColor(c) => text.fill_color = *c,
        StyleChange::Align(a) => text.align = *a,
        StyleChange::VAlign(v) => text.valign = *v,
        StyleChange::Margin(side, margin) => {
            let margin = snap_margin(*margin);
            match side {
                Side::Left => text.margins.left = margin,
                Side::Top => text.margins.top = margin,
                Side::Right => text.margins.right = margin,
                Side::Bottom => text.margins.bottom = margin,
            }
        }
        StyleChange::AutoScale(_) | StyleChange::HighlightChorus(_) | StyleChange::Image(_) | StyleChange::Position(_) => return None,
    }
    Some(())
}

/// The undo history of the default style: earlier versions of it, newest last.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleHistory {
    pub(crate) undo: Vec<Arc<Style>>,
    pub(crate) redo: Vec<Arc<Style>>,
    /// Edits are still joining the last undo step.
    pub(crate) grouping: bool,
}

impl StyleHistory {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// serviceFlow's own copy of the default style (ADR-0003): what the operator made of it with the
/// style editor, kept in the library's `.serviceflow/` folder and never written to OpenSong's
/// settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DefaultStyle {
    /// The edited default style; `None` while the default is OpenSong's as imported.
    pub edited: Option<Arc<Style>>,
    pub history: StyleHistory,
    /// Changes not yet written to the sidecar (written when the undo step ends).
    pub(crate) unsaved: bool,
}

impl DefaultStyle {
    /// Whether the default style differs from OpenSong's.
    pub fn is_edited(&self) -> bool {
        self.edited.is_some()
    }
}
