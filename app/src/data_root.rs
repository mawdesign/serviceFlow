//! The desktop app: choosing, remembering and switching the OpenSong data root.
//!
//! On first run there is no remembered root, so a `craft-dialogs` folder picker opens. The choice
//! is saved with `craft-settings` and used on the next launch. "Change data root" picks another.
//! Once a root is open, the control window runs over [`FsStorage`] on it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use craft_dialogs::{Answer, Dialogs, FileDialog};
use craft_settings::{DocumentMut, Location, MigrateError, SettingsStore};
use serde::{Deserialize, Serialize};
use serviceflow_runtime::Driver;
use serviceflow_storage_fs::FsStorage;

use crate::{ControlWindow, tl};

/// serviceFlow's remembered preferences (`config.toml`).
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Prefs {
    pub library: LibraryPrefs,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct LibraryPrefs {
    /// The OpenSong data root last chosen by the operator.
    pub data_root: Option<PathBuf>,
}

impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "serviceflow";
    const SCHEMA_VERSION: u32 = 1;

    fn migrate(_from: u32, _doc: &mut DocumentMut) -> Result<(), MigrateError> {
        Ok(())
    }
}

/// What a folder dialog was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    PickDataRoot,
}

/// Why a folder could not be used as the data root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootProblem {
    /// The folder has no `Songs/` folder inside, so it is not an OpenSong data root.
    NotADataRoot(PathBuf),
    /// The remembered folder is gone (an unplugged drive, a renamed folder).
    Missing(PathBuf),
    /// The folder dialog could not be shown.
    NoDialog(String),
}

/// The desktop app: settings, the folder picker and, once a root is chosen, the control window.
pub struct DesktopApp {
    store: SettingsStore<Prefs>,
    dialogs: Dialogs<Then>,
    window: Option<ControlWindow<FsStorage>>,
    data_root: Option<PathBuf>,
    problem: Option<RootProblem>,
}

impl DesktopApp {
    /// Start up. Opens the remembered data root (or `override_root`, which is used for this run
    /// only and not remembered). With neither, the folder picker opens on the first frame.
    pub fn new(ctx: &egui::Context, location: Location, override_root: Option<PathBuf>) -> Self {
        let (store, _report) = SettingsStore::<Prefs>::open(location);
        let ctx = ctx.clone();
        let dialogs = Dialogs::new(Arc::new(move || ctx.request_repaint()));
        let mut app = Self { store, dialogs, window: None, data_root: None, problem: None };
        let remembered = app.store.get().library.data_root.clone();
        match override_root.or(remembered) {
            Some(root) => app.open_root(root, false),
            None => app.choose_data_root(),
        }
        app
    }

    /// The open data root, if any.
    pub fn data_root(&self) -> Option<&Path> {
        self.data_root.as_deref()
    }

    /// Why the last attempt to open a data root failed, if it did.
    pub fn problem(&self) -> Option<&RootProblem> {
        self.problem.as_ref()
    }

    /// The control window over the open data root.
    pub fn window(&mut self) -> Option<&mut ControlWindow<FsStorage>> {
        self.window.as_mut()
    }

    /// The folder picker, for tests to answer without a native dialog (`set_override`).
    pub fn dialogs(&mut self) -> &mut Dialogs<Then> {
        &mut self.dialogs
    }

    /// Ask the operator for a data root (first run, and "change data root").
    pub fn choose_data_root(&mut self) {
        let mut dialog = FileDialog::folder().title(tl!("Choose your OpenSong data folder"));
        if let Some(current) = &self.data_root {
            dialog = dialog.directory(current.clone());
        }
        self.dialogs.ask(dialog, Then::PickDataRoot);
    }

    /// Open `root` as the data root and remember it. The previous root stays open if `root`
    /// cannot be used.
    pub fn use_data_root(&mut self, root: PathBuf) {
        self.open_root(root, true);
    }

    fn open_root(&mut self, root: PathBuf, remember: bool) {
        if !root.exists() {
            self.problem = Some(RootProblem::Missing(root));
            self.ask_if_nothing_is_open();
            return;
        }
        if !FsStorage::looks_like_data_root(&root) {
            self.problem = Some(RootProblem::NotADataRoot(root));
            self.ask_if_nothing_is_open();
            return;
        }
        match FsStorage::open(&root) {
            Ok(storage) => {
                self.window = Some(ControlWindow::new(Driver::new(storage)));
                self.problem = None;
                if remember {
                    self.store.update(|p| p.library.data_root = Some(root.clone()));
                    // The choice must survive a crash during the service: do not wait for the debounce.
                    let _ = self.store.flush();
                }
                self.data_root = Some(root);
            }
            Err(_) => {
                self.problem = Some(RootProblem::Missing(root));
                self.ask_if_nothing_is_open();
            }
        }
    }

    fn ask_if_nothing_is_open(&mut self) {
        if self.window.is_none() && self.dialogs.queued() == 0 && !self.dialogs.is_open() {
            self.choose_data_root();
        }
    }

    /// Draw one frame. `parent` is the window the folder picker belongs to.
    pub fn ui(&mut self, ui: &mut egui::Ui, parent: Option<&eframe::Frame>) {
        for (then, answer) in self.dialogs.take_answers() {
            match (then, answer) {
                (Then::PickDataRoot, Answer::Paths(paths)) => {
                    if let Some(path) = paths.into_iter().next() {
                        self.use_data_root(path);
                    }
                }
                (Then::PickDataRoot, Answer::Unavailable(why)) => self.problem = Some(RootProblem::NoDialog(why)),
                (_, Answer::Cancelled | Answer::Message(_)) => {}
            }
        }
        let tick = self.store.tick(Instant::now());
        if let Some(deadline) = tick.next_deadline {
            ui.ctx().request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        }
        egui::Panel::top("data_root").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &self.data_root {
                    Some(root) => ui.label(format!("{} {}", tl!("Data root:"), root.display())),
                    None => ui.label(tl!("No data root is chosen.")),
                };
                if ui.button(tl!("Change data root…")).clicked() {
                    self.choose_data_root();
                }
            });
            if let Some(problem) = &self.problem {
                ui.colored_label(ui.visuals().error_fg_color, problem_text(problem));
            }
        });
        match &mut self.window {
            Some(window) => window.ui(ui),
            None => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(tl!("serviceFlow"));
                    ui.label(tl!("Choose the folder that holds your OpenSong Songs and Sets."));
                });
            }
        }
        self.dialogs.show_pending(parent);
    }
}

fn problem_text(problem: &RootProblem) -> String {
    match problem {
        RootProblem::NotADataRoot(path) => format!("{} {}", tl!("This folder has no Songs folder, so it is not an OpenSong data root:"), path.display()),
        RootProblem::Missing(path) => format!("{} {}", tl!("This data root cannot be found:"), path.display()),
        RootProblem::NoDialog(why) => format!("{} {why}", tl!("The folder picker could not be shown:")),
    }
}

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        DesktopApp::ui(self, ui, Some(frame));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.store.flush();
    }
}
