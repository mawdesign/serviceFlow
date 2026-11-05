//! The browser build: runs the control window on the page's canvas, over browser storage
//! (`serviceflow-storage-web`).
//!
//! - **Chromium** can open a real OpenSong folder (File System Access API): the first screen asks
//!   for it. The operator may use browser storage instead.
//! - **Firefox and Safari** have no folder access, so the library lives in the browser's private
//!   file system (OPFS). A banner says so, with zip import and export of an OpenSong folder.
//! - There is one screen: "Show output" fills the page and asks the browser for full screen.
//!   Leaving full screen (Escape) closes the output.
//! - The CJK fonts are fetched from `fonts/` the first time slide text needs them.
//!
//! Browser calls are asynchronous; their results come back through [`Inbox`] and are handled at
//! the start of the next frame. None of this can run in the native tests: see TODO.md "To test".

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serviceflow_core::command::Command;
use serviceflow_core::state::ArchiveImport;
use serviceflow_runtime::Driver;
use serviceflow_storage::StorageError;
use serviceflow_storage_web::{Backend, WebStorage, folder_access_supported};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::{ControlWindow, tl};

impl ControlWindow<WebStorage> {
    /// A second storage on the same folder as this window's own.
    fn second_storage(&self) -> WebStorage {
        let storage = self.driver.storage();
        WebStorage::from_directory(storage.directory().clone(), storage.backend())
    }
}

/// Id of the `<canvas>` in `app/index.html`.
const CANVAS_ID: &str = "serviceflow_canvas";
/// The folder in the browser's private file system that holds the library.
const BROWSER_FOLDER: &str = "serviceflow";

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("no document");
        let canvas =
            document.get_element_by_id(CANVAS_ID).expect("no canvas").dyn_into::<web_sys::HtmlCanvasElement>().expect("not a canvas element");
        let result =
            eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(WebApp::new(&cc.egui_ctx))))).await;
        if let Some(loading) = document.get_element_by_id("serviceflow_loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_text_content(Some(&format!("{} {e:?}", tl!("serviceFlow could not start:")))),
            }
        }
    });
}

/// A result of a browser call, delivered to the next frame.
enum Event {
    /// A storage was opened (`None`: the operator cancelled the folder picker).
    Opened(Result<Option<(WebStorage, bool)>, StorageError>),
    /// A zip file was chosen for import: its name and bytes.
    ZipChosen(String, Vec<u8>),
    Exported(Result<(), StorageError>),
    Font(&'static str, Result<Vec<u8>, String>),
}

/// Where browser calls leave their results, and how they wake the UI.
#[derive(Clone)]
struct Inbox {
    events: Rc<RefCell<Vec<Event>>>,
    ctx: egui::Context,
}

impl Inbox {
    fn post(&self, event: Event) {
        self.events.borrow_mut().push(event);
        self.ctx.request_repaint();
    }

    fn spawn(&self, task: impl std::future::Future<Output = Event> + 'static) {
        let inbox = self.clone();
        spawn_local(async move { inbox.post(task.await) });
    }
}

/// Things worth telling the operator about storage, shown at the top.
#[derive(Debug, Clone, PartialEq)]
enum Notice {
    NotADataRoot(String),
    Failed(String),
    ExportFailed(String),
}

/// The browser app: picking storage, then the control window over it.
pub struct WebApp {
    inbox: Inbox,
    window: Option<ControlWindow<WebStorage>>,
    /// The open storage's backend and folder name.
    open: Option<(Backend, String)>,
    busy: bool,
    notice: Option<Notice>,
    /// A zip waiting for the operator to confirm that it replaces the browser library.
    to_import: Option<(String, Vec<u8>)>,
    /// Whether the page was in full screen last frame, while the output was shown.
    full_screen: bool,
    /// Full screen was asked for since the output was last opened.
    asked_full_screen: bool,
}

impl WebApp {
    pub fn new(ctx: &egui::Context) -> Self {
        let inbox = Inbox { events: Rc::default(), ctx: ctx.clone() };
        let mut app =
            Self { inbox, window: None, open: None, busy: false, notice: None, to_import: None, full_screen: false, asked_full_screen: false };
        // Without folder access there is nothing to choose: open the browser library at once.
        if !folder_access_supported() {
            app.open_browser_storage();
        }
        app
    }

    fn open_browser_storage(&mut self) {
        self.busy = true;
        self.inbox.spawn(async {
            let opened = WebStorage::open_private(BROWSER_FOLDER).await;
            // Ask the browser to keep the files when space runs low.
            WebStorage::request_persistence().await;
            Event::Opened(opened.map(|s| Some((s, true))))
        });
    }

    /// The folder picker must open soon after the click, which it does: tasks start at once.
    fn pick_folder(&mut self) {
        self.busy = true;
        self.inbox.spawn(async {
            let picked = match WebStorage::pick_folder().await {
                Ok(Some(storage)) => match storage.looks_like_data_root().await {
                    Ok(root) => Ok(Some((storage, root))),
                    Err(e) => Err(e),
                },
                other => other.map(|_| None),
            };
            Event::Opened(picked)
        });
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Opened(result) => {
                self.busy = false;
                match result {
                    Ok(Some((storage, true))) => self.show(storage),
                    Ok(Some((storage, false))) => self.notice = Some(Notice::NotADataRoot(storage.name())),
                    Ok(None) => {}
                    Err(e) => self.notice = Some(Notice::Failed(e.to_string())),
                }
            }
            Event::ZipChosen(name, bytes) => self.to_import = Some((name, bytes)),
            Event::Exported(result) => {
                self.busy = false;
                if let Err(e) = result {
                    self.notice = Some(Notice::ExportFailed(e.to_string()));
                }
            }
            Event::Font(file, Ok(bytes)) => {
                if !crate::fonts::shared().provide(file, Arc::from(bytes)) {
                    crate::fonts::shared().font_failed(file);
                }
            }
            Event::Font(file, Err(e)) => {
                log::warn!("serviceFlow: cannot load the font {file}: {e}");
                crate::fonts::shared().font_failed(file);
            }
        }
    }

    fn show(&mut self, storage: WebStorage) {
        self.open = Some((storage.backend(), storage.name()));
        self.window = Some(ControlWindow::new(Driver::new(storage)).with_single_output());
    }

    /// A second storage on the open folder, for an export (it only reads) that runs beside the
    /// control window.
    fn same_folder(&mut self) -> Option<WebStorage> {
        Some(self.window.as_ref()?.second_storage())
    }

    fn export(&mut self) {
        let Some(storage) = self.same_folder() else { return };
        self.busy = true;
        self.inbox.spawn(async move {
            let result = match storage.export_archive().await {
                Ok(zip) => download("OpenSong.zip", &zip).map_err(StorageError::Other),
                Err(e) => Err(e),
            };
            Event::Exported(result)
        });
    }

    /// The engine replaces the library through its own storage (ADR-0001): the result shows in
    /// its state (and a failure among its problems).
    fn start_import(&mut self, bytes: Vec<u8>) {
        if let Some(window) = &mut self.window {
            window.driver().send(Command::ImportArchive(Arc::from(bytes)));
        }
    }

    /// Fetch every lazy font that slide text has asked for.
    fn fetch_fonts(&self) {
        for face in crate::fonts::shared().take_wanted() {
            let file = face.file;
            self.inbox.spawn(async move { Event::Font(file, fetch(&format!("fonts/{file}")).await) });
        }
    }

    /// The browser shows the output full screen; leaving full screen closes it.
    fn follow_full_screen(&mut self) {
        let Some(window) = &mut self.window else { return };
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let is_full = document.fullscreen_element().is_some();
        if window.output_open() {
            if !self.asked_full_screen {
                // Once, shortly after the "Show output" click, so the browser allows it. It may
                // refuse (an iframe, iPad Safari): the output then fills the page.
                self.asked_full_screen = true;
                if let Some(root) = document.document_element() {
                    let _ = root.request_fullscreen();
                }
            }
            if self.full_screen && !is_full {
                window.close_output();
            }
            self.full_screen = is_full;
        } else {
            if is_full && self.full_screen {
                document.exit_fullscreen();
            }
            self.full_screen = false;
            self.asked_full_screen = false;
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let import = self.window.as_ref().and_then(|w| w.state().archive_import);
        let busy = self.busy || import == Some(ArchiveImport::Importing);
        egui::Panel::top("storage").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                match &self.open {
                    Some((Backend::Folder, name)) => {
                        ui.label(format!("{} {name}", tl!("Folder:")));
                        if ui.add_enabled(!busy, egui::Button::new(tl!("Open another data root…"))).clicked() {
                            self.pick_folder();
                        }
                    }
                    Some((Backend::Browser, _)) => {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            tl!("Your songs and sets are stored in this browser, not in a folder on your computer. Export them as a zip to keep a copy or to use them in OpenSong."),
                        );
                        if ui.add_enabled(!busy, egui::Button::new(tl!("Import data root (zip)…"))).clicked() {
                            choose_zip(self.inbox.clone());
                        }
                    }
                    None => {}
                }
                if self.open.is_some() && ui.add_enabled(!busy, egui::Button::new(tl!("Export as zip"))).clicked() {
                    self.export();
                }
                if busy {
                    ui.spinner();
                }
            });
            if let Some(ArchiveImport::Imported(n)) = import {
                ui.label(format!("{n} {}", tl!("files imported.")));
            }
            if let Some((name, bytes)) = self.to_import.take() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{} “{name}”? {}", tl!("Replace everything stored in this browser with"), tl!("This cannot be undone.")));
                    if ui.button(tl!("Replace")).clicked() {
                        self.start_import(bytes);
                    } else if !ui.button(tl!("Cancel")).clicked() {
                        self.to_import = Some((name, bytes));
                    }
                });
            }
            if let Some(notice) = &self.notice {
                let error = ui.visuals().error_fg_color;
                match notice {
                    Notice::NotADataRoot(name) => {
                        ui.colored_label(error, format!("“{name}” {}", tl!("has no Songs folder, so it is not an OpenSong data root.")))
                    }
                    Notice::Failed(e) => ui.colored_label(error, format!("{} {e}", tl!("The library could not be opened:"))),
                    Notice::ExportFailed(e) => ui.colored_label(error, format!("{} {e}", tl!("The library could not be exported:"))),
                };
            }
        });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(tl!("serviceFlow"));
            if self.busy {
                ui.spinner();
                return;
            }
            ui.label(tl!("Choose the data root that holds your OpenSong Songs and Sets."));
            if ui.button(tl!("Open data root…")).clicked() {
                self.pick_folder();
            }
            ui.add_space(12.0);
            ui.label(tl!("Or keep your songs in this browser and bring them in from a zip file."));
            if ui.button(tl!("Use browser storage")).clicked() {
                self.open_browser_storage();
            }
        });
    }
}

impl eframe::App for WebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let events: Vec<Event> = self.inbox.events.borrow_mut().drain(..).collect();
        for event in events {
            self.handle(event);
        }
        self.follow_full_screen();
        let output_shown = self.window.as_ref().is_some_and(ControlWindow::output_open);
        if !output_shown {
            self.top_bar(ui);
        }
        match &mut self.window {
            Some(window) => window.ui(ui),
            None => self.welcome(ui),
        }
        // Slide text drawn this frame may need a font that is not here yet.
        self.fetch_fonts();
    }
}

/// The bytes at `url`, relative to the page.
async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response = JsFuture::from(window.fetch_with_str(url)).await.map_err(|e| format!("{e:?}"))?;
    let response: web_sys::Response = response.unchecked_into();
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Let the operator choose a zip file; its bytes arrive as [`Event::ZipChosen`].
fn choose_zip(inbox: Inbox) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(input) = document.create_element("input").map(|e| e.unchecked_into::<web_sys::HtmlInputElement>()) else { return };
    input.set_type("file");
    input.set_accept(".zip,application/zip");
    let chosen = input.clone();
    let on_change = Closure::<dyn FnMut()>::new(move || {
        let Some(file) = chosen.files().and_then(|files| files.get(0)) else { return };
        let inbox = inbox.clone();
        spawn_local(async move {
            if let Ok(buffer) = JsFuture::from(file.array_buffer()).await {
                inbox.post(Event::ZipChosen(file.name(), js_sys::Uint8Array::new(&buffer).to_vec()));
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    // The input lives as long as the page may still call it back.
    on_change.forget();
    input.click();
}

/// Offer `bytes` as a download called `name`.
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let error = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/zip");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(error)?;
    let link: web_sys::HtmlAnchorElement = document.create_element("a").map_err(error)?.unchecked_into();
    link.set_href(&url);
    link.set_download(name);
    link.click();
    web_sys::Url::revoke_object_url(&url).map_err(error)?;
    Ok(())
}
