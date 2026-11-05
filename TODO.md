# TODO

Things we could not settle while building Phase 1. Each item names the ticket where it came up
(`#n` is the GitHub issue). Tick an item off and, when it changes behaviour, record the answer in
`GLOSSARY.md` or an ADR (ADR-0007 for deliberate differences from OpenSong).

## To confirm in OpenSong

Run the real OpenSong (or read its source, notes in `.sf-notes/opensong.md`) and compare.

- [ ] **Image item titles (#15).** Do image items show their title or subtitle over the picture? We
  show the picture alone. Also check what `descriptions_in_subtitle` does.
- [x] **Per-item slide options (#15, #10, #16, #29).** Settled by the operator: `seconds` is how
  long each slide of the item shows before advancing; `loop` returns to the first slide after the
  last (otherwise it stops there and does not run into the next item); `transition` is the fade
  between the item's slides. Built for custom and image items (GLOSSARY "Auto-advance").
- [ ] **`link` on a custom or image item (#15, #29).** Still unknown. We keep it in the item's XML
  and write it back untouched; we do not interpret it. The operator will search historic sets to
  see whether any use it.
- [ ] **`transition` values (#29).** OpenSong's source has 0 = as the output settings, 1 = fade,
  2 = no fade. We follow that and fade for the output's `transition_time`. If a set uses
  `transition` as a time in milliseconds instead, tell us.
- [ ] **Auto-advance on song items (#29).** Only custom and image items advance by themselves. A
  song item with `seconds`/`loop` attributes keeps them but ignores them. Does OpenSong honour
  them on songs?
- [ ] **Image paths outside `Backgrounds/` (#15).** We treat `../x` and similar as missing. Does
  OpenSong follow them?
- [ ] **Song style when `song_style_preferred` is off (#16).** We follow the source: the song's own
  style still applies when no style item is in effect. Confirm in the running app.
- [x] **Blank between set items (#10, #29).** Settled by the operator and built: with `blanks` on,
  stepping past the end of an item (or back past its start) lands on a blank showing the item
  before's background (the item after's with `blank_uses_next`), including that item's style. None
  between two items with the same name, none before the first item or after the last. Blanks
  appear only in the presentation helper, never in the set.
- [ ] **Blank beside placeholders and missing songs (#29).** Items we cannot present (scripture,
  external, style, missing songs) are passed over, so the blank sits between the two presentable
  items either side of them, and the same-name rule compares those two. Confirm that is what the
  operator wants. A song opened straight from the library (no set) has no blank after it.
- [ ] **Capo display (#25).** We show chord shapes relative to the capo (the sounding chord moved
  down by the capo fret, spelled for the key the shapes are played in) and offer the sounding key
  beside them. OpenSong's source was not at hand: check in the running OpenSong what a song with
  `<capo print="true">` prints (shapes, or the written chords with a "Capo n" line), and whether
  `sharp="true"` on `<capo>` means sharp spelling. We apply a song's own capo only when
  `print` is true, and the item's capo replaces it.
- [ ] **Sharps and flats for transposed chords (#25).** The target key's usual signature decides
  (G up one = Ab major, so flats; F# and Gb major both take sharps; Eb minor takes flats). A song
  with no key takes sharps going up and flats going down. Ask a musician whether the choices read
  right, in particular for keys with six accidentals.
- [ ] **Logo mask (#10).** `logo_mask` is read but not applied. What does it do?
- [ ] **Unit conversion for imported styles (#8).** We scale OpenSong sizes and margins (640-wide
  reference) by 3 and add the border of width/50 to each margin. Compare an imported style against
  OpenSong's own output side by side.
- [ ] **Section type of unknown tags (#7).** By the first-letter rule `[Coda]` counts as a chorus.
  Is that what OpenSong does for colouring and for the section keys?
- [ ] **Multilanguage edge cases (#11).** A `||` between a primary line and its translation (we move
  it after the translation and warn), unequal numbers of `|` in a pair, and the italic flip on
  translation lines.
- [ ] **Missing songs in a set (#14).** OpenSong drops them; we show a placeholder. Is a song
  "missing" by exact id only? Check case sensitivity and categories.
- [x] **Moving past the last slide of an item (#14, #10, #29).** The blank comes first, then the
  next item. Going back is the mirror: the blank, then the earlier item's last slide.
- [ ] **Search (#12).** OpenSong searches filenames only; we search more fields, and `#n` matches the
  hymn number or a `#n` in the filename not followed by another digit. Confirm `#n` finds the same
  songs OpenSong finds on a real library.
- [ ] **New song filename (#20).** We use the sanitised title plus `.xml`, then ` (2)`, ` (3)`.
  Check what characters OpenSong refuses or replaces, and that OpenSong lists a file with an
  extension.
- [ ] **Text encodings (#7).** We detect a byte-order mark, then UTF-16 without one, then the
  declared encoding, then UTF-8, falling back to Windows-1252. Check old libraries with other
  encodings.
- [ ] **Key bindings (#6, #17).** B (black), W (blank), L (logo) and the section keys are our
  choices. Compare them with OpenSong's keys and decide what to keep.
- [ ] **Items written by serviceFlow (#22).** New and edited custom, image and style items are
  written from the model (`<notes/>`, `keep_aspect="false"`, `resize="screen"`, a pushed style
  with every attribute). Check OpenSong reads them as written, and what it does with an image
  item that has `resize` but no `seconds`/`loop`.
- [ ] **Pushed style units (#22).** A new style item copies the default style and writes it back
  in OpenSong's units (sizes ÷ 3, margins ÷ 3 minus the border, rounded to 2 decimals). Check a
  pushed style looks the same in OpenSong as the default style it copied.
- [ ] **Style editor units (#30).** The editor works in reference pixels (1080 high) and rounds
  sizes and margins to what OpenSong's file holds (÷3, margins also minus the 12.8 border, two
  decimals; a margin never goes below OpenSong's 0). Check in OpenSong that a pushed style edited
  here looks the same there, and that OpenSong accepts a font size with decimals (we write 33.33).
  Also whether OpenSong honours `<style>` on a song, custom or image item the way it does on a
  style item (we only edit styles that exist; the editor does not add one to other items).
- [ ] **Chord display on output (#24).** OpenSong never projects chords (its song display filters
  `.` lines), so there is nothing to compare; the look is ours. Check with musicians: chord size
  (60% of the body), colour (amber), and how a chord line that is wider than its lyric line
  should behave (we let it run past the lyric line).
- [ ] **Chord columns and `_` (#24).** We anchor each chord to the character in its column after
  trimming and removing `_`, so a chord over `_` or leading spaces moves to the next character.
  Compare with OpenSong's chord sheet (`CleanLyrics`, printing) on songs that use `_` spacers.

## To test

Things that need a real display, real hardware, other platforms or real data. Automated tests
cannot cover them.

### Windows, displays and hardware
- [ ] **Output window (#6, #10).** Full screen and borderless on the chosen monitor, `SetMonitor`
  while it is open, cursor hiding, and monitor enumeration through the winit wrapper.
- [ ] **Second monitor (#6, #10).** The remembered choice, the PresentSettings monitor as the
  initial one, and the fallback when a monitor is unplugged.
- [ ] **Crossfade (#10).** The fade time and look. The fixtures have transitions off, so turn them
  on to see it.
- [ ] **Control window overall (#2, #5).** Open it on macOS (Intel and Apple silicon), Windows and
  Linux and look at it. The native window was never visually checked.
- [ ] **A real clicker (#17).** PgUp/PgDn with the search box focused, and the rebinding dialog
  (it was only checked to open).
- [ ] **Fonts (#9).** The real system-font lookup (craft-font-catalog) is compiled but only run
  against a stub. Try a style that names an installed font and one that does not.
- [ ] **Italic (#11).** Italic is a slanted regular face. Check it looks acceptable, and that real
  italic faces are not needed yet.

- [ ] **Chords on the projector (#24).** The chord toggles for the output window and the control
  monitor, on a real second screen. Their state is not remembered between runs.

- [ ] **Transposition and capo with a band (#25).** Transpose a real song for a service, check
  the chord names on the monitor read right, that a capo shows the shapes a guitarist expects, and
  that "Save in this key" leaves OpenSong able to open the song with the chords still lined up
  over the words (a chord that grows pushes its neighbours right, so very tight lines may shift).
- [ ] **Blanks and auto-advance on the projector (#29).** With `blanks` on, step through a set on
  the real output: check the blank between items looks right (previous item's background, or the
  next item's with `blank_uses_next`; a set item style shows), that Next and Previous pass through
  it in order, and that items with the same name run together without one. Then run a custom or
  image item with `seconds` set: timing against a wall clock, `loop`, the fade between slides
  (`transition`), and that a manual step, B/W/L or leaving the item stops or restarts it. The
  timer was only tested with a fake clock.

- [ ] **Style editor (#30).** Open it from a real control window and use it: the colour pickers
  (an undo step ends when the pointer is released), the font box (the warning shows as you type;
  try an installed font, a missing one and a CJK one), margins and sizes while dragging, the
  preview against the projector, undo and redo of both the default style and a style item.
  Automated tests drive the font box and the checkboxes only (the colour popups and drop-downs were
  not clicked).

### Data and files
- [ ] **Canonical save on a real library (#19).** Copy a real data root, save a few edited songs,
  and open them in OpenSong. Check nothing is lost (comments, capo attributes, odd elements).
- [ ] **What a save still drops (#19, #21).** Tidy library leaves alone (and lists) any file
  holding what the codec does not keep: XML comments, attributes on `<song>` and on the standard
  elements (other than `<capo>`), a repeated standard element, elements inside one, stray text,
  `<timesig>` beside `<time_sig>`, and in sets attributes of `<slide_groups>` or a second
  `<notes>` on a song item (`serviceflow_opensong::unkept_in_song`/`unkept_in_set`). Saving an
  edited song or set still rewrites it canonically and drops those without a warning. Showing the
  same list before such a save needs the file's bytes in the save path; count how often real
  libraries hit it first.
- [ ] **Colons and backslashes in file names (#4).** On macOS and Linux a song or set called
  `Psalm 23: The Lord's my shepherd` (or with a `\`) is listed and opened as normal. On Windows such
  names cannot exist. In the browser such files (a Chromium folder on a Mac) are not listed at all,
  with no message, because the host may be Windows. Check what OpenSong itself does with them.
- [ ] **Open without edits (#19).** Open many songs from a real library and confirm no file changes
  (check with `git diff` on a copy).
- [ ] **Change plans (#20).** Rename a song used by several sets and check every set. Interrupt a
  plan part-way (kill the app) and check that applying it again finishes the job.
- [ ] **File watching (#13).** Edit files from OpenSong, a text editor and a sync client (Dropbox,
  OneDrive) while serviceFlow is open. Try a network share, where polling may miss an edit that
  keeps both size and modified time.
- [ ] **Large libraries (#12, #27).** Indexing runs on the UI thread in chunks. Time it on a
  library of several thousand songs.
- [ ] **Many large photos in one set (#15).** All images named by the open set load into memory
  when it opens. Watch memory with a set of big photos.
- [ ] **Sidecars (#23, #31).** Language mode, transposition and capo are restored after saving and
  reopening a set, and stay with their items when items are reordered on a real set (the same
  song twice included). Rename a set from the set panel and check `.serviceflow/Sets/` holds only
  the new name's sidecar.
- [ ] **Set editing on a real library (#22).** Build a set by dragging songs in, reorder by
  dragging, add custom/image/style items, save, and open it in OpenSong. Also save a set while
  OpenSong has just changed it, and check the conflict choices. Dragging was only tested with
  synthetic pointer events.

### Build and release
- [ ] **CI on Windows and Linux (#2).** Those legs are written but have not run.
- [ ] **GitHub Pages (#2).** Set the repository's Pages source to "GitHub Actions", then check the
  deploy.
- [ ] **macOS universal builds (#2).** DeckCraft has no unsigned universal binary recipe; build
  both Intel and Apple silicon.
- [ ] **Browser build size (#26).** The CJK faces (8.4 MB) are no longer compiled into the wasm;
  the other bundled faces (about 2.5 MB) still are. Check the download size and first-load time of
  the Pages build on a slow connection.
- [ ] **Browser tests on CI (#26).** The `wasm` job now runs the storage contract in headless Chrome
  and Firefox (`cargo xtask browser-test`). Only Chrome was run locally; check the Firefox leg and
  that the runner download matches the wasm-bindgen version in `Cargo.lock`.

### Browser (#26)
- [ ] **A real OpenSong folder in Chrome or Edge.** Open a data root with "Open OpenSong folder…",
  edit and save songs and sets, and check OpenSong reads them. Edit files in OpenSong while the
  page is open: changes should appear within about 4 seconds (polling every 2 s, two scans must
  agree). Check the permission prompts, and a folder on a network share or sync folder.
- [ ] **Firefox and Safari.** Browser storage opens at once, with the banner. Import a zip of a real
  OpenSong folder (made by Finder, Windows Explorer and 7-Zip), present from it, export it and open
  the export in OpenSong. Safari only recently gained `createWritable()` for OPFS on the main
  thread: if writing fails there, the storage needs a worker with `createSyncAccessHandle`.
- [ ] **Browser storage survives.** Close and reopen the browser; check the library is still there,
  and whether the browser granted persistent storage (`navigator.storage.persist`).
- [ ] **Full-screen output.** "Show output" on a projector or second screen dragged into place:
  the page goes full screen, keys and a clicker work, Escape closes the output, the pointer hides
  and "Close output" appears when it moves. Check browsers that refuse full screen (iPad Safari).
- [ ] **Lazy CJK fonts.** Open a song with Chinese or Korean text on a slow connection (DevTools
  throttling): blank placeholders and "Loading the fonts…" until the face arrives, then the slides
  redraw. The control window's own text (library titles) has no CJK font in the browser and shows
  boxes; feeding the fetched face to egui would fix that.
- [ ] **Folder remembered between visits.** Chromium can keep the folder handle (IndexedDB) and ask
  again for permission on the next visit; not done, so the folder is picked on every visit.

## Decisions waiting for the operator

- [ ] **Where the default style edit lives (#30).** It is kept in `.serviceflow/styles.sidecar` in
  the data root (shared by everyone on that folder, ADR-0003), not in this computer's preferences.
  Keep that, or make it per computer? "Re-import styles from OpenSong" drops the edit.
- [ ] **Scripture style (#30).** The editor edits the default style and style items only. Should the
  scripture style be editable too (scripture items are placeholders for now)?
- [ ] **Other style fields (#30).** Translation size and colour, chord size and colour, the subtitle's
  contents, outline thickness and the italic flip of translations have no controls yet. Which are
  wanted?

- [ ] **Change-plan confirmation (#20).** A save that renames tags only asks for confirmation when
  at least one set changes. If no set refers to the song it saves directly, as #19 does. Keep
  that, or always ask?
- [ ] **Unsaved set edits (#22).** Opening another set drops the open set's unsaved edits without
  asking (as before #22). Should it ask first, or keep the edits as a draft like songs? (A change
  plan that rewrites the open set, such as renaming a song it uses, keeps the edits: they stay on
  top of the rewritten file, the renamed song items included, and the set stays unsaved.)
- [ ] **Blank lines as slide breaks (#7, #8).** OpenSong's `blank_is_slide_change` is now taken
  from the imported PresentSettings. `SetSlideOptions` overrides it for the rest of the run, but
  no control sends it yet. Does the operator want a switch, and should it be remembered?
- [ ] **Duplicate vs save as (#22).** "Duplicate set…" copies the set as saved and leaves the open
  set (and its edits) open; "Save set as…" writes the edits under the new name and switches to
  it. A plan is shown only when the set has a sidecar to copy. Is that the split operators expect?
- [ ] **Transposition is now a set change (#31).** Language mode, transposition and capo of a set
  item are saved with the set ("Save set"), not at once, so a key chosen during a service is lost
  if the set is not saved. Opening another set drops them with the other unsaved edits. Also,
  "Save in this key" saves the song at once but puts the item back to zero only in the open set:
  if the set is then not saved, the item is transposed again on top of the new key next time.
  Should these settings save themselves (or ask) instead?
- [ ] **Renaming a set keeps its `name` (#31).** "Rename set…" moves the file as it is, so the
  `name` attribute inside it still holds the old name ("Save set as…" writes the new one). Check
  whether OpenSong shows the file name or the attribute.
- [ ] **Tidy library on a real library (#21).** Run "Tidy library…" on a copy of a large shared
  library (hundreds of songs, OpenSong running on it): check how long the dry run takes (every
  file is read), that OpenSong still opens the rewritten songs and sets, and that CRLF-only diffs
  are what a version-control diff shows.
- [ ] **English strings in core Problems.** Some `Problem` values and plan failures carry English
  text made in core or the runtime (storage error messages, "written but unreadable", "no free copy
  name"). They should become typed variants the app words with `tl!`.
- [ ] **Other `tl!` sentences built from pieces.** The `Problem` list now uses single `tl!`
  strings with `{name}` placeholders; about 40 other sites in the app (files, set panel, editor,
  web) still join several `tl!` fragments with `format!`.
