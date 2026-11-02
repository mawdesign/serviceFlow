//! Drawing what is live: the full-screen output window and the small monitor in the control
//! window. Both show the same [`Output`] state through an [`OutputFrame`].
//!
//! The window plumbing (`show_window`) is deliberately thin; what is shown is decided by the
//! engine's state and tested through it.

use serviceflow_core::command::Command;
use std::sync::Arc;

use serviceflow_core::song::{Slide, Song};
use serviceflow_core::state::{Output, OutputMode};
use serviceflow_core::style::{Captions, Styles};
use serviceflow_render::{Renderer, Size};

use crate::monitors::MonitorInfo;
use crate::thumbnails::color_image;

/// The viewport id of the output window.
pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("serviceflow output")
}

/// What is on screen, so a frame that has not changed renders nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shown {
    /// `None` is the background alone (blank, or nothing live yet).
    slide: Option<(Slide, Arc<Song>)>,
    size: Size,
}

/// Renders the output state into one texture, re-rendering only when it changes.
pub struct OutputFrame {
    name: &'static str,
    renderer: Renderer,
    styles: Option<Arc<Styles>>,
    texture: Option<(Shown, egui::TextureHandle)>,
}

impl OutputFrame {
    pub fn new(name: &'static str) -> Self {
        Self { name, renderer: Renderer::default(), styles: None, texture: None }
    }

    /// Paint `output` filling `rect` of `ui`.
    pub fn paint(&mut self, ui: &egui::Ui, rect: egui::Rect, output: &Output, styles: &Arc<Styles>) {
        let painter = ui.painter();
        let slide = match output.mode {
            OutputMode::Black => {
                painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
                return;
            }
            OutputMode::Blank => None,
            OutputMode::Normal => output.live.as_ref().map(|live| (live.slide.clone(), Arc::clone(&live.source))),
        };
        let pixels = rect.size() * ui.ctx().pixels_per_point();
        let size = Size { width: (pixels.x.round() as u32).max(1), height: (pixels.y.round() as u32).max(1) };
        let shown = Shown { slide, size };
        let same_styles = self.styles.as_ref().is_some_and(|s| Arc::ptr_eq(s, styles));
        if !(same_styles && matches!(&self.texture, Some((current, _)) if *current == shown)) {
            self.styles = Some(Arc::clone(styles));
            let image = match &shown.slide {
                Some((slide, song)) => {
                    let captions = Captions::for_song(song, &styles.default);
                    self.renderer.render_captioned(slide, &captions, &styles.default, size)
                }
                None => {
                    let empty = Slide { section: String::new(), lines: Vec::new() };
                    self.renderer.render(&empty, &styles.default, size)
                }
            };
            let image = color_image(&image);
            match &mut self.texture {
                Some((current, handle)) => {
                    handle.set(image, egui::TextureOptions::LINEAR);
                    *current = shown;
                }
                None => self.texture = Some((shown, ui.ctx().load_texture(self.name, image, egui::TextureOptions::LINEAR))),
            }
        }
        if let Some((_, handle)) = &self.texture {
            painter.image(handle.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }
}

/// Show the output window. With a `monitor` index it is borderless full screen there; without
/// one (browser, or no monitor list) it is an ordinary window. Returns true while it stays open.
///
/// The window cannot be exercised by the tests (it needs a display); it only paints the frame.
///
/// Keys pressed in the window (a clicker may be focused there) are added to `commands`.
pub fn show_window(
    ctx: &egui::Context,
    monitor: Option<(usize, &MonitorInfo)>,
    frame: &mut OutputFrame,
    output: &Output,
    styles: &Arc<Styles>,
    commands: &mut Vec<Command>,
) -> bool {
    let mut builder = egui::ViewportBuilder::default().with_title("serviceFlow output").with_inner_size([960.0, 540.0]);
    if let Some((index, _)) = monitor {
        builder = builder.with_monitor(index);
    }
    let mut open = true;
    ctx.show_viewport_immediate(viewport_id(), builder, |ui, _class| {
        if ui.input(|i| i.viewport().close_requested()) {
            open = false;
        }
        commands.extend(crate::keys::live_commands(ui.ctx()));
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| {
            let rect = ui.max_rect();
            frame.paint(ui, rect, output, styles);
        });
    });
    open
}
