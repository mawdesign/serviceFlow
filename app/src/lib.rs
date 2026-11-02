//! The serviceFlow egui shell, shared by the desktop and browser builds.
//!
//! The shell is thin (ADR-0001): each frame it pumps the runtime, reads one immutable state
//! snapshot, draws it, and turns operator actions into commands.

pub mod i18n;
pub mod thumbnails;
#[cfg(target_arch = "wasm32")]
pub mod web;

use serviceflow_core::state::{Problem, State};
use serviceflow_render::Size;
use serviceflow_runtime::Driver;
use serviceflow_storage::Storage;

/// The control window: where the operator builds and runs the service.
pub struct ControlWindow<S> {
    driver: Driver<S>,
    thumbnails: thumbnails::Thumbnails,
}

/// Thumbnail width in points; thumbnails are 16:9.
const THUMBNAIL_WIDTH: f32 = 240.0;

impl<S: Storage + 'static> ControlWindow<S> {
    pub fn new(driver: Driver<S>) -> Self {
        Self { driver, thumbnails: thumbnails::Thumbnails::default() }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.driver.pump();
        if !self.driver.is_idle() {
            // Storage results arrive outside frames; keep pumping until they are in.
            ui.ctx().request_repaint();
        }
        let state = self.driver.snapshot();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(tl!("serviceFlow"));
            problems(ui, &state);
            slides(ui, &state, &mut self.thumbnails);
        });
    }
}

fn problems(ui: &mut egui::Ui, state: &State) {
    for problem in &state.problems {
        match problem {
            Problem::SongUnreadable { id, error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{} “{id}”: {error}", tl!("Could not open the song")));
            }
        }
    }
}

fn slides(ui: &mut egui::Ui, state: &State, thumbnails: &mut thumbnails::Thumbnails) {
    let Some(open) = &state.song else {
        ui.label(if state.loading.is_some() { tl!("Loading…") } else { tl!("No song is open.") });
        return;
    };
    ui.label(egui::RichText::new(&open.song.title).strong());
    let points = egui::vec2(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH * 9.0 / 16.0);
    let pixels = points * ui.ctx().pixels_per_point();
    let size = Size { width: pixels.x.round() as u32, height: pixels.y.round() as u32 };
    let textures = thumbnails.for_song(ui.ctx(), open, size);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (slide, texture) in open.slides.iter().zip(textures) {
                ui.vertical(|ui| {
                    ui.add(egui::Image::from_texture((texture.id(), points)).alt_text(slide.lines.join("\n")));
                    ui.label(egui::RichText::new(&slide.section).weak());
                });
            }
        });
    });
}

impl<S: Storage + 'static> eframe::App for ControlWindow<S> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ControlWindow::ui(self, ui);
    }
}

/// The control window's native viewport. craft-window decides between system and app-drawn
/// decorations (override with `SERVICEFLOW_TITLEBAR=app|system`).
pub fn control_viewport() -> egui::ViewportBuilder {
    let choice = craft_window::Choice::probe(craft_window::Prefix::new("SERVICEFLOW"));
    let builder =
        egui::ViewportBuilder::default().with_title(tl!("serviceFlow")).with_inner_size([1200.0, 800.0]).with_min_inner_size([640.0, 400.0]);
    craft_window::decorate(builder, choice.frame())
}
