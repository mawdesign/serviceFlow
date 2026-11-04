//! Drawing what is live: the full-screen output window and the small monitor in the control
//! window. Both show the same [`Output`] state through an [`OutputFrame`].
//!
//! The window plumbing (`show_window`) is deliberately thin; what is shown is decided by the
//! engine's state and tested through it.

use serviceflow_core::command::Command;
use std::sync::Arc;

use serviceflow_core::output::Logo;
use serviceflow_core::resolve::{Images, ResolvedStyle};
use serviceflow_core::song::{Slide, Song};
use serviceflow_core::state::{OutputId, OutputMode, State};
use serviceflow_core::style::Captions;
use serviceflow_render::{Image, Renderer, Size};

use crate::monitors::MonitorInfo;
use crate::thumbnails::color_image;

/// The viewport id of the output window.
pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("serviceflow output")
}

/// The logo, compared by identity (it is a large text).
#[derive(Debug, Clone)]
struct SameLogo(Logo);

impl PartialEq for SameLogo {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0.image, &other.0.image)
    }
}

/// What is on screen, so a frame that has not changed renders nothing.
#[derive(Debug, Clone, PartialEq)]
enum Content {
    Black,
    /// The logo; `None` when none is configured (the background alone).
    Logo(Option<SameLogo>),
    /// `None` is the background alone (blank, or nothing live yet). The look is the style and
    /// song background the slide is drawn with.
    Slide(Option<(Slide, Arc<Song>)>, ResolvedStyle),
}

#[derive(Debug, Clone, PartialEq)]
struct Shown {
    content: Content,
    size: Size,
}

/// A crossfade in progress: `start` and `duration` are in seconds of egui time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    pub start: f64,
    pub duration: f64,
}

impl Fade {
    /// How far the new picture has faded in at `now`: 0 at the start, 1 once done.
    pub fn progress(&self, now: f64) -> f32 {
        ((now - self.start) / self.duration).clamp(0.0, 1.0) as f32
    }
}

/// Renders the output state into a texture, re-rendering only when it changes, and crossfades
/// from the previous picture when the slide changes and transitions are on.
pub struct OutputFrame {
    name: &'static str,
    id: OutputId,
    renderer: Renderer,
    styles: Option<Arc<serviceflow_core::style::Styles>>,
    images: Option<Arc<Images>>,
    texture: Option<(Shown, egui::TextureHandle)>,
    /// `Output::changes` when the texture was last made.
    changes: u64,
    fading: Option<(egui::TextureHandle, Fade)>,
}

impl OutputFrame {
    pub fn new(name: &'static str, id: OutputId) -> Self {
        Self { name, id, renderer: Renderer::with_fonts(crate::fonts::shared()), styles: None, images: None, texture: None, changes: 0, fading: None }
    }

    /// Paint what the outputs show in `state`, filling `rect` of `ui`.
    pub fn paint(&mut self, ui: &egui::Ui, rect: egui::Rect, state: &State) {
        let (output, styles) = (&state.output, &state.styles);
        let content = match output.mode {
            OutputMode::Black => Content::Black,
            OutputMode::Logo => Content::Logo(state.output_settings.logo.clone().map(SameLogo)),
            // The blank between two items has its own background; a blank on a slide uses the slide's.
            OutputMode::Blank => Content::Slide(
                None,
                match &output.item_blank {
                    Some(blank) => blank.resolved.clone(),
                    None => output.live.as_ref().map(|live| live.resolved.clone()).unwrap_or_default(),
                },
            ),
            OutputMode::Normal => Content::Slide(
                // Chords are shown only on outputs whose chord toggle is on.
                output.live.as_ref().map(|live| (shown_slide(&live.slide, output.chords.get(self.id)), Arc::clone(&live.source))),
                output.live.as_ref().map(|live| live.resolved.clone()).unwrap_or_default(),
            ),
        };
        let pixels = rect.size() * ui.ctx().pixels_per_point();
        let size = Size { width: (pixels.x.round() as u32).max(1), height: (pixels.y.round() as u32).max(1) };
        let shown = Shown { content, size };
        let same_styles =
            self.styles.as_ref().is_some_and(|s| Arc::ptr_eq(s, styles)) && self.images.as_ref().is_some_and(|i| Arc::ptr_eq(i, &state.images));
        let changed = output.changes != self.changes;
        if changed || !(same_styles && matches!(&self.texture, Some((current, _)) if *current == shown)) {
            self.styles = Some(Arc::clone(styles));
            self.images = Some(Arc::clone(&state.images));
            let image = color_image(&self.render(&shown, styles, &state.images));
            // A slide change starts a crossfade from what was on screen; a resize or restyle
            // just redraws.
            let fade = if changed { state.fade_ms() } else { None };
            match (&mut self.texture, fade) {
                (Some((_, old)), Some(ms)) => {
                    let old = old.clone();
                    let handle = ui.ctx().load_texture(self.name, image, egui::TextureOptions::LINEAR);
                    self.fading = Some((old, Fade { start: ui.ctx().input(|i| i.time), duration: f64::from(ms) / 1000.0 }));
                    self.texture = Some((shown, handle));
                }
                (Some((current, handle)), None) => {
                    handle.set(image, egui::TextureOptions::LINEAR);
                    *current = shown;
                    self.fading = None;
                }
                (None, _) => self.texture = Some((shown, ui.ctx().load_texture(self.name, image, egui::TextureOptions::LINEAR))),
            }
            self.changes = output.changes;
        }
        let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let painter = ui.painter();
        let mut alpha = 1.0;
        if let Some((old, fade)) = &self.fading {
            let now = ui.ctx().input(|i| i.time);
            let progress = fade.progress(now);
            if progress < 1.0 {
                painter.image(old.id(), rect, whole, egui::Color32::WHITE);
                alpha = progress;
                ui.ctx().request_repaint();
            } else {
                self.fading = None;
            }
        }
        if let Some((_, handle)) = &self.texture {
            // The textures are premultiplied, so fading in is scaling every channel.
            painter.image(handle.id(), rect, whole, egui::Color32::from_white_alpha((alpha * 255.0).round() as u8));
        }
    }

    fn render(&self, shown: &Shown, styles: &serviceflow_core::style::Styles, images: &Images) -> Image {
        let size = shown.size;
        match &shown.content {
            Content::Black => Image { width: 1, height: 1, pixels: vec![0, 0, 0, 255] },
            Content::Logo(Some(logo)) => self.renderer.render_logo(&logo.0, &styles.default, size),
            Content::Logo(None) => self.renderer.render(&Slide::default(), &styles.default, size),
            Content::Slide(None, resolved) => {
                self.renderer.render_with_images(&Slide::default(), &Captions::default(), &resolved.style(styles), images, size)
            }
            Content::Slide(Some((slide, song)), resolved) => {
                let style = resolved.style(styles);
                let captions = Captions::for_song(song, &style);
                self.renderer.render_with_images(slide, &captions, &style, images, size)
            }
        }
    }
}

/// The slide as an output shows it: chords only when that output's toggle is on.
fn shown_slide(slide: &Slide, chords: bool) -> Slide {
    if chords { slide.clone() } else { slide.without_chords() }
}

/// Show the output window. With a `monitor` index it is borderless full screen there; without
/// one (browser, or no monitor list) it is an ordinary window. Returns true while it stays open.
///
/// The window cannot be exercised by the tests (it needs a display); it only paints the frame.
///
/// Keys pressed in the window (a clicker may be focused there) are added to `commands`.
pub fn show_window(
    ctx: &egui::Context,
    monitor: Option<(usize, &MonitorInfo)>,
    frame: &mut OutputFrame,
    shortcuts: &mut crate::shortcuts::Shortcuts,
    state: &State,
    commands: &mut Vec<Command>,
) -> bool {
    let mut builder = egui::ViewportBuilder::default().with_title("serviceFlow output").with_inner_size([960.0, 540.0]);
    if let Some((index, _)) = monitor {
        builder = builder.with_monitor(index);
    }
    let mut open = true;
    ctx.show_viewport_immediate(viewport_id(), builder, |ui, _class| {
        if ui.input(|i| i.viewport().close_requested()) {
            open = false;
        }
        commands.extend(shortcuts.commands(ui.ctx()));
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| {
            let rect = ui.max_rect();
            frame.paint(ui, rect, state);
        });
    });
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use serviceflow_core::song::Chord;

    #[test]
    fn an_output_shows_chords_only_when_its_toggle_is_on() {
        let slide = Slide { lines: vec!["Hi".into()], chords: vec![vec![Chord { offset: 0, text: "G".into() }]], ..Slide::default() };
        assert!(shown_slide(&slide, true).has_chords());
        let plain = shown_slide(&slide, false);
        assert!(!plain.has_chords());
        assert_eq!(plain.lines, slide.lines);
    }

    #[test]
    fn a_fade_runs_from_nothing_to_everything_over_its_duration() {
        let fade = Fade { start: 10.0, duration: 0.4 };
        assert_eq!(fade.progress(9.0), 0.0);
        assert_eq!(fade.progress(10.0), 0.0);
        assert!((fade.progress(10.1) - 0.25).abs() < 1e-6);
        assert_eq!(fade.progress(10.4), 1.0);
        assert_eq!(fade.progress(99.0), 1.0);
    }
}
