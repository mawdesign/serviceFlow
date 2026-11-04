//! serviceFlow core: domain model, commands, engine, undo, transposition, search, multilanguage
//! pairing and style resolution. No IO and no UI (ADR-0001).
//!
//! Layering: must not depend on egui, eframe, winit, UI craft crates or any concrete storage
//! implementation (`cargo xtask layers`).

pub mod canonical;
pub mod command;
pub mod edit;
pub mod engine;
pub mod ids;
pub mod lyrics;
pub mod output;
pub mod plan;
pub mod resolve;
pub mod search;
pub mod set;
pub mod sidecar;
pub mod song;
pub mod state;
pub mod style;
