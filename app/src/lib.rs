//! The serviceFlow egui shell, shared by the desktop and browser builds.
//!
//! The shell is thin (ADR-0001): each frame it pumps the runtime, reads one immutable state
//! snapshot, draws it, and turns operator actions into commands.

#[cfg(not(target_arch = "wasm32"))]
pub mod data_root;
pub mod i18n;
pub mod keys;
pub mod monitors;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod output;
pub mod prefs;
pub mod thumbnails;
#[cfg(target_arch = "wasm32")]
pub mod web;

use serviceflow_core::command::Command;
use serviceflow_core::state::{OutputMode, Problem, State};
use serviceflow_render::Size;
use serviceflow_runtime::Driver;
use serviceflow_storage::Storage;

use monitors::{MonitorChoice, MonitorInfo, Monitors};
use output::OutputFrame;
use prefs::Prefs;

/// The control window: where the operator builds and runs the service.
pub struct ControlWindow<S> {
    driver: Driver<S>,
    thumbnails: thumbnails::Thumbnails,
    /// The small monitor in the control window.
    mirror: OutputFrame,
    /// The output window's frame.
    screen: OutputFrame,
    output_open: bool,
    monitors: Monitors,
    prefs: Prefs,
    save_prefs: Option<SavePrefs>,
}

/// Called with the preferences whenever the operator changes them.
type SavePrefs = Box<dyn FnMut(&Prefs)>;

/// Thumbnail width in points; thumbnails are 16:9.
const THUMBNAIL_WIDTH: f32 = 240.0;
/// Width in points of the output monitor in the control window.
const MIRROR_WIDTH: f32 = 320.0;

impl<S: Storage + 'static> ControlWindow<S> {
    pub fn new(driver: Driver<S>) -> Self {
        Self {
            driver,
            thumbnails: thumbnails::Thumbnails::default(),
            mirror: OutputFrame::new("output monitor"),
            screen: OutputFrame::new("output window"),
            output_open: false,
            monitors: Monitors::default(),
            prefs: Prefs::default(),
            save_prefs: None,
        }
    }

    /// Share the monitor list that the native event loop keeps up to date.
    pub fn with_monitors(mut self, monitors: Monitors) -> Self {
        self.monitors = monitors;
        self
    }

    /// Start from remembered preferences; `save` is called with them whenever they change.
    pub fn with_prefs(mut self, prefs: Prefs, save: impl FnMut(&Prefs) + 'static) -> Self {
        self.prefs = prefs;
        self.save_prefs = Some(Box::new(save));
        self
    }

    /// The current state snapshot.
    pub fn state(&self) -> std::sync::Arc<State> {
        self.driver.snapshot()
    }

    /// The runtime behind the window, to send commands to it.
    pub fn driver(&mut self) -> &mut Driver<S> {
        &mut self.driver
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.driver.pump();
        if !self.driver.is_idle() {
            // Storage results arrive outside frames; keep pumping until they are in.
            ui.ctx().request_repaint();
        }
        let mut commands = keys::live_commands(ui.ctx());
        let monitors = self.monitors.lock().map(|m| m.clone()).unwrap_or_default();

        // Send keyboard commands first so this frame already draws their result.
        let sent = !commands.is_empty();
        for command in commands.drain(..) {
            self.driver.send(command);
        }
        let state = self.driver.snapshot();

        egui::Panel::top("live controls").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(tl!("serviceFlow"));
                ui.separator();
                live_buttons(ui, &state, &mut commands);
                ui.separator();
                self.output_controls(ui, &monitors);
            });
        });
        egui::Panel::right("output monitor").show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Output monitor")).strong());
            let size = egui::vec2(MIRROR_WIDTH, MIRROR_WIDTH * 9.0 / 16.0);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            self.mirror.paint(ui, rect, &state.output);
            ui.label(match state.output.mode {
                OutputMode::Normal => "",
                OutputMode::Blank => tl!("Blank"),
                OutputMode::Black => tl!("Black"),
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            problems(ui, &state);
            slides(ui, &state, &mut self.thumbnails, &mut commands);
        });

        if self.output_open {
            let index = monitors::pick(self.prefs.output.monitor.as_ref(), &monitors);
            let target = index.and_then(|i| monitors.get(i).map(|m| (i, m)));
            self.output_open = output::show_window(ui.ctx(), target, &mut self.screen, &state.output, &mut commands);
        }

        let changed = sent || !commands.is_empty();
        for command in commands {
            self.driver.send(command);
        }
        if changed {
            ui.ctx().request_repaint();
        }
    }

    fn output_controls(&mut self, ui: &mut egui::Ui, monitors: &[MonitorInfo]) {
        let current = monitors::pick(self.prefs.output.monitor.as_ref(), monitors);
        if !monitors.is_empty() {
            let selected = current.map(|i| monitors[i].label(i)).unwrap_or_default();
            egui::ComboBox::from_label(tl!("Output monitor")).selected_text(selected).show_ui(ui, |ui| {
                for (i, monitor) in monitors.iter().enumerate() {
                    if ui.selectable_label(current == Some(i), monitor.label(i)).clicked() {
                        self.prefs.output.monitor = Some(MonitorChoice::of(monitor));
                        if let Some(save) = &mut self.save_prefs {
                            save(&self.prefs);
                        }
                        if self.output_open {
                            ui.ctx().send_viewport_cmd_to(output::viewport_id(), egui::ViewportCommand::SetMonitor(i));
                        }
                    }
                }
            });
        }
        let label = if self.output_open { tl!("Close output") } else { tl!("Show output") };
        if ui.button(label).clicked() {
            self.output_open = !self.output_open;
        }
    }
}

/// Blank and black as toggle buttons, with next and previous.
fn live_buttons(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    if ui.button(tl!("Previous")).clicked() {
        commands.push(Command::PreviousSlide);
    }
    if ui.button(tl!("Next")).clicked() {
        commands.push(Command::NextSlide);
    }
    if ui.selectable_label(state.output.mode == OutputMode::Blank, tl!("Blank")).clicked() {
        commands.push(Command::ToggleBlank);
    }
    if ui.selectable_label(state.output.mode == OutputMode::Black, tl!("Black")).clicked() {
        commands.push(Command::ToggleBlack);
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

fn slides(ui: &mut egui::Ui, state: &State, thumbnails: &mut thumbnails::Thumbnails, commands: &mut Vec<Command>) {
    let Some(open) = &state.song else {
        ui.label(if state.loading.is_some() { tl!("Loading…") } else { tl!("No song is open.") });
        return;
    };
    ui.label(egui::RichText::new(&open.song.title).strong());
    let points = egui::vec2(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH * 9.0 / 16.0);
    let pixels = points * ui.ctx().pixels_per_point();
    let size = Size { width: pixels.x.round() as u32, height: pixels.y.round() as u32 };
    let live = state.output.live.as_ref().filter(|live| live.song == open.id).map(|live| live.index);
    let textures = thumbnails.for_song(ui.ctx(), open, size);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, (slide, texture)) in open.slides.iter().zip(textures).enumerate() {
                ui.vertical(|ui| {
                    let image = egui::Image::from_texture((texture.id(), points)).alt_text(slide.lines.join("\n")).sense(egui::Sense::click());
                    let response = ui.add(image);
                    if live == Some(index) {
                        ui.painter().rect_stroke(
                            response.rect,
                            0.0,
                            egui::Stroke::new(3.0, ui.visuals().selection.bg_fill),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if response.clicked() {
                        commands.push(Command::GoLive { index });
                    }
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
