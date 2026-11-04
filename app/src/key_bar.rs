//! Transposition and capo of the selected song item (GLOSSARY "Transposition", "Capo"): the key
//! the song is in, steps up and down, the capo, and "Save in this key".

use serviceflow_core::command::Command;
use serviceflow_core::state::State;

use crate::tl;

/// The key bar, for a song that is an item of the open set; nothing otherwise.
pub fn key_bar(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    let Some(info) = state.key_info() else { return };
    let shift = state.shift();
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Key"));
        if ui.button("−").on_hover_text(tl!("Down a semitone")).clicked() {
            commands.push(Command::SetTranspose((info.semitones - 1).max(-11)));
        }
        let key = match (info.original.as_str(), info.sounding.as_deref()) {
            ("", _) | (_, None) => String::new(),
            (original, Some(sounding)) if sounding == original => original.to_owned(),
            (original, Some(sounding)) => format!("{original} → {sounding}"),
        };
        let steps = if info.semitones == 0 { String::new() } else { format!(" ({:+})", info.semitones) };
        ui.label(egui::RichText::new(format!("{key}{steps}")).strong());
        if ui.button("+").on_hover_text(tl!("Up a semitone")).clicked() {
            commands.push(Command::SetTranspose((info.semitones + 1).min(11)));
        }
        ui.separator();
        // The capo: 0 is none; "As in the song" goes back to the capo the song asks for.
        let capo_text = |fret: u8| if fret == 0 { tl!("none").to_string() } else { fret.to_string() };
        egui::ComboBox::from_label(tl!("Capo")).selected_text(capo_text(info.capo)).show_ui(ui, |ui| {
            if ui.selectable_label(shift.capo.is_none(), tl!("As in the song")).clicked() {
                commands.push(Command::SetCapo(None));
            }
            for fret in 0..=11u8 {
                if ui.selectable_label(shift.capo == Some(fret), capo_text(fret)).clicked() {
                    commands.push(Command::SetCapo(Some(fret)));
                }
            }
        });
        if info.capo > 0
            && let Some(shapes) = &info.shapes
        {
            // The shapes are what is on the slides; the sound is what the audience hears.
            let sounding = info.sounding.as_deref().unwrap_or_default();
            ui.label(format!("{} {shapes}, {} {sounding}", tl!("shapes in"), tl!("sounds in")));
        }
        ui.separator();
        if ui.add_enabled(info.semitones != 0, egui::Button::new(tl!("Save in this key"))).clicked() {
            commands.push(Command::SaveInThisKey);
        }
        if ui.add_enabled(info.semitones != 0 || shift.capo.is_some(), egui::Button::new(tl!("Reset"))).clicked() {
            commands.push(Command::SetTranspose(0));
            commands.push(Command::SetCapo(None));
        }
    });
}
