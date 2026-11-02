//! The library panel: categories and song titles, with a search box.

use serviceflow_core::command::Command;
use serviceflow_core::ids::SongId;
use serviceflow_core::search::category;
use serviceflow_core::state::State;

use crate::tl;

/// UI-only state of the panel: the text being typed (the engine holds the query it was last sent).
#[derive(Debug, Default)]
pub struct LibraryView {
    query: String,
}

impl LibraryView {
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &State, commands: &mut Vec<Command>) {
        let library = &state.library;
        let hint = tl!("Search songs (#50 for a hymn number)");
        let edit = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(hint).desired_width(f32::INFINITY));
        if edit.changed() {
            commands.push(Command::Search(self.query.clone()));
        }
        if library.indexing {
            ui.weak(format!("{} {}/{}", tl!("Indexing…"), library.indexed, library.songs.len()));
        }
        let open = state.song.as_ref().map(|s| &s.id);
        if !self.query.trim().is_empty() {
            if library.results.is_empty() {
                ui.label(tl!("No songs match."));
                return;
            }
            let row = ui.spacing().interact_size.y;
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row, library.results.len(), |ui, range| {
                for id in &library.results[range] {
                    song_row(ui, state, id, open, commands);
                }
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // Songs are sorted by id, so a category's songs are contiguous.
            let mut songs = library.songs.iter().peekable();
            while let Some(first) = songs.peek() {
                let name = category(first).to_string();
                let mut group = Vec::new();
                while let Some(id) = songs.next_if(|id| category(id) == name) {
                    group.push(id);
                }
                if name.is_empty() {
                    for id in group {
                        song_row(ui, state, id, open, commands);
                    }
                } else {
                    egui::CollapsingHeader::new(&name).id_salt(("category", &name)).show(ui, |ui| {
                        for id in group {
                            song_row(ui, state, id, open, commands);
                        }
                    });
                }
            }
            if library.songs.is_empty() && !library.indexing {
                ui.label(tl!("The library is empty."));
            }
        });
    }
}

fn song_row(ui: &mut egui::Ui, state: &State, id: &SongId, open: Option<&SongId>, commands: &mut Vec<Command>) {
    let title = state.library.title(id);
    if ui.selectable_label(open == Some(id), title).clicked() {
        commands.push(Command::OpenSong(id.clone()));
    }
}
