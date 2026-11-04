# TODO

Things we could not settle while building Phase 1. Each item names the ticket where it came up
(`#n` is the GitHub issue). Tick an item off and, when it changes behaviour, record the answer in
`GLOSSARY.md` or an ADR (ADR-0007 for deliberate differences from OpenSong).

## To confirm in OpenSong

Run the real OpenSong (or read its source, notes in `.sf-notes/opensong.md`) and compare.

- [ ] **Image item titles (#15).** Do image items show their title or subtitle over the picture? We
  show the picture alone. Also check what `descriptions_in_subtitle` does.
- [ ] **Per-item slide options (#15, #10, #16).** What do `seconds`, `loop`, `transition` and `link`
  on a custom or image item (`slide_group`) do? We read and ignore them. Check the per-item
  `@transition` override as well.
- [ ] **Image paths outside `Backgrounds/` (#15).** We treat `../x` and similar as missing. Does
  OpenSong follow them?
- [ ] **Song style when `song_style_preferred` is off (#16).** We follow the source: the song's own
  style still applies when no style item is in effect. Confirm in the running app.
- [ ] **Blank between set items (#10).** `blanks` and `blank_uses_next` are stored but not used
  between items. What exactly does OpenSong show, and in which style?
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
- [ ] **Moving past the last slide of an item (#14, #10).** We carry on to the next set item, with a
  blank first when `blanks` is on. Confirm the order of those two.
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

### Data and files
- [ ] **Canonical save on a real library (#19).** Copy a real data root, save a few edited songs,
  and open them in OpenSong. Check nothing is lost (comments, capo attributes, odd elements).
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
- [ ] **Sidecars (#23).** Language mode is restored after reopening a set. Rename a set and check
  what happens to its sidecar (it is not moved yet).
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
- [ ] **Browser build (#26).** The bundled fonts add about 11 MB (8 MB of it Noto Sans SC). Load
  the CJK fonts lazily in the browser.

## Decisions waiting for the operator

- [ ] **Change-plan confirmation (#20).** A save that renames tags only asks for confirmation when
  at least one set changes. If no set refers to the song it saves directly, as #19 does. Keep
  that, or always ask?
- [ ] **Unsaved set edits (#22).** Opening another set drops the open set's unsaved edits without
  asking (as before #22). Should it ask first, or keep the edits as a draft like songs?
- [ ] **Duplicate vs save as (#22).** "Duplicate set…" copies the set as saved and leaves the open
  set (and its edits) open; "Save set as…" writes the edits under the new name and switches to
  it. A plan is shown only when the set has a sidecar to copy. Is that the split operators expect?
