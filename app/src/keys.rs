//! Keyboard and clicker control of the live slide.
//!
//! A clicker remote sends arrow keys, Space or PageUp/PageDown, so those are the bindings. They
//! are fixed for now; the configurable keymap (craft-keymap) replaces this table later.
//!
//! Escape is deliberately not bound: it must never clear the output.

use egui::{Key, Modifiers};
use serviceflow_core::command::Command;

const BINDINGS: &[(Key, Command)] = &[
    (Key::ArrowRight, Command::NextSlide),
    (Key::ArrowDown, Command::NextSlide),
    (Key::Space, Command::NextSlide),
    (Key::PageDown, Command::NextSlide),
    (Key::ArrowLeft, Command::PreviousSlide),
    (Key::ArrowUp, Command::PreviousSlide),
    (Key::PageUp, Command::PreviousSlide),
    (Key::B, Command::ToggleBlack),
    (Key::W, Command::ToggleBlank),
    (Key::L, Command::ToggleLogo),
];

/// The live commands for the keys pressed this frame in the current viewport. The keys are
/// consumed, so widgets (a focused button reacts to Space) do not also act on them. While a text
/// field has the keyboard, nothing is taken.
pub fn live_commands(ctx: &egui::Context) -> Vec<Command> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input_mut(|input| BINDINGS.iter().filter(|(key, _)| input.consume_key(Modifiers::NONE, *key)).map(|(_, command)| command.clone()).collect())
}
