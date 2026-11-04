//! Browser storage: File System Access API, plus OPFS with zip import and export.
//!
//! Both are the same [`WebStorage`] over a `FileSystemDirectoryHandle` laid out as an OpenSong
//! data root (`Songs/ Sets/ Settings/ Backgrounds/`, sidecars in `.serviceflow/`, as
//! `serviceflow-storage-fs`). Only where the handle comes from differs ([`Backend`]):
//!
//! - **Folder** (Chromium): `showDirectoryPicker()` gives a real folder on disk, read and written in
//!   place, so OpenSong and serviceFlow share it.
//! - **Browser** (Firefox, Safari, and Chromium if the operator prefers): the origin private file
//!   system (`navigator.storage.getDirectory()`). Files live inside the browser; the operator
//!   brings a data root in and out as a zip ([`archive`], [`WebStorage::import_archive`],
//!   [`WebStorage::export_archive`]).
//!
//! The browser types only exist on `wasm32`; [`archive`] is plain Rust and builds everywhere.

pub mod archive;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
pub use web::{Backend, DEFAULT_POLL_MS, ImportError, WebStorage, folder_access_supported};
