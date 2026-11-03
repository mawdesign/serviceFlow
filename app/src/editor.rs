//! The song editor (#18): a form for the song's details, a monospace lyrics editor in OpenSong's
//! line syntax with highlighting, the warnings the engine found, and a live preview drawn with
//! the real slide renderer and the current style.
//!
//! Every change is an [`EditSong`](Command::EditSong) command and undo/redo are commands too
//! (ADR-0001); nothing here keeps a copy of the song. Nothing is saved (ADR-0002, #19).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Key, Modifiers, TextStyle};
use serviceflow_core::command::Command;
use serviceflow_core::edit::{SongEdit, SongField};
use serviceflow_core::lyrics::{Highlight, highlight};
use serviceflow_core::song::SongWarning;
use serviceflow_core::state::State;
use serviceflow_render::Size;

use crate::thumbnails::Thumbnails;
use crate::tl;

/// Width in points of a preview slide; previews are 16:9.
const PREVIEW_WIDTH: f32 = 220.0;

/// Draw the editor for the open song. Returns true when the operator is done editing.
pub fn ui(ui: &mut egui::Ui, state: &State, thumbnails: &mut Thumbnails, commands: &mut Vec<Command>) -> bool {
    let Some(open) = &state.song else {
        ui.label(tl!("No song is open."));
        return true;
    };
    let draft = state.draft();
    // Undo and redo are the engine's, not the text box's own, so one history covers every field.
    // Take the keys before a text field sees them.
    let (undo, redo) = ui.input_mut(|input| {
        let redo = input.consume_key(Modifiers { shift: true, ..Modifiers::COMMAND }, Key::Z) | input.consume_key(Modifiers::COMMAND, Key::Y);
        (input.consume_key(Modifiers::COMMAND, Key::Z), redo)
    });
    if undo {
        commands.push(Command::Undo);
    }
    if redo {
        commands.push(Command::Redo);
    }

    let mut done = false;
    ui.horizontal(|ui| {
        done = ui.button(tl!("Done")).clicked();
        if ui.add_enabled(draft.is_some_and(|d| d.can_undo()), egui::Button::new(tl!("Undo"))).clicked() {
            commands.push(Command::Undo);
        }
        if ui.add_enabled(draft.is_some_and(|d| d.can_redo()), egui::Button::new(tl!("Redo"))).clicked() {
            commands.push(Command::Redo);
        }
        if draft.is_some_and(|d| d.is_modified()) {
            ui.label(egui::RichText::new(tl!("Edited, not saved")).weak());
        }
    });
    ui.separator();

    ui.columns(2, |columns| {
        egui::ScrollArea::vertical().id_salt("song editor").show(&mut columns[0], |ui| {
            details(ui, &open.song, commands);
            ui.add_space(8.0);
            lyrics(ui, &open.song.lyrics, commands);
            warnings(ui, &open.song.warnings);
        });
        columns[1].label(egui::RichText::new(tl!("Preview")).strong());
        preview(&mut columns[1], state, thumbnails);
    });
    done
}

/// The metadata form.
fn details(ui: &mut egui::Ui, song: &serviceflow_core::song::Song, commands: &mut Vec<Command>) {
    egui::Grid::new("song details").num_columns(2).show(ui, |ui| {
        for field in SongField::ALL {
            let label = ui.label(field_label(field));
            let mut text = field.get(song).to_owned();
            let response = ui.add(egui::TextEdit::singleline(&mut text).id_salt(("song field", field)).desired_width(f32::INFINITY));
            let response = response.labelled_by(label.id);
            send(&response, text, SongEdit::Field(field, String::new()), commands);
            ui.end_row();
        }
    });
}

fn field_label(field: SongField) -> &'static str {
    match field {
        SongField::Title => tl!("Title"),
        SongField::Author => tl!("Author"),
        SongField::Copyright => tl!("Copyright"),
        SongField::Ccli => tl!("CCLI number"),
        SongField::HymnNumber => tl!("Hymn number"),
        SongField::Key => tl!("Key"),
        SongField::Capo => tl!("Capo"),
        SongField::Aka => tl!("AKA"),
        SongField::Theme => tl!("Song theme"),
        SongField::User1 => tl!("User field 1"),
        SongField::User2 => tl!("User field 2"),
        SongField::User3 => tl!("User field 3"),
    }
}

/// Turn what happened to a text field into commands: a change is an edit (of the same kind as
/// `like`, carrying the new text), and leaving the field ends the undo step.
fn send(response: &egui::Response, text: String, like: SongEdit, commands: &mut Vec<Command>) {
    if response.changed() {
        commands.push(Command::EditSong(match like {
            SongEdit::Field(field, _) => SongEdit::Field(field, text),
            SongEdit::Lyrics(_) => SongEdit::Lyrics(text),
        }));
    }
    if response.lost_focus() {
        commands.push(Command::EndEditGroup);
    }
}

/// The lyrics, in a monospace box coloured by line syntax.
fn lyrics(ui: &mut egui::Ui, current: &str, commands: &mut Vec<Command>) {
    let heading = ui.label(egui::RichText::new(tl!("Lyrics")).strong());
    let font = TextStyle::Monospace.resolve(ui.style());
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = highlighted(buffer.as_str(), &font, ui.visuals());
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let mut text = current.to_owned();
    let response = ui.add(
        egui::TextEdit::multiline(&mut text)
            .id_salt("song lyrics")
            .font(TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .desired_rows(20)
            .lock_focus(true)
            .layouter(&mut layouter),
    );
    let response = response.labelled_by(heading.id);
    send(&response, text, SongEdit::Lyrics(String::new()), commands);
}

/// `text` as a layout job with each [`Highlight`] stretch in its colour.
pub fn highlighted(text: &str, font: &FontId, visuals: &egui::Visuals) -> LayoutJob {
    let dark = visuals.dark_mode;
    let pick = |dark_color: Color32, light_color: Color32| if dark { dark_color } else { light_color };
    let format = |color| TextFormat { font_id: font.clone(), color, ..Default::default() };
    let mut job = LayoutJob::default();
    let mut at = 0;
    for (range, kind) in highlight(text) {
        if range.start > at {
            job.append(&text[at..range.start], 0.0, format(visuals.text_color()));
        }
        let color = match kind {
            Highlight::Header => pick(Color32::from_rgb(110, 170, 255), Color32::from_rgb(0, 80, 190)),
            Highlight::Chords => pick(Color32::from_rgb(240, 170, 70), Color32::from_rgb(170, 90, 0)),
            Highlight::Comment | Highlight::PrintLayout => visuals.weak_text_color(),
            Highlight::VerseNumber => pick(Color32::from_rgb(130, 210, 130), Color32::from_rgb(0, 120, 30)),
            Highlight::StrayFirstCharacter => visuals.error_fg_color,
            Highlight::SlideBreak | Highlight::LineBreak | Highlight::Spacer => {
                pick(Color32::from_rgb(230, 120, 220), Color32::from_rgb(160, 0, 150))
            }
        };
        job.append(&text[range.clone()], 0.0, format(color));
        at = range.end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, format(visuals.text_color()));
    }
    job
}

/// The warnings the engine found in the lyrics, each with its line.
fn warnings(ui: &mut egui::Ui, warnings: &[SongWarning]) {
    if warnings.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Warnings")).strong());
    for warning in warnings {
        ui.colored_label(ui.visuals().warn_fg_color, warning_text(warning));
    }
}

/// The operator-facing wording of a warning.
pub fn warning_text(warning: &SongWarning) -> String {
    let line = warning.line();
    match warning {
        SongWarning::StrayFirstCharacter { character, .. } => {
            format!(
                "{} {line}: {} “{character}” {}",
                tl!("Line"),
                tl!("starts with"),
                tl!("so the whole line is read as lyrics. Start lyric lines with a space.")
            )
        }
        SongWarning::MaybeMultilanguage { tag, .. } => {
            format!("{} {line}: {} [{tag}] {}", tl!("Line"), tl!("the tag"), tl!("looks like it might be multilanguage. Add (L) after it if it is."))
        }
        SongWarning::BreakInsidePair { .. } => {
            format!(
                "{} {line}: {}",
                tl!("Line"),
                tl!("a || falls between a line and its translation, so the slide breaks after the translation instead.")
            )
        }
        SongWarning::TagCollision { tag, other, .. } => {
            format!(
                "{} {line}: {} [{tag}] {} [{other}] {}",
                tl!("Line"),
                tl!("the tag"),
                tl!("names the same section as"),
                tl!("once written canonically, so they will not be renamed.")
            )
        }
    }
}

/// The slides as they will be projected, drawn with the real renderer and the current style.
fn preview(ui: &mut egui::Ui, state: &State, thumbnails: &mut Thumbnails) {
    let Some(open) = &state.song else { return };
    let points = egui::vec2(PREVIEW_WIDTH, PREVIEW_WIDTH * 9.0 / 16.0);
    let pixels = points * ui.ctx().pixels_per_point();
    let size = Size { width: pixels.x.round() as u32, height: pixels.y.round() as u32 };
    let textures = thumbnails.for_song(ui.ctx(), open, &state.styles, size);
    egui::ScrollArea::vertical().id_salt("song preview").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, (slide, texture)) in open.slides.iter().zip(textures).enumerate() {
                ui.vertical(|ui| {
                    let image = egui::Image::from_texture((texture.id(), points)).alt_text(slide.lines.join("\n"));
                    let image = if open.is_in_flow(index) { image } else { image.tint(Color32::from_gray(110)) };
                    ui.add(image);
                    ui.label(egui::RichText::new(&slide.section).weak());
                });
            }
        });
    });
}
