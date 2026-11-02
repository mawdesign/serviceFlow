# OpenSong XML is the native format, written in canonical form on save

serviceFlow reads and writes OpenSong data folders directly. It does not import them into a format of its own. This makes migration risk-free, and teams can keep OpenSong and serviceFlow running side by side on one shared folder.

Files are read leniently, accepting every legacy variant. They are written in a single **canonical form**, because one way of writing each thing keeps later phases (remote storage, MCP, a structured song model) simpler.

## Rules

- Section tags are written as `[V1]` and `[V1] (L)`. OpenSong deprecates the `[V1-L]` form, so it is converted on write.
- Presentation-order tokens are rewritten to match the renamed tags.
- Element order is fixed. Unknown elements are kept.
- Files are written as UTF-8 with CRLF line endings, which is what OpenSong itself writes.
- Numbered verse blocks stay compact.

## Considered Options

- **Byte-for-byte or semantically lossless round-trip.** Rejected: it would keep every legacy variant alive forever.
- **Import into a serviceFlow format.** Rejected: OpenSong teams could no longer share a folder.

## Consequences

- Canonicalisation happens only when a file is saved because it was edited, or through the explicit **Tidy library** action, which previews its changes. Opening a file never rewrites it, so a shared library does not churn.
- Renaming a song's tags, or renaming or moving the song file, also rewrites every set that references it, in one listed multi-file change. Otherwise the library would break for anyone still on OpenSong.
- If a tag rename would collide with an existing tag (for example, a song containing both `[1]` and `[V1]`), the tags are not renamed and the editor warns instead.
- A song's filename and title are independent, and neither ever renames the other. Teams rely on filenames for search tricks such as `Amazing Grace HOF#50`.
- New songs get a `.xml` extension. Existing files are never given one. OpenSong ignores extensions and lists every visible file, so `.xml` files work there too; they simply show the extension in OpenSong's list.
