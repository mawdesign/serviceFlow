//! Which style a slide is drawn with (GLOSSARY "Style", "Style stack"; opensong.md §12).
//!
//! Precedence, highest first:
//!
//! 1. the set item's own style,
//! 2. the song's own style, when `song_style_preferred` is on (or when no style item is in
//!    effect, as in OpenSong),
//! 3. the top of the style stack (pushed and reverted by style items),
//! 4. the default style, or the scripture style for scripture items. A scripture item without
//!    its own style uses the scripture style even while a style item is in effect (OpenSong).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::set::{ItemKind, Set, SetItem, StyleAction};
use crate::song::{Slide, Song};
use crate::style::{ImageSource, Resize, Style, Styles};

/// Which style a slide uses, without copying it. `Default` and `Scripture` follow the imported
/// styles, so re-importing them restyles what is already live.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum StyleChoice {
    #[default]
    Default,
    Scripture,
    /// A style of a set item, style item or song.
    Custom(Arc<Style>),
}

/// A background image a song brings for one slide (opensong.md §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideBackground {
    pub image: ImageSource,
    pub resize: Resize,
    pub keep_aspect: bool,
    /// Show the image without the lyrics.
    pub as_text: bool,
}

/// Everything that decides how one slide resolved besides its text: the style it uses and the
/// background image its song gives it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedStyle {
    pub style: StyleChoice,
    pub background: Option<SlideBackground>,
}

impl ResolvedStyle {
    /// The style to draw with: the chosen one, with the song background laid over its
    /// background. A custom style borrows the imported outline thickness, which OpenSong keeps
    /// global.
    pub fn style<'a>(&'a self, styles: &'a Styles) -> Cow<'a, Style> {
        let mut style = match &self.style {
            StyleChoice::Default => Cow::Borrowed(&styles.default),
            StyleChoice::Scripture => Cow::Borrowed(&styles.scripture),
            StyleChoice::Custom(custom) if custom.outline_thickness == styles.default.outline_thickness => Cow::Borrowed(&**custom),
            StyleChoice::Custom(custom) => {
                let mut owned = (**custom).clone();
                owned.outline_thickness = styles.default.outline_thickness;
                Cow::Owned(owned)
            }
        };
        if let Some(bg) = &self.background {
            let owned = style.to_mut();
            owned.background.image = Some(bg.image.clone());
            owned.background.resize = Some(bg.resize);
            owned.background.keep_aspect = bg.keep_aspect;
            owned.background.as_text = bg.as_text;
        }
        style
    }

    /// The `Backgrounds/` files this look needs.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        let own = match &self.style {
            StyleChoice::Custom(style) => file_of(style),
            _ => None,
        };
        let bg = self.background.as_ref().and_then(|b| match &b.image {
            ImageSource::File(name) => Some(name.as_str()),
            ImageSource::Embedded(_) => None,
        });
        own.into_iter().chain(bg)
    }
}

/// The `Backgrounds/` file a style's background image names.
pub fn file_of(style: &Style) -> Option<&str> {
    match &style.background.image {
        Some(ImageSource::File(name)) => Some(name),
        _ => None,
    }
}

/// The style in effect on the style stack just before set item `before` (`None`: the stack is
/// at the default style). Style items push and revert (`new` pushes the item's style, `revert`
/// goes back one, a revert with nothing pushed does nothing; another action pushes only when
/// the item has a style).
pub fn stack_top(set: &Set, before: usize) -> Option<Arc<Style>> {
    let mut current: Option<Arc<Style>> = None;
    let mut stack: Vec<Option<Arc<Style>>> = Vec::new();
    for item in set.items.iter().take(before) {
        let ItemKind::Style { action, .. } = &item.kind else { continue };
        match action {
            StyleAction::New => {
                stack.push(current.clone());
                current = item.style.clone().or(current);
            }
            StyleAction::Revert => {
                if let Some(previous) = stack.pop() {
                    current = previous;
                }
            }
            StyleAction::Other => {
                if item.style.is_some() {
                    stack.push(current.clone());
                    current = item.style.clone();
                }
            }
        }
    }
    current
}

/// How deep the style stack is just before item `before`.
pub fn stack_depth(set: &Set, before: usize) -> usize {
    let mut depth = 0usize;
    for item in set.items.iter().take(before) {
        let ItemKind::Style { action, .. } = &item.kind else { continue };
        match action {
            StyleAction::New => depth += 1,
            StyleAction::Revert => depth = depth.saturating_sub(1),
            StyleAction::Other => depth += usize::from(item.style.is_some()),
        }
    }
    depth
}

/// The style for a song shown as `item` of `set` (or on its own: `set` is `None`).
pub fn choose(set: Option<(&Set, usize)>, song: Option<&Song>, song_style_preferred: bool) -> StyleChoice {
    let item: Option<&SetItem> = set.and_then(|(set, index)| set.items.get(index));
    let current = set.and_then(|(set, index)| stack_top(set, index));
    if let Some(own) = item.and_then(|i| i.style.clone()) {
        return StyleChoice::Custom(own);
    }
    if matches!(item.map(|i| &i.kind), Some(ItemKind::Placeholder { item_type, .. }) if item_type == "scripture") {
        return StyleChoice::Scripture;
    }
    if let Some(own) = song.and_then(|s| s.style.clone())
        && (current.is_none() || song_style_preferred)
    {
        return StyleChoice::Custom(own);
    }
    current.map_or(StyleChoice::Default, StyleChoice::Custom)
}

/// The background `song` gives `slide`, if any.
pub fn slide_background(song: &Song, slide: &Slide) -> Option<SlideBackground> {
    let backgrounds = song.backgrounds.as_ref()?;
    let image = backgrounds.image_for(&slide.section, slide.part)?.clone();
    Some(SlideBackground { image, resize: backgrounds.resize, keep_aspect: backgrounds.keep_aspect, as_text: backgrounds.as_text })
}

/// Background image files loaded from `Backgrounds/` (or still loading), by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Images {
    files: BTreeMap<String, ImageFile>,
}

/// The state of one background file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFile {
    /// Being read.
    Loading,
    /// Could not be read.
    Missing,
    Loaded(Arc<[u8]>),
}

impl Images {
    /// The bytes of a loaded file.
    pub fn bytes(&self, name: &str) -> Option<&Arc<[u8]>> {
        match self.files.get(name)? {
            ImageFile::Loaded(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub fn get(&self, name: &str) -> Option<&ImageFile> {
        self.files.get(name)
    }

    pub fn set(&mut self, name: String, file: ImageFile) {
        self.files.insert(name, file);
    }
}
