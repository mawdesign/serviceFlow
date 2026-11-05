# serviceFlow-only data lives in sidecar files, not in OpenSong XML

Some information has no OpenSong equivalent: per-set-item transposition, language mode, and style fields OpenSong cannot express. This is stored in sidecar files in a dot-prefixed folder beside the OpenSong files, rather than as extra elements inside the XML.

OpenSong can strip unknown elements when it re-saves a file, so data embedded in the XML would be lost silently whenever someone used OpenSong. A sidecar keeps the OpenSong files clean and OpenSong-readable. If a sidecar is lost, only serviceFlow extras are lost, never content. OpenSong already ignores any file whose name starts with `.` or `_`, so the sidecar folder never appears in its song list.

## Library-level sidecar: the edited default style

Not every serviceFlow-only setting belongs to a song or a set. The operator's edits of the **default style** (the style editor, #30) belong to the library, so they are kept in `.serviceflow/styles.sidecar` at the data root: the same dot folder, the same TOML codec crate, shared by everyone using that data root. OpenSong's `Settings/PresentSettings` is never written. The sidecar holds the whole edited style (including the extras OpenSong has no place for) or nothing; when it holds a style, that style replaces the default imported from OpenSong, and "Re-import styles from OpenSong" drops it. This follows this ADR, so it is not a deviation in ADR-0007. A style pushed by a style item is part of the set and is written into the set in OpenSong's own units.
