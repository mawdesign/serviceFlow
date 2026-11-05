//! Style editing: edits of the default style and of a set item's style, with undo and redo.
//!
//! A set item's style is part of the open set, so its edits go through the set's history, are
//! saved with the set (ADR-0002, in OpenSong's units by the codec) and are undone with
//! `UndoSet`. The default style is serviceFlow's own (ADR-0003): it is never written to OpenSong's
//! `Settings/`. Its edits have their own history, are applied over what is imported from
//! OpenSong, and are written to the library's sidecar when an undo step ends.

use std::sync::Arc;

use super::set_edit;
use crate::command::{Command, Effect};
use crate::set_edit::{Change, ItemField, Target};
use crate::state::{Problem, State};
use crate::style::{Style, Styles};
use crate::style_edit::{DefaultStyle, StyleEdit, StyleTarget};

pub(super) fn apply(state: &mut State, command: Command) -> Vec<Effect> {
    match command {
        Command::EditStyle { target: StyleTarget::Default, edit } => edit_default(state, &edit),
        Command::EditStyle { target: StyleTarget::Item(item), edit } => {
            let target = Some(Target { item, field: ItemField::Style });
            set_edit::change(state, target, |set| {
                let mut set = set.clone();
                let item = set.items.get_mut(item)?;
                // An item has a style only when it has been given one.
                let style = edit.applied(item.style.as_ref()?)?;
                item.style = Some(Arc::new(style));
                Some((set, Change::InPlace))
            })
        }
        Command::UndoStyle | Command::RedoStyle => step_history(state, command == Command::UndoStyle),
        Command::DefaultStyleLoaded(edited) => {
            state.default_style.edited = edited;
            let styles = Arc::clone(&state.styles);
            super::set_styles(state, styles);
            Vec::new()
        }
        Command::DefaultStyleLoadFailed { error } => {
            state.problems.push(Problem::DefaultStyleUnreadable { error });
            Vec::new()
        }
        Command::StyleSidecarSaved => Vec::new(),
        Command::StyleSidecarSaveFailed { error } => {
            state.problems.push(Problem::DefaultStyleUnsaved { error });
            Vec::new()
        }
        _ => super::unrouted(&command),
    }
}

fn edit_default(state: &mut State, edit: &StyleEdit) -> Vec<Effect> {
    let Some(style) = edit.applied(&state.styles.default) else { return Vec::new() };
    if style == state.styles.default {
        return Vec::new();
    }
    let before = Arc::new(state.styles.default.clone());
    let history = &mut state.default_style.history;
    if !history.grouping {
        history.undo.push(before);
    }
    history.grouping = true;
    history.redo.clear();
    show(state, style);
    state.default_style.unsaved = true;
    Vec::new()
}

fn step_history(state: &mut State, undo: bool) -> Vec<Effect> {
    let current = Arc::new(state.styles.default.clone());
    let history = &mut state.default_style.history;
    history.grouping = false;
    let (from, to) = if undo { (&mut history.undo, &mut history.redo) } else { (&mut history.redo, &mut history.undo) };
    let Some(version) = from.pop() else { return Vec::new() };
    to.push(current);
    show(state, (*version).clone());
    state.default_style.unsaved = true;
    save(state)
}

/// Make `style` the default style being drawn, and the edited one.
fn show(state: &mut State, style: Style) {
    state.default_style.edited = Some(Arc::new(style.clone()));
    state.styles = Arc::new(Styles { default: style, scripture: state.styles.scripture.clone() });
}

/// Write the sidecar if there is something to write.
fn save(state: &mut State) -> Vec<Effect> {
    let ds = &mut state.default_style;
    if !std::mem::take(&mut ds.unsaved) {
        return Vec::new();
    }
    vec![Effect::WriteStyleSidecar(ds.edited.clone())]
}

/// The undo step ends: the next edit starts a new one, and what was edited is written.
pub(super) fn end_group(state: &mut State) -> Vec<Effect> {
    state.default_style.history.grouping = false;
    save(state)
}

/// The operator asked to import OpenSong's styles again: their own edits of the default style
/// are dropped (and the sidecar says so).
pub(super) fn reset(state: &mut State) -> Vec<Effect> {
    let had_edits = state.default_style.is_edited();
    state.default_style = DefaultStyle::default();
    if had_edits { vec![Effect::WriteStyleSidecar(None)] } else { Vec::new() }
}

/// The styles to draw with once OpenSong's have been read: the operator's edited default style,
/// if any, stays over OpenSong's.
pub(super) fn over(state: &State, mut styles: Arc<Styles>) -> Arc<Styles> {
    if let Some(edited) = &state.default_style.edited {
        Arc::make_mut(&mut styles).default = (**edited).clone();
    }
    styles
}
