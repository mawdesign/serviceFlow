//! The serviceFlow egui shell, shared by the desktop and browser builds.
//!
//! The shell is thin (ADR-0001): each frame it pumps the runtime, reads one immutable state
//! snapshot, draws it, and turns operator actions into commands.

#[cfg(not(target_arch = "wasm32"))]
pub mod data_root;
pub mod editor;
pub mod files;
pub mod fonts;
pub mod i18n;
pub mod library;
pub mod monitors;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod output;
pub mod prefs;
pub mod set_panel;
pub mod shortcuts;
pub mod thumbnails;
#[cfg(target_arch = "wasm32")]
pub mod web;

use craft_ui::docking::{DockArea, DockContent, DockStyle, Layout, Node};
use craft_ui::layout::{SplitAxis, SplitSize};
use serviceflow_core::command::{Command, ImportTrigger};
use serviceflow_core::song::LanguageMode;
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
    shortcuts: shortcuts::Shortcuts,
    /// The Keyboard Shortcuts dialog, while it is open.
    shortcuts_dialog: Option<craft_keymap::ui::DialogState>,
    /// The slides panel shows the song editor instead of the slide grid. UI-only state.
    editing: bool,
    files: files::FileActions,
}

/// The control window's dockable panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Library,
    Set,
    Slides,
}

fn default_dock() -> Layout<Panel> {
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::Ratio(0.28),
            first: Box::new(Node::Split {
                axis: SplitAxis::Vertical,
                size: SplitSize::Ratio(0.5),
                first: Box::new(Node::Tabs { panels: vec![Panel::Set], active: 0 }),
                second: Box::new(Node::Tabs { panels: vec![Panel::Library], active: 0 }),
            }),
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
    editing: &'a mut bool,
    files: &'a mut files::FileActions,
    commands: &'a mut Vec<Command>,
}

impl DockContent<Panel> for Panels<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        match panel {
            Panel::Library => {
                self.files.new_song(ui, self.state, self.commands);
                self.library.ui(ui, self.state, self.commands);
            }
            Panel::Set => set_panel::ui(ui, self.state, self.commands),
            Panel::Slides if *self.editing => *self.editing = !editor::ui(ui, self.state, self.thumbnails, self.commands),
            Panel::Slides => slides(ui, self.state, self.thumbnails, self.editing, self.files, self.commands),
        }
    }

    fn logical_label(&self, panel: &Panel) -> String {
        match panel {
            Panel::Library => tl!("Library").to_string(),
            Panel::Set => tl!("Set").to_string(),
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
        driver.send(Command::RefreshSets);
        // First run: slides look like the congregation's OpenSong from the start.
        driver.send(Command::ImportOpenSongStyles { trigger: ImportTrigger::FirstRun });
        // And behave like it: logo, output monitor, initial mode, transitions, thumbnail colours.
        driver.send(Command::ImportOpenSongSettings { trigger: ImportTrigger::FirstRun });
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
            shortcuts: shortcuts::Shortcuts::new(Default::default()),
            shortcuts_dialog: None,
            editing: false,
            files: files::FileActions::default(),
        }
    }

    /// Share the monitor list that the native event loop keeps up to date.
    pub fn with_monitors(mut self, monitors: Monitors) -> Self {
        self.monitors = monitors;
        self
    }

    /// Start from remembered preferences; `save` is called with them whenever they change.
    pub fn with_prefs(mut self, prefs: Prefs, save: impl FnMut(&Prefs) + 'static) -> Self {
        self.shortcuts = shortcuts::Shortcuts::new(prefs.shortcuts.clone());
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
        // While the shortcuts dialog is open it records the presses, so they run nothing.
        let mut commands = if self.shortcuts_dialog.is_some() { Vec::new() } else { self.shortcuts.commands(ui.ctx()) };
        let monitors = self.monitors.lock().map(|m| m.clone()).unwrap_or_default();

        // Send keyboard commands first so this frame already draws their result.
        let sent = !commands.is_empty();
        for command in commands.drain(..) {
            self.driver.send(command);
        }
        let state = self.driver.snapshot();

        egui::Panel::top("live controls").show(ui, |ui| {
            // Wraps on narrow windows, so no control falls off the end.
            ui.horizontal_wrapped(|ui| {
                ui.heading(tl!("serviceFlow"));
                ui.separator();
                live_buttons(ui, &state, &mut commands);
                ui.separator();
                self.output_controls(ui, &monitors, state.output_settings.monitor);
                ui.separator();
                if ui.button(tl!("Keyboard shortcuts…")).clicked() {
                    self.shortcuts_dialog.get_or_insert_with(craft_keymap::ui::DialogState::new);
                }
                if ui.button(tl!("Re-import styles from OpenSong")).clicked() {
                    commands.push(Command::ImportOpenSongStyles { trigger: ImportTrigger::Manual });
                    commands.push(Command::ImportOpenSongSettings { trigger: ImportTrigger::Manual });
                }
            });
        });
        egui::Panel::right("output monitor").show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Output monitor")).strong());
            let size = egui::vec2(MIRROR_WIDTH, MIRROR_WIDTH * 9.0 / 16.0);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            self.mirror.paint(ui, rect, &state);
            ui.label(match state.output.mode {
                OutputMode::Normal => "",
                OutputMode::Blank => tl!("Blank"),
                OutputMode::Black => tl!("Black"),
                OutputMode::Logo => tl!("Logo"),
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            files::plan_panel(ui, &state, &mut commands);
            problems(ui, &state);
            let style = DockStyle::from_ui(ui);
            let mut panels = Panels {
                state: &state,
                thumbnails: &mut self.thumbnails,
                library: &mut self.library,
                editing: &mut self.editing,
                files: &mut self.files,
                commands: &mut commands,
            };
            let out = DockArea::new(ui.scope_id().with("dock")).show_content(ui, &self.dock, &style, &mut panels);
            if !out.actions.is_empty() {
                // A rejected action leaves the layout as it was.
                let _ = self.dock.apply_all(out.actions, |_| Default::default());
            }
        });

        if self.output_open {
            let index = monitors::pick(self.prefs.output.monitor.as_ref(), state.output_settings.monitor, &monitors);
            let target = index.and_then(|i| monitors.get(i).map(|m| (i, m)));
            self.output_open = output::show_window(ui.ctx(), target, &mut self.screen, &mut self.shortcuts, &state, &mut commands);
        }
        self.shortcuts_dialog(ui.ctx());

        // Rendering may have had to substitute a style's font; tell the engine so it is shown.
        commands.extend(fonts::substitution_commands());

        let changed = sent || !commands.is_empty();
        for command in commands {
            self.driver.send(command);
        }
        if changed {
            ui.ctx().request_repaint();
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        let Some(state) = &mut self.shortcuts_dialog else { return };
        let text = craft_keymap::ui::I18nText { i18n: &i18n::I18N, lang: i18n::lang() };
        let outcome = craft_keymap::ui::ShortcutsDialog::new(self.shortcuts.keymap_mut(), &text).show(ctx, state);
        let open = state.is_open();
        if matches!(outcome, Some(craft_keymap::ui::Outcome::Ok | craft_keymap::ui::Outcome::Changed)) {
            self.remember_shortcuts();
        }
        if !open {
            self.shortcuts_dialog = None;
        }
    }

    /// Store the operator's shortcut changes with the other preferences.
    fn remember_shortcuts(&mut self) {
        self.prefs.shortcuts = self.shortcuts.overrides().clone();
        if let Some(save) = &mut self.save_prefs {
            save(&self.prefs);
        }
    }

    /// Replace the keys of one shortcut and remember the change, as the dialog does. Returns false
    /// when a key does not parse or the command is unknown.
    pub fn rebind(&mut self, command: &craft_keymap::CommandRef, keys: &[&str]) -> bool {
        let done = self.shortcuts.rebind(command, keys);
        if done {
            self.remember_shortcuts();
        }
        done
    }

    fn output_controls(&mut self, ui: &mut egui::Ui, monitors: &[MonitorInfo], initial: Option<usize>) {
        let current = monitors::pick(self.prefs.output.monitor.as_ref(), initial, monitors);
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
    if ui.selectable_label(state.output.mode == OutputMode::Logo, tl!("Logo")).clicked() {
        commands.push(Command::ToggleLogo);
    }
    ui.separator();
    // The language mode of multilanguage sections (GLOSSARY "Language mode").
    let language = state.slide_options.language;
    let modes = [
        (LanguageMode::Both, tl!("Both languages")),
        (LanguageMode::Primary, tl!("Primary only")),
        (LanguageMode::Translation, tl!("Translation only")),
    ];
    let selected = modes.iter().find(|(m, _)| *m == language).map_or("", |(_, l)| l);
    egui::ComboBox::from_label(tl!("Language")).selected_text(selected).show_ui(ui, |ui| {
        for (mode, label) in modes {
            if ui.selectable_label(language == mode, label).clicked() && language != mode {
                commands.push(Command::SetLanguageMode(mode));
            }
        }
    });
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
            Problem::SetUnreadable { id, error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{} “{id}”: {error}", tl!("Could not open the set")));
            }
            Problem::SettingsUnreadable { file, error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{} {file}: {error}", tl!("Could not import the OpenSong settings file")));
            }
            Problem::SidecarUnreadable { id, error } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} “{id}”, {}: {error}", tl!("serviceFlow's own settings for the set"), tl!("so the defaults are used")),
                );
            }
            Problem::SidecarUnsaved { id, error } => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{} “{id}”: {error}", tl!("Could not save serviceFlow's own settings for the set")),
                );
            }
            Problem::SongUnsaved { id, error } => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{} “{id}”: {error}", tl!("Could not save the song")));
            }
            Problem::TagsNotRenamed { id } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("“{id}” {}", tl!("was saved with its section tags as written: renaming them would have merged two sections.")),
                );
            }
            Problem::PlanRefused(refusal) => {
                ui.colored_label(ui.visuals().error_fg_color, files::refusal_text(refusal));
            }
            Problem::PlanIncomplete { action, failure } => {
                ui.colored_label(ui.visuals().error_fg_color, files::failure_text(action, failure));
            }
            Problem::FontSubstituted { requested, used } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} “{requested}”, {} “{used}”. {}", tl!("Font not installed:"), tl!("showing"), tl!("Line breaks may differ.")),
                );
            }
        }
    }
}

fn slides(
    ui: &mut egui::Ui,
    state: &State,
    thumbnails: &mut thumbnails::Thumbnails,
    editing: &mut bool,
    files: &mut files::FileActions,
    commands: &mut Vec<Command>,
) {
    // Operator notes of the selected set item sit under the grid and are never projected.
    let notes = state.set.as_ref().and_then(|set| set.set.items.get(set.selected?)).map(|item| item.notes.as_str()).filter(|n| !n.trim().is_empty());
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        if let Some(notes) = notes {
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(tl!("Notes")).weak());
                ui.label(notes);
            });
        }
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| slide_grid(ui, state, thumbnails, editing, files, commands));
    });
}

fn slide_grid(
    ui: &mut egui::Ui,
    state: &State,
    thumbnails: &mut thumbnails::Thumbnails,
    editing: &mut bool,
    files: &mut files::FileActions,
    commands: &mut Vec<Command>,
) {
    let Some(open) = &state.song else {
        if state.set.as_ref().is_some_and(|set| set.selected.is_some()) && state.loading.is_none() {
            set_panel::placeholder(ui, state, commands);
        } else {
            ui.label(if state.loading.is_some() { tl!("Loading…") } else { tl!("No song is open.") });
        }
        return;
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&open.song.title).strong());
        // A custom or image item is shown read-only here (editing sets comes later).
        if !open.id.is_set_item() && ui.button(tl!("Edit song")).clicked() {
            *editing = true;
        }
    });
    ui.horizontal_wrapped(|ui| files.song_buttons(ui, state, commands));
    if open.id.is_set_item() {
        set_panel::image_notice(ui, state, commands);
    }
    let points = egui::vec2(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH * 9.0 / 16.0);
    let pixels = points * ui.ctx().pixels_per_point();
    let size = Size { width: pixels.x.round() as u32, height: pixels.y.round() as u32 };
    let live = state.output.live.as_ref().filter(|live| live.song == open.id).map(|live| live.index);
    let textures = thumbnails.for_song(ui.ctx(), state, size);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, (slide, texture)) in open.slides.iter().zip(textures).enumerate() {
                ui.vertical(|ui| {
                    let mut image = egui::Image::from_texture((texture.id(), points)).alt_text(slide.lines.join("\n")).sense(egui::Sense::click());
                    // Sections outside the presentation order: greyed, out of next/previous, but
                    // still clickable (ADR-0007).
                    let in_flow = open.is_in_flow(index);
                    if !in_flow {
                        image = image.tint(egui::Color32::from_gray(110));
                    }
                    let mut response = ui.add(image);
                    if !in_flow {
                        response = response.on_hover_text(tl!("Not in the presentation order"));
                    }
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
                    // The label sits on the colour OpenSong gives this kind of section.
                    let color = match open.item_kind {
                        Some(kind) => state.slide_colors.color(kind),
                        None => state.slide_colors.for_tag(&slide.section),
                    };
                    match color {
                        Some(c) => {
                            egui::Frame::new().fill(egui::Color32::from_rgb(c.r, c.g, c.b)).inner_margin(2.0).show(ui, |ui| {
                                ui.set_width(THUMBNAIL_WIDTH - 4.0);
                                ui.label(egui::RichText::new(&slide.section).color(egui::Color32::BLACK));
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new(&slide.section).weak());
                        }
                    }
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
