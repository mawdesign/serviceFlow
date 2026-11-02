# serviceFlow

serviceFlow is a worship and media presentation application: operators build a running order of songs and slides for a service and project them live. Phase 1 reaches parity with OpenSong and works directly on OpenSong data folders.

## Library

**Data root**:
The folder holding a congregation's material in OpenSong layout (`Songs/`, `Sets/`, `Backgrounds/`, `Settings/`). serviceFlow works on one data root at a time.
_Avoid_: Database, workspace, OpenSong folder

**Library**:
All songs under the data root's `Songs/` folder.
_Avoid_: Song database, song list

**Category**:
A subfolder of `Songs/` used to group songs.
_Avoid_: Folder (in user-facing text), collection, tag

**Song filename**:
The name of the file a song is stored in. Independent of the title; neither ever changes the other.
_Avoid_: Song name (ambiguous)

**Song theme**:
The topical labels on a song (e.g. "Worship: Intimacy"). Not a visual concept.
_Avoid_: Theme (unqualified), topic

**Hymn number**:
A song's number in a printed hymnal, searchable as `#n`.

## Songs

**Song**:
A piece of lyrics, optionally with chords, plus its metadata (title, author, copyright, CCLI number, key, capo, and so on).
_Avoid_: Track, hymn (except in hymnal contexts)

**Section**:
A named part of a song's lyrics, such as a verse, chorus or bridge, introduced by a section tag.
_Avoid_: Part, block, stanza

**Section tag**:
The label of a section, such as `V1`, `C`, `B` or `Coda`. Its first letter gives the section type.
_Avoid_: Verse label, heading

**Section type**:
The kind of section: verse, pre-chorus, chorus, bridge or tag. Any unrecognised letter counts as verse.

**Numbered verse block**:
A single `[V]` section whose lyric lines start with digits 1–9. Each digit's lines form their own verse (V1, V2, …), and all of them share the same chord lines.
_Avoid_: Multi-verse, column verses

**Multilanguage section**:
A section whose lyric lines alternate between a primary line and a translation line.
_Avoid_: Bilingual verse, L-section, dual-language section

**Primary line**:
The first line of each pair in a multilanguage section. It is shown at full size.

**Translation line**:
The second line of each pair in a multilanguage section. It is shown smaller and styled differently, directly beneath its primary line.
_Avoid_: Second language, subtitle

**Language mode**:
Whether multilanguage sections are shown as both lines, primary lines only, or translation lines only.

**Presentation order**:
The sequence of section tags a song is shown in. When it is empty, sections are shown in file order. Sections not listed are not part of the flow.
_Avoid_: Verse order, arrangement, running order

**Slide break**:
A marker (`||`) that splits a section across several slides.

**Line break**:
A marker (`|`) that forces a new line on screen within a lyric line.

**Chord line**:
A line of chords positioned by column above the lyric line that follows it.

**Transposition**:
A shift of a song's chords by a number of semitones, applied only to a set item's display. It changes the song itself only when the operator explicitly saves the song in the new key.

**Capo**:
The fret a guitarist clamps, which changes the chord shapes they play but not the key that sounds.

## Sets

**Set**:
An ordered running order of set items for a service.
_Avoid_: Playlist, service plan, setlist

**Set item**:
One entry in a set: a song item, custom slide item, image item, style item, or a placeholder.
_Avoid_: Slide group (OpenSong's term), cue

**Song item**:
A set item that refers to a song. It can override the song's presentation order, transposition and language mode.

**Custom slide item**:
A set item holding its own titled text slides that are not drawn from the library.

**Image item**:
A set item that shows one or more images.

**Style item**:
A set item that changes the style of the items after it, either by pushing a new style or by reverting to the previous one.

**Placeholder**:
A set item that serviceFlow keeps but cannot present: an unsupported type such as scripture or external, or a reference to a song or image that is missing.
_Avoid_: Broken item, error item

**Item notes**:
Operator-only notes attached to a set item. They are never projected.

## Appearance

**Style**:
The full visual specification for a slide: title, subtitle and body text formatting, translation-line formatting, and background.
_Avoid_: Theme, template, look

**Default style**:
The style used when nothing more specific applies.

**Scripture style**:
The default style for scripture items.

**Style stack**:
The styles pushed by style items so far in a set. The top one applies to the items that follow.

**Auto-scale**:
Shrinking a slide's text so the whole slide fits the screen. Translation lines shrink in proportion to primary lines.

## Live presentation

**Operator**:
The person running the presentation during a service.
_Avoid_: User (when the presentation role is meant), tech

**Control window**:
The operator's window, where the set, slides and output monitor are shown and the presentation is run.
_Avoid_: Main window, presenter view

**Output**:
A destination where slides are shown to an audience. In Phase 1 this is the output window.
_Avoid_: Screen, display, projector (unless physical hardware is meant)

**Live**:
The slide currently shown on the outputs.

**Blank**:
An output state showing the style's background with no text.

**Black**:
An output state showing pure black.

**Logo**:
An output state showing the configured logo image.

**Slide**:
One screenful of content produced from a set item.

## Files

**Canonical form**:
The single preferred way serviceFlow writes a song or set. For example, sections are always written as `[V1]` and `[V1] (L)`, never as `[1]` or `[V1-L]`.

**Canonical save**:
Saving an edited file in canonical form, together with any sets that need updating to stay consistent with it.

**Tidy library**:
An operator-initiated action that rewrites every file not yet in canonical form, after showing a dry-run list of the changes.

**Sidecar**:
A file storing serviceFlow-only information about a song or set (such as transposition or language mode), kept beside the OpenSong files rather than inside them.
