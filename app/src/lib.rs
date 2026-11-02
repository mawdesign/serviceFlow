//! The serviceFlow egui shell, shared by the desktop and browser builds.

pub mod i18n;
#[cfg(target_arch = "wasm32")]
pub mod web;

/// The control window: where the operator builds and runs the service. Empty for now.
#[derive(Default)]
pub struct ControlWindow {}

impl ControlWindow {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(tl!("serviceFlow"));
            ui.label(tl!("No data root is open."));
        });
    }
}

impl eframe::App for ControlWindow {
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
