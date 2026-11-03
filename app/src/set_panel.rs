//! The set list panel (GLOSSARY "Set"): the running order, with missing songs flagged, and the
//! placeholders shown where an item cannot be presented.

use serviceflow_core::command::Command;
use serviceflow_core::set::{ItemKind, SetItem, StyleAction};
use serviceflow_core::state::State;

use crate::tl;

pub fn ui(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    let selected_text = state.set.as_ref().map(|s| s.id.to_string()).unwrap_or_else(|| tl!("Open a set…").to_string());
    egui::ComboBox::from_label(tl!("Open set")).selected_text(selected_text).show_ui(ui, |ui| {
        for id in &state.sets {
            let open = state.set.as_ref().is_some_and(|s| &s.id == id);
            if ui.selectable_label(open, id.as_str()).clicked() {
                commands.push(Command::OpenSet(id.clone()));
            }
        }
        if state.sets.is_empty() {
            ui.weak(tl!("There are no sets."));
        }
    });
    let Some(set) = &state.set else {
        ui.label(if state.loading_set.is_some() { tl!("Loading…") } else { tl!("No set is open.") });
        return;
    };
    ui.horizontal(|ui| {
        if ui.button(tl!("Previous item")).clicked() {
            commands.push(Command::PreviousItem);
        }
        if ui.button(tl!("Next item")).clicked() {
            commands.push(Command::NextItem);
        }
    });
    if let Some(item) = set.relinking.and_then(|i| set.set.items.get(i)) {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, format!("{} “{}”", tl!("Pick a replacement in the library for"), item.name));
            if ui.button(tl!("Cancel")).clicked() {
                commands.push(Command::CancelRelink);
            }
        });
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (index, item) in set.set.items.iter().enumerate() {
            let selected = set.selected == Some(index);
            ui.horizontal(|ui| {
                let missing = set.is_missing(index);
                let label = item_label(state, item);
                let text = match (&item.kind, missing) {
                    (_, true) => egui::RichText::new(label).color(ui.visuals().error_fg_color),
                    (ItemKind::Placeholder { .. } | ItemKind::Style { .. }, false) => egui::RichText::new(label).weak(),
                    _ => egui::RichText::new(label),
                };
                if ui.selectable_label(selected, text).clicked() {
                    commands.push(Command::SelectItem(index));
                }
                if missing && ui.small_button(tl!("find…")).clicked() {
                    commands.push(Command::StartRelink(index));
                }
            });
        }
    });
}

/// What the list shows for an item: a song's title (its filename until it has been read), a
/// placeholder's name and kind.
pub fn item_label(state: &State, item: &SetItem) -> String {
    match (&item.kind, item.song_id()) {
        (ItemKind::Song(_), Some(id)) => state.library.title(&id).to_owned(),
        (ItemKind::Style { action, .. }, _) => format!("{} ({}: {})", item.name, tl!("style"), style_action(*action)),
        (ItemKind::Placeholder { item_type, .. }, _) if item_type.is_empty() => item.name.clone(),
        (ItemKind::Placeholder { item_type, .. }, _) => format!("{} ({item_type})", item.name),
        _ => item.name.clone(),
    }
}

/// The central view of a selected item that has no slides: a missing song (red, with "find…")
/// or a placeholder.
pub fn placeholder(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    let Some(set) = &state.set else { return };
    let Some(index) = set.selected else { return };
    let Some(item) = set.set.items.get(index) else { return };
    if set.is_missing(index) {
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, format!("{} “{}”", tl!("The song is missing:"), item.name));
            if ui.button(tl!("find…")).clicked() {
                commands.push(Command::StartRelink(index));
            }
        });
    } else if let ItemKind::Placeholder { item_type, .. } = &item.kind {
        ui.weak(format!("{} “{}” ({item_type}) {}", tl!("Item"), item.name, tl!("cannot be shown yet. It is kept as it is.")));
    }
}

fn style_action(action: StyleAction) -> String {
    match action {
        StyleAction::New => tl!("new").to_string(),
        StyleAction::Revert => tl!("revert").to_string(),
        StyleAction::Other => tl!("other").to_string(),
    }
}
