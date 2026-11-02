//! Slide thumbnails: frames from the slide renderer (ADR-0005), uploaded as egui textures.
//!
//! Textures are cached per song (by `Arc` identity) and pixel size, so a frame where nothing
//! changed renders nothing.

use std::sync::Arc;

use serviceflow_core::song::Song;
use serviceflow_core::state::OpenSong;
use serviceflow_core::style::{Captions, Styles};
use serviceflow_render::{Image, Renderer, Size};

/// Turns a drawn frame (premultiplied RGBA8) into an egui image.
pub fn color_image(image: &Image) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.pixels)
}

/// The thumbnails of the open song.
pub struct Thumbnails {
    renderer: Renderer,
    key: Option<(Arc<Song>, Arc<Styles>, Size)>,
    textures: Vec<egui::TextureHandle>,
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self { renderer: Renderer::with_fonts(crate::fonts::shared()), key: None, textures: Vec::new() }
    }
}

impl Thumbnails {
    /// One texture per slide of `open`, drawn at `size` pixels.
    pub fn for_song(&mut self, ctx: &egui::Context, open: &OpenSong, styles: &Arc<Styles>, size: Size) -> &[egui::TextureHandle] {
        let fresh = matches!(&self.key, Some((song, st, s)) if Arc::ptr_eq(song, &open.song) && Arc::ptr_eq(st, styles) && *s == size);
        if !fresh {
            let captions = Captions::for_song(&open.song, &styles.default);
            self.textures = open
                .slides
                .iter()
                .enumerate()
                .map(|(i, slide)| {
                    let image = self.renderer.render_captioned(slide, &captions, &styles.default, size);
                    ctx.load_texture(format!("slide thumbnail {}", i + 1), color_image(&image), egui::TextureOptions::LINEAR)
                })
                .collect();
            self.key = Some((open.song.clone(), Arc::clone(styles), size));
        }
        &self.textures
    }
}
