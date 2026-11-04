//! Slide thumbnails: frames from the slide renderer (ADR-0005), uploaded as egui textures.
//!
//! Textures are cached per list of slides and pixel size, so a frame where nothing changed
//! renders nothing.

use std::sync::Arc;

use serviceflow_core::resolve::{Images, ResolvedStyle};
use serviceflow_core::song::Slide;
use serviceflow_core::state::State;
use serviceflow_core::style::{Captions, Styles};
use serviceflow_render::{Image, Renderer, Size};

/// Turns a drawn frame (premultiplied RGBA8) into an egui image.
pub fn color_image(image: &Image) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.pixels)
}

/// What the thumbnails were drawn from: the slides, how each is styled, the styles and background
/// images, and the size.
type Key = (Vec<(Slide, ResolvedStyle)>, Arc<Styles>, Arc<Images>, Size);

/// The thumbnails of the open song.
pub struct Thumbnails {
    renderer: Renderer,
    key: Option<Key>,
    textures: Vec<egui::TextureHandle>,
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self { renderer: Renderer::with_fonts(crate::fonts::shared()), key: None, textures: Vec::new() }
    }
}

impl Thumbnails {
    /// One texture per slide of the open song, drawn at `size` pixels with the style each slide
    /// has in `state` (set item, song, style stack) and its song background. When a blank follows
    /// the item (GLOSSARY "Blank"), one more: the blank, as the last.
    pub fn for_song(&mut self, ctx: &egui::Context, state: &State, size: Size) -> &[egui::TextureHandle] {
        let Some(open) = &state.song else { return &[] };
        let mut resolved: Vec<(Slide, ResolvedStyle)> =
            open.slides.iter().map(|slide| (slide.clone(), state.resolved_of(&open.id, &open.song, slide))).collect();
        let slides = resolved.len();
        resolved.extend(state.blank_after().map(|blank| (Slide::default(), blank)));
        let fresh = matches!(&self.key, Some((slides, st, im, s)) if *slides == resolved && Arc::ptr_eq(st, &state.styles) && Arc::ptr_eq(im, &state.images) && *s == size);
        if !fresh {
            self.textures = resolved
                .iter()
                .enumerate()
                .map(|(i, (slide, resolved))| {
                    let style = resolved.style(&state.styles);
                    // The blank has no text, so no title either.
                    let captions = if i < slides { Captions::for_song(&open.song, &style) } else { Captions::default() };
                    let image = self.renderer.render_with_images(slide, &captions, &style, &state.images, size);
                    let name = if i < slides { format!("slide thumbnail {}", i + 1) } else { "blank thumbnail".to_owned() };
                    ctx.load_texture(name, color_image(&image), egui::TextureOptions::LINEAR)
                })
                .collect();
            self.key = Some((resolved, Arc::clone(&state.styles), Arc::clone(&state.images), size));
        }
        &self.textures
    }
}
