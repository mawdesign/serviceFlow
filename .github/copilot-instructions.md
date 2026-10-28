# Copilot instructions for serviceFlow

## Project snapshot

- `serviceFlow` is a Rust-based media and worship presentation engine for the Lyric Sanctuary ecosystem.
- The product is not a generic app; it is designed to run a full church service, from notices and videos to sermon slides, chord sheets, and live presentation output.
- The repo currently appears documentation-first and lightweight; there are no checked-in build/test/lint configs in the current working tree.

## Build, test, and lint commands

No project manifest or CI configuration is present in the current repository snapshot (no `Cargo.toml`, `package.json`, `Makefile`, or similar files were found in the checked-out tree). That means there is no verified local build/test/lint command to run yet.

When this project becomes a Rust workspace, the expected commands are the standard Cargo commands:

- `cargo build`
- `cargo test`
- `cargo test <test_name>` for a single test
- `cargo clippy --all-targets --all-features -- -D warnings`

Do not invent repo-specific commands that are not present in the working tree.

## High-level architecture

The architecture is best understood from the product goals in `README.md`, not from any single source file:

- Content and import layer: service data, song/lyrics/chord content, OpenSong/XML compatibility, and service-flow definitions.
- Presentation layer: the "run of show" model that sequences notices, videos, sermon slides, and other items within a live service.
- Output layer: dual-screen or multi-display presentation, overlays, lower-thirds, stage displays, and projector/audience output.
- Cross-platform runtime: native desktop execution plus WebAssembly/web access, with low-latency behavior as a first-class requirement.

Treat features as part of a live presentation pipeline, not as an isolated UI or document workflow. Shared state should generally track service timeline progression, screen output, and media playback, rather than act like a generic CRUD app.

## Key conventions and constraints

- Keep the product aligned with the repo's purpose: live worship/media presentation, not a general-purpose music player or slide app.
- Favor low-latency, deterministic presentation behavior over complex background processing patterns.
- Maintain a clear separation between data import/management and live presentation output; content and playback logic should not be conflated.
- For new features, consider the full service-run context: what is on screen, what is next, and how it is presented across multiple outputs.
- If a new module is added, keep its responsibilities consistent with the product model described in the README: show flow, media timing, display orchestration, and cross-platform runtime behavior.

## Repository conventions

- The repo is intentionally sparse at the moment; prefer reading the product goal and documentation before adding new modules or abstractions.
- Avoid speculative project structure that assumes a web app, Node app, or database-driven service unless the repo later adds those manifests.
- When the project expands, prefer the architecture implied by the README: a Rust-first, presentation-focused system with cross-platform delivery and multi-output orchestration.
