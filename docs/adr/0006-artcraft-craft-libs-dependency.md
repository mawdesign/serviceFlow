# Build the UI on ArtCraft's craft-libs (egui), pinned by git revision

The GUI is built with egui and ArtCraft's `craft-libs` crates from `storytold/craft-libs`. The main ones are `craft-ui`, `craft-window` (one egui viewport per window, used here for the control window plus the output window), `craft-text`, `craft-fonts`, `craft-atomic`, `craft-settings`, `craft-dialogs`, `craft-keymap` and `craft-i18n`.

Reusing them keeps maintenance down within this project and benefits from ArtCraft's active development. DeckCraft, from the same suite, already shows a dual-screen presenter on this stack.

iced was rejected because it would have meant giving up all of that reuse.

## Consequences

- craft-libs is not on crates.io. The crates are git dependencies pinned to a specific revision. A `craft-ui` crate on crates.io comes from a third-party fork and must not be used.
- The workspace must copy craft-libs' `[patch]` entries for its forks of egui, eframe and winit. Upgrading means deliberately moving the pinned revision forward, not following the latest changes.
- DeckCraft's patterns are copied rather than added as dependencies, because DeckCraft is an application, not a library.
