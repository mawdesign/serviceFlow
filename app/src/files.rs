//! File actions on songs (new, rename or move, delete) and the change plan the operator confirms
//! before anything is written (GLOSSARY "Change plan").

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::plan::{ChangePlan, PlanAction, PlanFailure, PlanFile, PlanRefusal, PlanStep, TidyChange, TidySkip, Unkept};
use serviceflow_core::search::category;
use serviceflow_core::state::State;

use crate::tl;

/// UI-only state: the names being typed.
#[derive(Debug, Default)]
pub struct FileActions {
    /// The new id typed for the open song, while renaming it.
    rename_to: Option<String>,
    /// The title typed for a new song.
    new_title: Option<String>,
}

impl FileActions {
    /// "Rename or move file…" and "Delete song" for the open song.
    pub fn song_buttons(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        let Some(open) = &state.song else { return };
        let busy = state.planning.is_some() || state.plan.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            if ui.button(tl!("Rename or move file…")).clicked() {
                self.rename_to = Some(open.id.as_str().to_owned());
            }
            if ui.button(tl!("Delete song…")).clicked() {
                commands.push(Command::DeleteSong(open.id.clone()));
            }
        });
        if let Some(name) = &mut self.rename_to {
            ui.end_row();
            let label = ui.label(tl!("New file name (folder/name):"));
            ui.add(egui::TextEdit::singleline(name).desired_width(240.0)).labelled_by(label.id);
            if ui.button(tl!("Rename")).clicked() {
                let to = SongId::new(name.trim());
                commands.push(Command::RenameSong { from: open.id.clone(), to });
                self.rename_to = None;
            } else if ui.button(tl!("Cancel")).clicked() {
                self.rename_to = None;
            }
        }
    }

    /// "Tidy library…": the dry-run list of every file not in canonical form (ADR-0002).
    pub fn tidy_button(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        let busy = state.planning.is_some() || state.plan.is_some();
        if ui.add_enabled(!busy, egui::Button::new(tl!("Tidy library…"))).clicked() {
            commands.push(Command::TidyLibrary);
        }
    }

    /// "New song…": a title, then the change plan names the file.
    pub fn new_song(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        match &mut self.new_title {
            None => {
                if ui.add_enabled(state.plan.is_none(), egui::Button::new(tl!("New song…"))).clicked() {
                    self.new_title = Some(String::new());
                }
            }
            Some(title) => {
                let done = ui
                    .horizontal(|ui| {
                        let label = ui.label(tl!("Title:"));
                        ui.add(egui::TextEdit::singleline(title).hint_text(tl!("Title of the new song")).desired_width(180.0)).labelled_by(label.id);
                        if ui.add_enabled(!title.trim().is_empty(), egui::Button::new(tl!("Create"))).clicked() {
                            // In the open song's category, if any.
                            let folder = state.song.as_ref().map(|s| category(&s.id).to_owned()).unwrap_or_default();
                            commands.push(Command::NewSong { category: folder, title: title.trim().to_owned() });
                            true
                        } else {
                            ui.button(tl!("Cancel")).clicked()
                        }
                    })
                    .inner;
                if done {
                    self.new_title = None;
                }
            }
        }
    }
}

fn file_name(file: &PlanFile) -> String {
    match file {
        PlanFile::Song(id) => format!("Songs/{id}"),
        PlanFile::Set(id) => format!("Sets/{id}"),
        PlanFile::Sidecar(id) => format!("{} Sets/{id}", tl!("serviceFlow's own settings for")),
    }
}

fn action_text(action: &PlanAction) -> String {
    match action {
        PlanAction::SaveSong(id) => format!("{} “{id}” {}", tl!("Saving"), tl!("renames its section tags.")),
        PlanAction::RenameSong { from, to } => format!("{} “{from}” → “{to}”", tl!("Rename song file")),
        PlanAction::DeleteSong(id) => format!("{} “{id}”", tl!("Delete song")),
        PlanAction::NewSong(id) => format!("{} “{id}”", tl!("New song")),
        PlanAction::SaveSetAs { from, to } => format!("{} “{from}” {} “{to}”", tl!("Save set"), tl!("as")),
        PlanAction::DuplicateSet { from, to } => format!("{} “{from}” {} “{to}”", tl!("Duplicate set"), tl!("as")),
        PlanAction::TidyLibrary => tl!("Tidy library: rewrite files not yet in canonical form").to_string(),
    }
}

fn step_text(step: &PlanStep) -> String {
    let file = file_name(&step.file());
    match step {
        PlanStep::WriteSet { expect: serviceflow_core::ids::Expect::Absent, .. } => format!("{file}: {}", tl!("create")),
        PlanStep::WriteSet { .. } => format!("{file}: {}", tl!("update the song item")),
        PlanStep::WriteSidecar { .. } => format!("{file}: {}", tl!("copy")),
        PlanStep::WriteSong { expect: serviceflow_core::ids::Expect::Absent, .. } => format!("{file}: {}", tl!("create")),
        PlanStep::WriteSong { .. } => format!("{file}: {}", tl!("save in canonical form")),
        PlanStep::RenameSong { to, .. } => format!("{file}: {} Songs/{to}", tl!("move to")),
        PlanStep::DeleteSong { .. } => format!("{file}: {}", tl!("delete")),
    }
}

/// The change plan waiting for the operator: every file, warnings, Apply / Cancel.
pub fn plan_panel(ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
    if state.planning.is_some() {
        ui.label(tl!("Checking which files change…"));
    }
    let Some(plan) = &state.plan else { return };
    ui.group(|ui| {
        plan_body(ui, plan);
        ui.horizontal(|ui| {
            if state.applying_plan {
                ui.label(tl!("Writing…"));
            } else {
                if !plan.steps.is_empty() && ui.button(tl!("Apply changes")).clicked() {
                    commands.push(Command::ConfirmPlan);
                }
                if ui.button(tl!("Cancel")).clicked() {
                    commands.push(Command::CancelPlan);
                }
            }
        });
    });
}

fn tidy_change_text(change: &TidyChange) -> String {
    match change {
        TidyChange::TagsRenamed(tags) => {
            let tags: Vec<String> = tags.iter().map(|(from, to)| format!("{from} → {to}")).collect();
            format!("{} ({})", tl!("section tags renamed"), tags.join(", "))
        }
        TidyChange::OrderRewritten => tl!("presentation order follows the new tags").to_string(),
        TidyChange::TrailingSpaces => tl!("spaces at the ends of lines removed").to_string(),
        TidyChange::ItemOrders(items) => format!("{}: {}", tl!("presentation order updated for"), items.join(", ")),
        TidyChange::Layout => tl!("file layout (encoding, line endings, element order)").to_string(),
    }
}

fn tidy_skip_text(skip: &TidySkip) -> String {
    match skip {
        TidySkip::TagCollision => tl!("left alone: renaming its section tags would merge two sections").to_string(),
        TidySkip::Unreadable(error) => format!("{}: {error}", tl!("left alone: it could not be read")),
        TidySkip::UnsavedEdits => tl!("left alone: it has unsaved edits").to_string(),
        TidySkip::Unkept(unkept) => {
            let parts: Vec<String> = unkept.iter().map(unkept_text).collect();
            format!("{}: {}", tl!("left alone: rewriting it would drop"), parts.join("; "))
        }
    }
}

fn unkept_text(unkept: &Unkept) -> String {
    match unkept {
        Unkept::Comments => tl!("XML comments").to_string(),
        Unkept::Attributes(element) => format!("{} <{element}>", tl!("attributes of")),
        Unkept::Repeated(element) => format!("{} <{element}>", tl!("a second")),
        Unkept::Elements(element) => format!("{} <{element}>", tl!("elements inside")),
        Unkept::Text(element) => format!("{} <{element}>", tl!("text directly inside")),
        Unkept::OldTimeSignature => tl!("<timesig> beside <time_sig>").to_string(),
    }
}

/// The dry-run list of a library tidy: what changes in each file, and what is left alone.
fn tidy_body(ui: &mut egui::Ui, plan: &ChangePlan) {
    let Some(report) = &plan.tidy else { return };
    if report.changes.is_empty() {
        ui.label(tl!("Nothing to tidy: every file is already in canonical form."));
    } else {
        ui.label(tl!("These files will be rewritten:"));
    }
    for (file, changes) in &report.changes {
        let changes: Vec<String> = changes.iter().map(tidy_change_text).collect();
        ui.label(format!("• {}: {}", file_name(file), changes.join("; ")));
    }
    for (file, skip) in &report.skipped {
        ui.colored_label(ui.visuals().warn_fg_color, format!("• {}: {}", file_name(file), tidy_skip_text(skip)));
    }
}

fn plan_body(ui: &mut egui::Ui, plan: &ChangePlan) {
    ui.label(egui::RichText::new(action_text(&plan.action)).strong());
    if plan.tidy.is_some() {
        tidy_body(ui, plan);
        return;
    }
    ui.label(tl!("These files will be written:"));
    for step in &plan.steps {
        ui.label(format!("• {}", step_text(step)));
    }
    if !plan.users.is_empty() {
        let sets: Vec<String> = plan.users.iter().map(|s| format!("“{s}”")).collect();
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("{} {}", tl!("These sets still use the song and will show it as missing:"), sets.join(", ")),
        );
    }
    if !plan.unreadable_sets.is_empty() {
        let sets: Vec<String> = plan.unreadable_sets.iter().map(|s| format!("“{s}”")).collect();
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("{} {}", tl!("These sets could not be read, so they are not updated:"), sets.join(", ")),
        );
    }
}

/// How a refused plan is worded.
pub fn refusal_text(refusal: &PlanRefusal) -> String {
    match refusal {
        PlanRefusal::SongMissing(id) => format!("“{id}” {}", tl!("is no longer in the library.")),
        PlanRefusal::NameTaken(id) => format!("“{id}” {}", tl!("already exists.")),
        PlanRefusal::BadName(id) => format!("“{id}” {}", tl!("cannot be used as a song file name.")),
        PlanRefusal::UnsavedSet(id) => format!("“{id}” {}", tl!("has unsaved edits. Save or discard them before tidying the library.")),
        PlanRefusal::Unreadable(error) => format!("{}: {error}", tl!("Could not read the library")),
    }
}

/// How a plan that stopped is worded: what was written and what was not.
pub fn failure_text(action: &PlanAction, failure: &PlanFailure) -> String {
    let what = action_text(action);
    let failed = file_name(&failure.failed);
    let cause = if failure.conflict { tl!("changed on disk since the changes were listed").to_string() } else { failure.error.clone() };
    if failure.written.is_empty() {
        format!("{what}: {} {failed} {cause}.", tl!("nothing was written, because"))
    } else {
        let written: Vec<String> = failure.written.iter().map(file_name).collect();
        format!(
            "{what}: {} {}. {} {failed} ({cause}). {}",
            tl!("written"),
            written.join(", "),
            tl!("Stopped at"),
            tl!("Do the same action again to finish.")
        )
    }
}
