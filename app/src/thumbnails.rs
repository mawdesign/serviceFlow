//! Slide thumbnails: frames from the slide renderer (ADR-0005), uploaded as egui textures.
//!
//! Textures are cached per list of slides and pixel size, so a frame where nothing changed
//! renders nothing.

use serviceflow_core::song::Slide;
use serviceflow_core::state::OpenSong;
use serviceflow_render::{Image, Renderer, Size, Style};

/// Turns a drawn frame (premultiplied RGBA8) into an egui image.
pub fn color_image(image: &Image) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.pixels)
}

/// The thumbnails of the open song.
#[derive(Default)]
pub struct Thumbnails {
    renderer: Renderer,
    style: Style,
    key: Option<(Vec<Slide>, Size)>,
    textures: Vec<egui::TextureHandle>,
}

impl Thumbnails {
    /// One texture per slide of `open`, drawn at `size` pixels.
    pub fn for_song(&mut self, ctx: &egui::Context, open: &OpenSong, size: Size) -> &[egui::TextureHandle] {
        let fresh = matches!(&self.key, Some((slides, s)) if *slides == open.slides && *s == size);
        if !fresh {
            self.textures = open
                .slides
                .iter()
                .enumerate()
                .map(|(i, slide)| {
                    let image = self.renderer.render(slide, &self.style, size);
                    ctx.load_texture(format!("slide thumbnail {}", i + 1), color_image(&image), egui::TextureOptions::LINEAR)
                })
                .collect();
            self.key = Some((open.slides.clone(), size));
        }
        &self.textures
    }
}
