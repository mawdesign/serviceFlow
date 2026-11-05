//! The set list panel (GLOSSARY "Set"): the running order, with missing songs flagged, and the
//! placeholders shown where an item cannot be presented. The set is built and edited here too:
//! songs dragged in from the library (or added with Enter there), items dragged to reorder,
//! custom, image and style items added, the selected item's texts edited, undo and redo, and
//! saving (also under a new name, or as a duplicate). Every change is a command (ADR-0001).

use serviceflow_core::command::{Command, ConflictChoice};
use serviceflow_core::ids::SongId;
use serviceflow_core::set::{ItemKind, SetItem, StyleAction};
use serviceflow_core::set_edit::{ItemField, SetEdit, field_text};
use serviceflow_core::state::State;

use crate::tl;

/// What is dragged when a set item is moved.
#[derive(Debug, Clone, Copy)]
pub struct ItemDrag(pub usize);

/// UI-only state of the set panel.
#[derive(Debug, Default)]
pub struct SetView {
    /// The text of the item field being typed in, while it has the keyboard. The engine keeps the
    /// field parsed (a presentation order as tokens, slides split at `---`), so showing its text
    /// back while typing would eat spaces and separators.
    typing: Option<((usize, ItemField), String)>,
    /// The pictures ticked for a new image item, while choosing them.
    choosing_images: Option<Vec<String>>,
    /// The name typed for "Save as…", "Duplicate…" or "Rename…".
    naming: Option<(Naming, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Naming {
    SaveAs,
    Duplicate,
    Rename,
}

impl SetView {
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
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
        self.toolbar(ui, state, commands);
        ui.horizontal(|ui| {
            if ui.button(tl!("Previous item")).clicked() {
                commands.push(Command::PreviousItem);
            }
            if ui.button(tl!("Next item")).clicked() {
                commands.push(Command::NextItem);
            }
        });
        if set.changed_on_disk {
            ui.group(|ui| {
                ui.colored_label(ui.visuals().warn_fg_color, tl!("This set was changed on disk."));
                ui.horizontal_wrapped(|ui| {
                    if set.modified && ui.button(tl!("Overwrite")).clicked() {
                        commands.push(Command::ResolveSetConflict(ConflictChoice::Overwrite));
                    }
                    if ui.button(tl!("Reload (discard my changes)")).clicked() {
                        commands.push(Command::ResolveSetConflict(ConflictChoice::Reload));
                    }
                    if set.modified && ui.button(tl!("Save as a copy")).clicked() {
                        commands.push(Command::ResolveSetConflict(ConflictChoice::SaveAsCopy));
                    }
                });
            });
        }
        if let Some(item) = set.relinking.and_then(|i| set.set.items.get(i)) {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().warn_fg_color, format!("{} “{}”", tl!("Pick a replacement in the library for"), item.name));
                if ui.button(tl!("Cancel")).clicked() {
                    commands.push(Command::CancelRelink);
                }
            });
        }
        self.add_buttons(ui, state, commands);
        if let Some(index) = set.selected {
            self.item_editor(ui, state, index, commands);
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let len = set.set.items.len();
            for (index, item) in set.set.items.iter().enumerate() {
                let selected = set.selected == Some(index);
                ui.horizontal(|ui| {
                    let missing_image = state.missing_image(index);
                    let missing = set.is_missing(index) || missing_image.is_some();
                    let label = item_label(state, item);
                    let text = match (&item.kind, missing) {
                        (_, true) => egui::RichText::new(label).color(ui.visuals().error_fg_color),
                        (ItemKind::Placeholder { .. } | ItemKind::Style { .. }, false) => egui::RichText::new(label).weak(),
                        _ => egui::RichText::new(label),
                    };
                    // Click to select; drag onto another item to move it there; drop a song from
                    // the library onto an item to put it before that item.
                    let row = ui.add(egui::Button::selectable(selected, text).sense(egui::Sense::click_and_drag()));
                    row.dnd_set_drag_payload(ItemDrag(index));
                    if row.clicked() {
                        commands.push(Command::SelectItem(index));
                    }
                    drop_onto(ui, &row, index, len, commands);
                    if let Some(slide) = missing_image {
                        if ui.small_button(tl!("find…")).clicked() {
                            commands.push(Command::FindImage { item: index, slide });
                        }
                    } else if missing && ui.small_button(tl!("find…")).clicked() {
                        commands.push(Command::StartRelink(index));
                    }
                });
            }
            // Below the last item: dropping here puts it at the end.
            let (rect, end) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0_f32.max(ui.available_height())), egui::Sense::hover());
            if len == 0 {
                ui.painter().text(
                    rect.left_top(),
                    egui::Align2::LEFT_TOP,
                    tl!("Drag songs here from the library."),
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().weak_text_color(),
                );
            }
            drop_onto(ui, &end, len, len, commands);
        });
    }

    /// Undo, redo and saving.
    fn toolbar(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        let Some(set) = &state.set else { return };
        let busy = state.plan.is_some() || state.planning.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(set.history.can_undo(), egui::Button::new(tl!("Undo"))).clicked() {
                commands.push(Command::UndoSet);
            }
            if ui.add_enabled(set.history.can_redo(), egui::Button::new(tl!("Redo"))).clicked() {
                commands.push(Command::RedoSet);
            }
            let can_save = set.modified && !set.saving && !set.changed_on_disk;
            if ui.add_enabled(can_save, egui::Button::new(tl!("Save set"))).clicked() {
                commands.push(Command::SaveSet);
            }
            if ui.add_enabled(!busy && !set.saving, egui::Button::new(tl!("Save set as…"))).clicked() {
                self.naming = Some((Naming::SaveAs, set.id.to_string()));
            }
            if ui.add_enabled(!busy && !set.saving, egui::Button::new(tl!("Duplicate set…"))).clicked() {
                self.naming = Some((Naming::Duplicate, format!("{} (copy)", set.id)));
            }
            if ui.add_enabled(!busy && !set.saving, egui::Button::new(tl!("Rename set…"))).clicked() {
                self.naming = Some((Naming::Rename, set.id.to_string()));
            }
            if set.saving {
                ui.label(egui::RichText::new(tl!("Saving…")).weak());
            } else if set.modified {
                ui.label(egui::RichText::new(tl!("Edited, not saved")).weak());
            }
        });
        if let Some((naming, name)) = &mut self.naming {
            let done = ui
                .horizontal(|ui| {
                    let label = ui.label(match naming {
                        Naming::SaveAs => tl!("Save the set as:"),
                        Naming::Duplicate => tl!("Name of the copy:"),
                        Naming::Rename => tl!("Rename the set to:"),
                    });
                    ui.add(egui::TextEdit::singleline(name).desired_width(180.0)).labelled_by(label.id);
                    let ok = match naming {
                        Naming::SaveAs => tl!("Save"),
                        Naming::Duplicate => tl!("Duplicate"),
                        Naming::Rename => tl!("Rename"),
                    };
                    if ui.add_enabled(!name.trim().is_empty(), egui::Button::new(ok)).clicked() {
                        let name = name.trim().to_owned();
                        commands.push(match naming {
                            Naming::SaveAs => Command::SaveSetAs { name },
                            Naming::Duplicate => Command::DuplicateSet { name },
                            Naming::Rename => Command::RenameSet { name },
                        });
                        true
                    } else {
                        ui.button(tl!("Cancel")).clicked()
                    }
                })
                .inner;
            if done {
                self.naming = None;
            }
        }
    }

    /// Adding custom, image and style items, and acting on the selected item.
    fn add_buttons(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        let Some(set) = &state.set else { return };
        ui.horizontal_wrapped(|ui| {
            if ui.button(tl!("Add custom slides")).clicked() {
                commands.push(Command::EditSet(SetEdit::AddCustom { name: tl!("Custom slides").to_owned(), at: None }));
            }
            if ui.button(tl!("Add images…")).clicked() {
                self.choosing_images = Some(Vec::new());
                commands.push(Command::ListBackgrounds);
            }
            if ui.button(tl!("Add style change")).clicked() {
                commands.push(Command::EditSet(SetEdit::AddStyle { name: tl!("Style").to_owned(), action: StyleAction::New, at: None }));
            }
            if ui.button(tl!("Add style revert")).clicked() {
                let name = tl!("Previous style").to_owned();
                commands.push(Command::EditSet(SetEdit::AddStyle { name, action: StyleAction::Revert, at: None }));
            }
        });
        if let Some(index) = set.selected {
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(index > 0, egui::Button::new(tl!("Move up"))).clicked() {
                    commands.push(Command::EditSet(SetEdit::Move { from: index, to: index - 1 }));
                }
                if ui.add_enabled(index + 1 < set.set.items.len(), egui::Button::new(tl!("Move down"))).clicked() {
                    commands.push(Command::EditSet(SetEdit::Move { from: index, to: index + 1 }));
                }
                if ui.button(tl!("Duplicate item")).clicked() {
                    commands.push(Command::EditSet(SetEdit::Duplicate(index)));
                }
                if ui.button(tl!("Remove item")).clicked() {
                    commands.push(Command::EditSet(SetEdit::Remove(index)));
                }
            });
        }
        let Some(chosen) = &mut self.choosing_images else { return };
        let mut done = false;
        ui.group(|ui| {
            ui.label(tl!("Pictures from Backgrounds/ for the new image item:"));
            if state.backgrounds.is_empty() {
                ui.weak(tl!("There are no pictures in Backgrounds/."));
            }
            egui::ScrollArea::vertical().id_salt("new image item").max_height(160.0).show(ui, |ui| {
                for file in &state.backgrounds {
                    let mut ticked = chosen.contains(file);
                    if ui.checkbox(&mut ticked, file).changed() {
                        if ticked {
                            chosen.push(file.clone());
                        } else {
                            chosen.retain(|f| f != file);
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!chosen.is_empty(), egui::Button::new(tl!("Add image item"))).clicked() {
                    let files = std::mem::take(chosen);
                    commands.push(Command::EditSet(SetEdit::AddImages { name: tl!("Images").to_owned(), files, at: None }));
                    done = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    done = true;
                }
            });
        });
        if done {
            self.choosing_images = None;
        }
    }

    /// The selected item's texts: a song item's order override; a custom item's name, title,
    /// subtitle and slides; an image or style item's name. Notes are edited under the slides.
    fn item_editor(&mut self, ui: &mut egui::Ui, state: &State, index: usize, commands: &mut Vec<Command>) {
        let Some(item) = state.set.as_ref().and_then(|s| s.set.items.get(index)) else { return };
        let fields: &[(ItemField, &str, bool)] = match &item.kind {
            ItemKind::Song(_) => &[(ItemField::Presentation, tl!("Presentation order"), false)],
            ItemKind::Custom(_) => &[
                (ItemField::Name, tl!("Name"), false),
                (ItemField::Title, tl!("Title"), false),
                (ItemField::Subtitle, tl!("Subtitle"), false),
                (ItemField::Slides, tl!("Slides (separate them with a line of ---)"), true),
            ],
            ItemKind::Image(_) | ItemKind::Style { .. } => &[(ItemField::Name, tl!("Name"), false)],
            ItemKind::Placeholder { .. } => &[],
        };
        if fields.is_empty() {
            return;
        }
        ui.group(|ui| {
            for (field, label, multiline) in fields {
                self.field(ui, item, index, *field, label, *multiline, commands);
            }
            if matches!(item.kind, ItemKind::Custom(_) | ItemKind::Image(_)) {
                advance_editor(ui, item, index, commands);
            }
            if matches!(item.kind, ItemKind::Song(_)) {
                ui.weak(tl!("Section tags separated by spaces. Empty: the song's own order."));
            }
        });
    }

    /// One text field of an item. Each change is an edit; leaving the field ends the undo step.
    #[allow(clippy::too_many_arguments)]
    pub fn field(
        &mut self,
        ui: &mut egui::Ui,
        item: &SetItem,
        index: usize,
        field: ItemField,
        label: &str,
        multiline: bool,
        commands: &mut Vec<Command>,
    ) {
        let Some(current) = field_text(item, field) else { return };
        let key = (index, field);
        let mut text = match &self.typing {
            Some((k, typed)) if *k == key => typed.clone(),
            _ => current,
        };
        let label = ui.label(label);
        let edit = if multiline { egui::TextEdit::multiline(&mut text).desired_rows(4) } else { egui::TextEdit::singleline(&mut text) };
        let response = ui.add(edit.id_salt(("set item field", index, field)).desired_width(f32::INFINITY)).labelled_by(label.id);
        if response.changed() {
            commands.push(Command::EditSet(SetEdit::Field { item: index, field, text: text.clone() }));
        }
        if response.has_focus() {
            self.typing = Some((key, text));
        } else if self.typing.as_ref().is_some_and(|(k, _)| *k == key) {
            self.typing = None;
        }
        if response.lost_focus() {
            commands.push(Command::EndEditGroup);
        }
    }
}

/// How a custom or image item's slides advance by themselves: seconds per slide, loop, fade.
fn advance_editor(ui: &mut egui::Ui, item: &SetItem, index: usize, commands: &mut Vec<Command>) {
    let text = |field| field_text(item, field).unwrap_or_default();
    let edit = |field, text: String| Command::EditSet(SetEdit::Field { item: index, field, text });
    ui.horizontal(|ui| {
        let mut seconds: u32 = text(ItemField::Seconds).parse().unwrap_or(0);
        ui.label(tl!("Seconds per slide"));
        if ui.add(egui::DragValue::new(&mut seconds).range(0..=3600)).on_hover_text(tl!("0: advance by hand")).changed() {
            commands.push(edit(ItemField::Seconds, seconds.to_string()));
            commands.push(Command::EndEditGroup);
        }
        let mut looping = text(ItemField::Loop) == "true";
        if ui.checkbox(&mut looping, tl!("Loop")).changed() {
            commands.push(edit(ItemField::Loop, looping.to_string()));
        }
        let current = text(ItemField::Transition);
        let name = |value: &str| match value {
            "on" => tl!("Fade"),
            "off" => tl!("No fade"),
            _ => tl!("Fade as the output settings say"),
        };
        egui::ComboBox::from_label(tl!("Transition")).selected_text(name(&current)).show_ui(ui, |ui| {
            for value in ["default", "on", "off"] {
                if ui.selectable_label(current == value, name(value)).clicked() {
                    commands.push(edit(ItemField::Transition, value.to_owned()));
                }
            }
        });
    });
}

/// Something dropped on a row (or, with `index == len`, below the last one): a library song is
/// added there, a set item is moved there.
fn drop_onto(ui: &egui::Ui, response: &egui::Response, index: usize, len: usize, commands: &mut Vec<Command>) {
    let hovering = response.dnd_hover_payload::<SongId>().is_some() || response.dnd_hover_payload::<ItemDrag>().is_some();
    if hovering {
        // A line where it will go.
        let y = response.rect.top();
        ui.painter().hline(response.rect.x_range(), y, egui::Stroke::new(2.0, ui.visuals().selection.bg_fill));
    }
    if let Some(song) = response.dnd_release_payload::<SongId>() {
        commands.push(Command::EditSet(SetEdit::AddSong { song: (*song).clone(), at: Some(index) }));
    }
    if let Some(drag) = response.dnd_release_payload::<ItemDrag>() {
        let from = drag.0;
        // Dropped on row `index`: it goes before that row.
        let to = if from < index { index - 1 } else { index };
        if from != to && from < len {
            commands.push(Command::EditSet(SetEdit::Move { from, to: to.min(len - 1) }));
        }
    }
}

/// What the list shows for an item: a song's title (its filename until it has been read), a
/// placeholder's name and kind.
pub fn item_label(state: &State, item: &SetItem) -> String {
    match (&item.kind, item.song_id()) {
        (ItemKind::Song(_), Some(id)) => state.library.title(&id).to_owned(),
        (ItemKind::Style { action, .. }, _) => format!("{} ({}: {})", item.name, tl!("style"), style_action(*action)),
        (ItemKind::Custom(_), _) => format!("{} ({})", item.name, tl!("custom")),
        (ItemKind::Image(_), _) => format!("{} ({})", item.name, tl!("image")),
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

/// Above the slides of an image item: each picture whose file is missing, in red with "find…";
/// while the operator is looking, the files of `Backgrounds/` to choose from.
pub fn image_notice(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    let Some(set) = &state.set else { return };
    let Some(index) = set.selected else { return };
    let Some(ItemKind::Image(image)) = set.set.items.get(index).map(|item| &item.kind) else { return };
    for (slide, picture) in image.slides.iter().enumerate() {
        let Some(serviceflow_core::style::ImageSource::File(name)) = &picture.image else { continue };
        if !matches!(state.images.get(name), Some(serviceflow_core::resolve::ImageFile::Missing)) {
            continue;
        }
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, format!("{} “{name}”", tl!("The image is missing:")));
            if ui.button(tl!("find…")).clicked() {
                commands.push(Command::FindImage { item: index, slide });
            }
        });
    }
    if let Some((item, slide)) = set.finding_image.filter(|(item, _)| *item == index) {
        ui.group(|ui| {
            ui.label(tl!("Pick a replacement from Backgrounds/"));
            if state.backgrounds.is_empty() {
                ui.weak(tl!("There are no pictures in Backgrounds/."));
            }
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                for file in &state.backgrounds {
                    if ui.selectable_label(false, file).clicked() {
                        commands.push(Command::RelinkImage { item, slide, file: file.clone() });
                    }
                }
            });
            if ui.button(tl!("Cancel")).clicked() {
                commands.push(Command::CancelFindImage);
            }
        });
    }
}
