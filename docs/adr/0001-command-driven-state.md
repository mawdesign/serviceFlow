# All state changes go through commands; the UI is a view of state

The UI must never mutate data directly. Each frame it reads an immutable snapshot of application state, and it expresses user actions as commands. A single engine applies every command, owns undo history, and is the only writer.

The file watcher, other serviceFlow instances, and future remote sync and MCP agents send the same commands. As a result, concurrent changes cannot leave the UI out of sync, and every caller (UI, automation, AI) has exactly the same capabilities. This follows the pattern in ArtCraft's DeckCraft ("everything is a command; the UI is thin").

## Consequences

- Undo covers document edits only. Live actions such as going live or blanking are transient and cannot be undone.
- An external reload of a document clears that document's undo history rather than trying to rebase it.
- Layering is enforced mechanically by a build check, not left to convention: the core and storage crates may not depend on any GUI, windowing or concrete storage crate.
