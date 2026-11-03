//! Keyboard and clicker control of the live slide, through craft-keymap.
//!
//! Every shortcut is a catalog entry with default bindings; the operator rebinds them in the
//! Keyboard Shortcuts dialog and the changes persist as `Prefs::shortcuts`. Command ids map to
//! engine commands in [`command_for`].
//!
//! A clicker remote sends arrow keys, Space or PageUp/PageDown. PageUp/PageDown also work while a
//! text field has the keyboard (the `Text:` bindings), so a remote works with the search box
//! focused. Plain keys otherwise wait while typing.
//!
//! Escape is deliberately not bound: it must never clear the output. (The shortcuts dialog still
//! uses Escape to cancel recording a key; that is the dialog's own, not an output action.)

use craft_keymap::egui::Dispatcher;
use craft_keymap::{Active, Bindable, Binding, CommandRef, Keymap, Overrides, Platform, Policy, Scope, Scopes};
use serde_json::json;
use serviceflow_core::command::Command;

use crate::tl;

/// The context that is active while a text field has the keyboard.
const TEXT: &str = "Text";

/// Sections that have a key of their own: the number keys, then the section types.
const SECTION_KEYS: &[(&str, &str)] = &[
    ("1", "Jump to verse 1"),
    ("2", "Jump to verse 2"),
    ("3", "Jump to verse 3"),
    ("4", "Jump to verse 4"),
    ("5", "Jump to verse 5"),
    ("6", "Jump to verse 6"),
    ("7", "Jump to verse 7"),
    ("8", "Jump to verse 8"),
    ("9", "Jump to verse 9"),
    ("C", "Jump to the chorus"),
    ("P", "Jump to the pre-chorus"),
    ("T", "Jump to the tag"),
];

/// The section jump command for `token`.
pub fn section_jump(token: &str) -> CommandRef {
    CommandRef::with_params("section.jump", json!({ "token": token }))
}

fn catalog() -> Vec<Bindable> {
    let key = |text: &str| Binding::parse(text).unwrap_or_else(|_| unreachable!("default binding {text:?} parses"));
    let live = |id: &str, label: &str, keys: &[&str]| {
        let mut bindable = Bindable::new(id, label, Scope::app()).with_path(vec![tl!("Live").to_string(), label.to_string()]);
        for k in keys {
            bindable = bindable.with_default(key(k));
        }
        bindable
    };
    let mut catalog = vec![
        live(
            "slide.next",
            tl!("Next slide"),
            &["Right", "Down", "Space", "PageDown", "Text: PageDown"],
        ),
        live("slide.previous", tl!("Previous slide"), &["Left", "Up", "PageUp", "Text: PageUp"]),
        live("item.next", tl!("Next set item"), &["Cmd+Right"]),
        live("item.previous", tl!("Previous set item"), &["Cmd+Left"]),
        live("output.black", tl!("Toggle black"), &["B"]),
        live("output.blank", tl!("Toggle blank"), &["W"]),
        live("output.logo", tl!("Toggle logo"), &["L"]),
    ];
    for (token, label) in SECTION_KEYS {
        let mut bindable = Bindable::new(section_jump(token), tl!(*label), Scope::app()).with_path(vec![tl!("Live").to_string(), tl!(*label).to_string()]);
        bindable = bindable.with_default(key(token));
        catalog.push(bindable);
    }
    // B is black, so the bridge takes Shift+B.
    catalog.push(
        Bindable::new(section_jump("B"), tl!("Jump to the bridge"), Scope::app())
            .with_path(vec![tl!("Live").to_string(), tl!("Jump to the bridge").to_string()])
            .with_default(key("Shift+B")),
    );
    catalog
}

/// The engine command a shortcut runs, if it is one of ours.
pub fn command_for(command: &CommandRef) -> Option<Command> {
    Some(match command.id.as_str() {
        "slide.next" => Command::NextSlide,
        "slide.previous" => Command::PreviousSlide,
        "item.next" => Command::NextItem,
        "item.previous" => Command::PreviousItem,
        "output.black" => Command::ToggleBlack,
        "output.blank" => Command::ToggleBlank,
        "output.logo" => Command::ToggleLogo,
        "section.jump" => Command::JumpToSection { token: command.params.get("token")?.as_str()?.to_owned() },
        _ => return None,
    })
}

/// The keymap and the dispatcher that reads each frame's key presses against it.
pub struct Shortcuts {
    keymap: Keymap,
    dispatcher: Dispatcher,
    platform: Platform,
}

impl Shortcuts {
    pub fn new(overrides: Overrides) -> Self {
        let platform = Platform::current();
        Self { keymap: Self::build(platform, overrides), dispatcher: Dispatcher::new(), platform }
    }

    fn build(platform: Platform, overrides: Overrides) -> Keymap {
        Keymap::new(catalog(), Scopes::new().with_context(TEXT), platform).with_overrides(overrides)
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn keymap_mut(&mut self) -> &mut Keymap {
        &mut self.keymap
    }

    /// The operator's changes to the defaults, for storing.
    pub fn overrides(&self) -> &Overrides {
        self.keymap.overrides()
    }

    /// Replace the bindings of `command`, taking the keys from whatever else held them.
    pub fn rebind(&mut self, command: &CommandRef, keys: &[&str]) -> bool {
        let bindings: Option<Vec<Binding>> = keys.iter().map(|k| Binding::parse(k).ok()).collect();
        let Some(bindings) = bindings else { return false };
        self.keymap.set_bindings(command, bindings, Policy::Steal).is_ok()
    }

    /// The engine commands for the keys pressed this frame in the current viewport. The presses
    /// are consumed, so widgets (a focused button reacts to Space) do not also act on them.
    pub fn commands(&mut self, ctx: &egui::Context) -> Vec<Command> {
        let platform = craft_keymap::egui::platform(ctx);
        if platform != self.platform {
            // The browser reports its operating system only once the first frame starts.
            self.platform = platform;
            self.keymap = Self::build(platform, self.keymap.overrides().clone());
        }
        let typing = ctx.egui_wants_keyboard_input();
        let contexts: &[&str] = if typing { &[TEXT] } else { &[] };
        let active = Active { panel: None, contexts, typing };
        let hits = ctx.input_mut(|input| self.dispatcher.dispatch(input, &self.keymap, &active));
        hits.iter().filter_map(|hit| command_for(&hit.command)).collect()
    }
}
