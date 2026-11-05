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
A line of chords positioned by column above the lyric line that follows it. Each output has its own chord toggle, off by default; control-window thumbnails and the editor preview always show chords. In a multilanguage section chords sit above primary lines only.

**Transposition**:
A shift of a song's chords by a number of semitones, applied only to a set item's display and kept in the set's sidecar. Only the chord names change, never their positions. The target key decides whether sharps or flats are written; chords the parser does not recognise are left as they are. It changes the song itself only when the operator chooses "Save in this key", a normal canonical save that rewrites the chord lines and the key and puts the item back at zero.

**Capo**:
The fret a guitarist clamps, which changes the chord shapes they play but not the key that sounds. A set item can set its own capo (the song's own printed capo applies otherwise). Slides then show the shapes, the chords relative to the capo, and the control window shows the sounding key beside them.

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
A set item that serviceFlow keeps but cannot present: an unsupported type such as scripture or external. A song item whose song is missing stays a song item, flagged `missing` until the song is back; it is not turned into a placeholder.
_Avoid_: Broken item, error item

**Item notes**:
Operator-only notes attached to a set item. They are never projected.

## Appearance

**Style**:
The full visual specification for a slide: title, subtitle and body text formatting, translation-line formatting, and background.
_Avoid_: Theme, template, look

**Default style**:
The style used when nothing more specific applies. It starts as OpenSong's (imported from its settings, never written back); edits made in the style editor are serviceFlow's own and are kept in the library's sidecar, over OpenSong's.

**Scripture style**:
The default style for scripture items.

**Style editor**:
The window where the operator edits the title, subtitle and body boxes and the background of the default style or of a style item's pushed style, with a live preview drawn by the real renderer. Edits of a style item are set edits; edits of the default style are serviceFlow's own.

**Style stack**:
The styles pushed by style items so far in a set. The top one applies to the items that follow.

**Set item style**:
A style stored on one set item. It applies to that item only and beats every other style.

**Song style**:
A style stored in a song file. It beats the style stack when the OpenSong setting "song style preferred" is on (the default), and also applies when no style item is in effect. A set item style beats it.

**Song background**:
Images a song file names for its sections (`<backgrounds>`). The nth image of a section goes with the nth slide of that section, and the last one repeats. With "background as text" the image replaces the lyrics.

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
A destination where slides are shown. In Phase 1 there are two, `OutputId::Window` (the output window) and `OutputId::Monitor` (the small mirror in the control window). Each has its own chord toggle; the live slide, blank, black and logo apply to both.
_Avoid_: Screen, display, projector (unless physical hardware is meant)

**Live**:
The slide currently shown on the outputs.

**Blank**:
An output state showing the style's background with no text.

**Blank between items**:
An output state, not an item or a slide of the set: the style's background with no text, which stepping reaches between two set items (`State.output.item_blank`). It stops the next item (a video, say) starting the moment the operator steps past the end of the last one. Shown only while presenting: never in the set or the set builder. Only with the `blanks` setting on; none before the first item or after the last, and none between two items with the same name (so the operator can group items into one). Its background is the item before's, or the item after's with `blank_uses_next`, including that item's own style.
_Avoid_: Spacer, separator slide

**Auto-advance**:
A custom or image item moving to its next slide by itself after its `seconds`. With `loop` it returns to the first slide after the last; without it stops on the last slide and never carries on into the next item. Any manual step, blank, black or logo, or leaving the item, stops or restarts it.

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

**Change plan**:
The full list of files one action will write (a song plus the sets that refer to it), shown to the operator, who applies or cancels it before anything is written. Renaming tags on save, renaming or moving a song file, deleting a song and creating a song each make one.

**Tidy library**:
An operator-initiated action that rewrites every file not yet in canonical form, after showing a dry-run list of the changes.

**Sidecar**:
A file storing serviceFlow-only information about a song or set (such as transposition or language mode), kept beside the OpenSong files rather than inside them.
