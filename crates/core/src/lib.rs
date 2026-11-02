//! serviceFlow core: domain model, commands, engine, undo, transposition, search, multilanguage
//! pairing and style resolution. No IO and no UI (ADR-0001).
//!
//! Layering: must not depend on egui, eframe, winit, UI craft crates or any concrete storage
//! implementation (`cargo xtask layers`).

pub mod ids;
pub mod song;
