//! The style editor (#30): a window over the control window for the default style or the style
//! a set item pushes. Title, subtitle and body boxes (font, size, colour, alignment, margins) and
//! the background, with a live preview drawn by the real slide renderer.
//!
//! Every change is an [`EditStyle`](Command::EditStyle) command (ADR-0001); nothing here keeps a
//! copy of the style. A set item's style is a set edit (saved with the set, undone with the set's
//! history); the default style is serviceFlow's own and is never written to OpenSong's settings.

use serviceflow_core::command::Command;
use serviceflow_core::song::Slide;
use serviceflow_core::state::State;
use serviceflow_core::style::{Align, Captions, ImagePosition, ImageSource, OPENSONG_BORDER, OPENSONG_SCALE, Rgba, Style, TextStyle, VAlign};
use serviceflow_core::style_edit::{Side, StyleChange, StyleEdit, StylePart, StyleTarget};
use serviceflow_render::{Renderer, Size};

use crate::thumbnails::color_image;
use crate::tl;

/// Width in points of the preview; it is 16:9.
const PREVIEW_WIDTH: f32 = 440.0;
/// The smallest margin the model holds: OpenSong's margin 0 plus its border.
const MIN_MARGIN: f32 = OPENSONG_BORDER * OPENSONG_SCALE;

/// What the preview was drawn from: the style, the slide, the size, the fonts' generation and the
/// background images read.
type Key = (Style, Slide, Captions, Size, u64, usize);

/// The style editor's UI state: which style is being edited, and the preview.
pub struct StyleEditor {
    target: Option<StyleTarget>,
    part: StylePart,
    preview: Preview,
    /// A colour was changed and the undo step stays open until the pointer is released.
    colour_open: bool,
}

impl Default for StyleEditor {
    fn default() -> Self {
        Self { target: None, part: StylePart::Body, preview: Preview::default(), colour_open: false }
    }
}

impl StyleEditor {
    /// Show the editor for a style.
    pub fn open(&mut self, target: StyleTarget, commands: &mut Vec<Command>) {
        self.target = Some(target);
        // The background picker lists the pictures of `Backgrounds/`.
        commands.push(Command::ListBackgrounds);
    }

    pub fn target(&self) -> Option<StyleTarget> {
        self.target
    }

    /// The style being edited, and what to call it. `None`: there is no such style any more.
    fn style(target: StyleTarget, state: &State) -> Option<(&Style, String)> {
        match target {
            StyleTarget::Default => Some((&state.styles.default, tl!("Default style").to_string())),
            StyleTarget::Item(index) => {
                let item = state.set.as_ref()?.set.items.get(index)?;
                Some((item.style.as_deref()?, tl!("Style item “{name}”", name = item.name)))
            }
        }
    }

    /// Draw the window, if it is open.
    pub fn ui(&mut self, ctx: &egui::Context, state: &State, commands: &mut Vec<Command>) {
        let Some(target) = self.target else { return };
        let Some((style, name)) = Self::style(target, state) else {
            // The item was removed or lost its style (undo of its creation).
            self.target = None;
            return;
        };
        let mut open = true;
        egui::Window::new(tl!("Style editor")).open(&mut open).default_width(900.0).default_height(620.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&name).strong());
                let (can_undo, can_redo, undo, redo) = match target {
                    StyleTarget::Default => {
                        let h = &state.default_style.history;
                        (h.can_undo(), h.can_redo(), Command::UndoStyle, Command::RedoStyle)
                    }
                    StyleTarget::Item(_) => {
                        let h = state.set.as_ref().map(|s| (s.history.can_undo(), s.history.can_redo()));
                        let (u, r) = h.unwrap_or_default();
                        (u, r, Command::UndoSet, Command::RedoSet)
                    }
                };
                if ui.add_enabled(can_undo, egui::Button::new(tl!("Undo style"))).clicked() {
                    commands.push(undo);
                }
                if ui.add_enabled(can_redo, egui::Button::new(tl!("Redo style"))).clicked() {
                    commands.push(redo);
                }
                match target {
                    StyleTarget::Default => {
                        let note = if state.default_style.is_edited() {
                            tl!("Changed from OpenSong's. Kept in serviceFlow's own settings; OpenSong's settings are never written.")
                        } else {
                            tl!("As in OpenSong. Changes are kept in serviceFlow's own settings; OpenSong's settings are never written.")
                        };
                        ui.label(egui::RichText::new(note).weak());
                    }
                    StyleTarget::Item(_) => {
                        let modified = state.set.as_ref().is_some_and(|s| s.modified);
                        if ui.add_enabled(modified, egui::Button::new(tl!("Save set"))).clicked() {
                            commands.push(Command::SaveSet);
                        }
                        if modified {
                            ui.label(egui::RichText::new(tl!("Edited, not saved")).weak());
                        }
                    }
                }
            });
            ui.separator();
            ui.columns(2, |columns| {
                let mut controls = Controls { target, part: self.part, commands, colour_open: &mut self.colour_open };
                egui::ScrollArea::vertical().id_salt("style controls").show(&mut columns[0], |ui| {
                    ui.horizontal(|ui| {
                        for (part, label) in [
                            (StylePart::Title, tl!("Title")),
                            (StylePart::Subtitle, tl!("Subtitle")),
                            (StylePart::Body, tl!("Body")),
                            (StylePart::Background, tl!("Background")),
                        ] {
                            if ui.selectable_label(self.part == part, label).clicked() {
                                self.part = part;
                            }
                        }
                    });
                    ui.separator();
                    controls.part = self.part;
                    match self.part {
                        StylePart::Title => controls.text(ui, &style.title.text),
                        StylePart::Subtitle => controls.text(ui, &style.subtitle.text),
                        StylePart::Body => {
                            controls.text(ui, &style.body.text);
                            ui.separator();
                            controls.check(ui, tl!("Shrink text to fit the slide"), style.body.auto_scale, StyleChange::AutoScale);
                            controls.check(ui, tl!("Flip italic on chorus slides"), style.body.highlight_chorus, StyleChange::HighlightChorus);
                        }
                        StylePart::Background => controls.background(ui, style, state),
                    }
                });
                self.preview.show(&mut columns[1], style, state);
            });
        });
        if !open {
            self.target = None;
        }
        // The undo step of a colour ends when the pointer is released.
        if self.colour_open && !ctx.input(|i| i.pointer.any_down()) {
            self.colour_open = false;
            commands.push(Command::EndEditGroup);
        }
    }
}

/// The preview: the style drawn on a sample slide.
struct Preview {
    renderer: Renderer,
    shown: Option<(Key, egui::TextureHandle)>,
}

impl Default for Preview {
    fn default() -> Self {
        Self { renderer: Renderer::with_fonts(crate::fonts::shared()), shown: None }
    }
}

impl Preview {
    /// The style drawn on a sample slide, with the real renderer: the open song's first slide, or
    /// a built-in one.
    fn show(&mut self, ui: &mut egui::Ui, style: &Style, state: &State) {
        let (slide, captions) = match state.song.as_ref().and_then(|open| open.slides.first().map(|s| (open, s))) {
            Some((open, slide)) => (slide.clone(), Captions::for_song(&open.song, style)),
            None => (
                Slide {
                    section: "V1".to_owned(),
                    lines: vec!["Amazing grace, how sweet the sound".to_owned(), "That saved a wretch like me".to_owned()],
                    ..Slide::default()
                },
                Captions { title: "Amazing Grace".to_owned(), subtitle: "John Newton".to_owned() },
            ),
        };
        let points = egui::vec2(PREVIEW_WIDTH, PREVIEW_WIDTH * 9.0 / 16.0);
        let pixels = points * ui.ctx().pixels_per_point();
        let size = Size { width: pixels.x.round() as u32, height: pixels.y.round() as u32 };
        let fonts = self.renderer.fonts().generation();
        let images = std::sync::Arc::as_ptr(&state.images) as usize;
        let key: Key = (style.clone(), slide.clone(), captions.clone(), size, fonts, images);
        if !matches!(&self.shown, Some((current, _)) if *current == key) {
            let image = self.renderer.render_with_images(&slide, &captions, style, &state.images, size);
            let texture = ui.ctx().load_texture("style preview", color_image(&image), egui::TextureOptions::LINEAR);
            self.shown = Some((key, texture));
        }
        if let Some((_, texture)) = &self.shown {
            ui.add(egui::Image::from_texture((texture.id(), points)).alt_text(tl!("Preview of the style")));
        }
    }
}

/// The controls for one part of the style, sending an edit for every change.
struct Controls<'a> {
    target: StyleTarget,
    part: StylePart,
    commands: &'a mut Vec<Command>,
    colour_open: &'a mut bool,
}

impl Controls<'_> {
    fn edit(&mut self, change: StyleChange) {
        self.commands.push(Command::EditStyle { target: self.target, edit: StyleEdit::new(self.part, change) });
    }

    fn end(&mut self) {
        self.commands.push(Command::EndEditGroup);
    }

    fn check(&mut self, ui: &mut egui::Ui, label: &str, value: bool, change: impl FnOnce(bool) -> StyleChange) {
        let mut v = value;
        if ui.checkbox(&mut v, label).changed() {
            self.edit(change(v));
            self.end();
        }
    }

    fn number(&mut self, ui: &mut egui::Ui, label: &str, value: f32, range: std::ops::RangeInclusive<f32>, change: impl FnOnce(f32) -> StyleChange) {
        ui.horizontal(|ui| {
            let name = ui.label(label);
            let mut v = value;
            let response = ui.add(egui::DragValue::new(&mut v).range(range).speed(1.0).max_decimals(2));
            let response = response.labelled_by(name.id);
            if response.changed() {
                self.edit(change(v));
            }
            if response.drag_stopped() || response.lost_focus() {
                self.end();
            }
        });
    }

    fn colour(&mut self, ui: &mut egui::Ui, label: &str, value: Rgba, change: impl FnOnce(Rgba) -> StyleChange) {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut rgba = [value.r, value.g, value.b, value.a];
            if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                self.edit(change(Rgba { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }));
                *self.colour_open = true;
            }
        });
    }

    fn choice<T: PartialEq + Copy>(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        value: T,
        options: &[(T, &str)],
        change: impl FnOnce(T) -> StyleChange,
    ) {
        let selected = options.iter().find(|(v, _)| *v == value).map_or("", |(_, l)| l);
        let mut chosen = None;
        egui::ComboBox::from_label(label).selected_text(selected).show_ui(ui, |ui| {
            for (v, l) in options {
                if ui.selectable_label(*v == value, *l).clicked() && *v != value {
                    chosen = Some(*v);
                }
            }
        });
        if let Some(v) = chosen {
            self.edit(change(v));
            self.end();
        }
    }

    /// Everything a text box has.
    fn text(&mut self, ui: &mut egui::Ui, t: &TextStyle) {
        self.check(ui, tl!("Show"), t.enabled, StyleChange::Enabled);
        ui.horizontal(|ui| {
            let name = ui.label(tl!("Font"));
            let mut font = t.font.clone();
            let response = ui.add(egui::TextEdit::singleline(&mut font).id_salt("style font").desired_width(200.0)).labelled_by(name.id);
            if response.changed() {
                self.edit(StyleChange::Font(font));
            }
            if response.lost_focus() {
                self.end();
            }
        });
        // The renderer says what it would draw instead, as the font is typed (#9).
        if let Some(sub) = crate::fonts::shared().substitution(&t.font) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tl!("Font not installed: “{requested}”, showing “{used}”. Line breaks may differ.", requested = sub.requested, used = sub.used),
            );
        }
        self.number(ui, tl!("Size"), t.size, 1.0..=600.0, StyleChange::Size);
        ui.horizontal(|ui| {
            self.check(ui, tl!("Bold"), t.bold, StyleChange::Bold);
            self.check(ui, tl!("Italic"), t.italic, StyleChange::Italic);
            self.check(ui, tl!("Underline"), t.underline, StyleChange::Underline);
        });
        self.colour(ui, tl!("Colour"), t.color, StyleChange::Color);
        self.check(ui, tl!("Outline"), t.outline, StyleChange::Outline);
        if t.outline {
            self.colour(ui, tl!("Outline colour"), t.outline_color, StyleChange::OutlineColor);
        }
        self.check(ui, tl!("Shadow"), t.shadow, StyleChange::Shadow);
        if t.shadow {
            self.colour(ui, tl!("Shadow colour"), t.shadow_color, StyleChange::ShadowColor);
        }
        self.check(ui, tl!("Fill behind text"), t.fill, StyleChange::Fill);
        if t.fill {
            self.colour(ui, tl!("Fill colour"), t.fill_color, StyleChange::FillColor);
        }
        self.choice(
            ui,
            tl!("Align"),
            t.align,
            &[(Align::Left, tl!("Left")), (Align::Center, tl!("Centre")), (Align::Right, tl!("Right"))],
            StyleChange::Align,
        );
        self.choice(
            ui,
            tl!("Vertical align"),
            t.valign,
            &[(VAlign::Top, tl!("Top")), (VAlign::Middle, tl!("Middle")), (VAlign::Bottom, tl!("Bottom"))],
            StyleChange::VAlign,
        );
        ui.label(egui::RichText::new(tl!("Margins")).strong());
        let m = t.margins;
        for (side, label, value) in [
            (Side::Left, tl!("Left margin"), m.left),
            (Side::Top, tl!("Top margin"), m.top),
            (Side::Right, tl!("Right margin"), m.right),
            (Side::Bottom, tl!("Bottom margin"), m.bottom),
        ] {
            self.number(ui, label, value, MIN_MARGIN..=600.0, move |v| StyleChange::Margin(side, v));
        }
    }

    fn background(&mut self, ui: &mut egui::Ui, style: &Style, state: &State) {
        let b = &style.background;
        self.colour(ui, tl!("Background colour"), b.color, StyleChange::Color);
        let current = match &b.image {
            Some(ImageSource::File(f)) => f.clone(),
            Some(ImageSource::Embedded(_)) => tl!("(picture stored in the style)").to_string(),
            None => tl!("None").to_string(),
        };
        let mut chosen = None;
        egui::ComboBox::from_label(tl!("Background picture")).selected_text(current).show_ui(ui, |ui| {
            if ui.selectable_label(b.image.is_none(), tl!("None")).clicked() {
                chosen = Some(None);
            }
            for file in &state.backgrounds {
                let selected = b.image == Some(ImageSource::File(file.clone()));
                if ui.selectable_label(selected, file).clicked() && !selected {
                    chosen = Some(Some(ImageSource::File(file.clone())));
                }
            }
        });
        if let Some(image) = chosen {
            self.edit(StyleChange::Image(image));
            self.end();
        }
        self.choice(
            ui,
            tl!("Picture fills"),
            b.position,
            &[
                (ImagePosition::Cover, tl!("The screen, cropped")),
                (ImagePosition::Stretch, tl!("The screen, stretched")),
                (ImagePosition::Center, tl!("Centred, natural size")),
                (ImagePosition::Tile, tl!("Tiled")),
            ],
            StyleChange::Position,
        );
    }
}
