//! The serviceFlow egui shell, shared by the desktop and browser builds.
//!
//! The shell is thin (ADR-0001): each frame it pumps the runtime, reads one immutable state
//! snapshot, draws it, and turns operator actions into commands.

#[cfg(not(target_arch = "wasm32"))]
pub mod data_root;
pub mod i18n;
pub mod keys;
pub mod library;
pub mod monitors;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod output;
pub mod prefs;
pub mod thumbnails;
#[cfg(target_arch = "wasm32")]
pub mod web;

use craft_ui::docking::{DockArea, DockContent, DockStyle, Layout, Node};
use craft_ui::layout::{SplitAxis, SplitSize};
use serviceflow_core::command::{Command, ImportTrigger};
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
    /// Where the dockable panels are. UI-only state.
    dock: Layout<Panel>,
    library: library::LibraryView,
}

/// The control window's dockable panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Library,
    Slides,
}

fn default_dock() -> Layout<Panel> {
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::Ratio(0.28),
            first: Box::new(Node::Tabs { panels: vec![Panel::Library], active: 0 }),
            second: Box::new(Node::Tabs { panels: vec![Panel::Slides], active: 0 }),
        }),
        floating: Vec::new(),
    }
}

/// The panels' bodies, borrowing what one frame needs.
struct Panels<'a> {
    state: &'a State,
    thumbnails: &'a mut thumbnails::Thumbnails,
    library: &'a mut library::LibraryView,
    commands: &'a mut Vec<Command>,
}

impl DockContent<Panel> for Panels<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        match panel {
            Panel::Library => self.library.ui(ui, self.state, self.commands),
            Panel::Slides => slides(ui, self.state, self.thumbnails, self.commands),
        }
    }

    fn logical_label(&self, panel: &Panel) -> String {
        match panel {
            Panel::Library => tl!("Library").to_string(),
            Panel::Slides => tl!("Slides").to_string(),
        }
    }
}

/// Called with the preferences whenever the operator changes them.
type SavePrefs = Box<dyn FnMut(&Prefs)>;

/// Thumbnail width in points; thumbnails are 16:9.
const THUMBNAIL_WIDTH: f32 = 240.0;
/// Width in points of the output monitor in the control window.
const MIRROR_WIDTH: f32 = 320.0;

impl<S: Storage + 'static> ControlWindow<S> {
    pub fn new(mut driver: Driver<S>) -> Self {
        // Start listing and indexing the library in the background; the window is usable meanwhile.
        driver.send(Command::RefreshLibrary);
        // First run: slides look like the congregation's OpenSong from the start.
        driver.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
        Self {
            driver,
            thumbnails: thumbnails::Thumbnails::default(),
            mirror: OutputFrame::new("output monitor"),
            screen: OutputFrame::new("output window"),
            output_open: false,
            monitors: Monitors::default(),
            prefs: Prefs::default(),
            save_prefs: None,
            dock: default_dock(),
            library: library::LibraryView::default(),
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
                ui.separator();
                if ui.button(tl!("Re-import styles from OpenSong")).clicked() {
                    commands.push(Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual });
                }
            });
        });
        egui::Panel::right("output monitor").show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Output monitor")).strong());
            let size = egui::vec2(MIRROR_WIDTH, MIRROR_WIDTH * 9.0 / 16.0);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            self.mirror.paint(ui, rect, &state.output, &state.styles);
            ui.label(match state.output.mode {
                OutputMode::Normal => "",
                OutputMode::Blank => tl!("Blank"),
                OutputMode::Black => tl!("Black"),
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            problems(ui, &state);
            let style = DockStyle::from_ui(ui);
            let mut panels = Panels { state: &state, thumbnails: &mut self.thumbnails, library: &mut self.library, commands: &mut commands };
            let out = DockArea::new(ui.scope_id().with("dock")).show_content(ui, &self.dock, &style, &mut panels);
            if !out.actions.is_empty() {
                // A rejected action leaves the layout as it was.
                let _ = self.dock.apply_all(out.actions, |_| Default::default());
            }
        });

        if self.output_open {
            let index = monitors::pick(self.prefs.output.monitor.as_ref(), &monitors);
            let target = index.and_then(|i| monitors.get(i).map(|m| (i, m)));
            self.output_open = output::show_window(ui.ctx(), target, &mut self.screen, &state.output, &state.styles, &mut commands);
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
            Problem::LibraryUnreadable { error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{}: {error}", tl!("Could not read the library")));
            }
            Problem::StylesUnreadable { error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{}: {error}", tl!("Could not import the OpenSong styles")));
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
    let textures = thumbnails.for_song(ui.ctx(), open, &state.styles, size);
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
