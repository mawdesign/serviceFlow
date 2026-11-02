# Where OpenSong's behaviour is a bug, serviceFlow fixes it rather than copying it

Phase 1 aims for OpenSong parity, but a few behaviours in OpenSong's source are bugs or data-loss traps. serviceFlow deliberately behaves differently in these cases. They are recorded here so nobody later "restores parity" by reintroducing them.

- **`||` in multilanguage sections.** In OpenSong, a slide break on its own line, or after a primary line, merges slides and shifts the primary/translation pairing. This happens in real songs such as *How Deep the Father's Love*. serviceFlow treats `||` at any pair boundary as a clean slide break.
- **Legacy multilanguage detection.** OpenSong treats any tag longer than one character that ends in `L` (case-insensitive) as multilanguage, so `[Final]` would be caught. serviceFlow recognises only `(L)` and the strict `<type letter><digits>-L` / `<type letter><digits>L` forms.
- **Numbered verse blocks.** In OpenSong, any non-space first character on a lyric line starts a sub-section, so `Hello` under `[C]` becomes section `CH` with the text `ello`. In serviceFlow only digits 1–9 start a sub-section. Any other first character is read as a lyric line and the editor warns.
- **Sections not in the presentation order.** These are still excluded from the flow, as in OpenSong, but the operator can still send them live by hand.
