# Bundled fonts

Every font here is under the SIL Open Font License 1.1; the licence text next to it is checked by
`tests/fonts.rs` (`every_bundled_font_is_open_source_licensed`). Add a font to `BUNDLED` in
`src/fonts.rs` together with its licence file, and only if it is OFL or Apache 2.0.

| File | Family | Used for | Source |
|---|---|---|---|
| `LiberationSans-Regular.ttf` | Liberation Sans | default face; Arial, Helvetica | liberation-fonts 2.1.5 |
| `LiberationSerif-Regular.ttf` | Liberation Serif | Times New Roman, Times | liberation-fonts 2.1.5 |
| `Carlito-Regular.ttf` | Carlito | Calibri (same metrics) | googlefonts/carlito |
| `NotoSans-Regular.ttf` | Noto Sans | last resort; Latin, Greek, Cyrillic | notofonts/latin-greek-cyrillic |
| `NotoSansArabic-Regular.ttf` | Noto Sans Arabic | Arabic script | notofonts/arabic |
| `NotoSansHebrew-Regular.ttf` | Noto Sans Hebrew | Hebrew script | notofonts/hebrew |
| `NotoSansSC-Regular.otf` | Noto Sans SC | Chinese, Japanese, Korean | notofonts/noto-cjk `Sans/SubsetOTF/SC` |

| `NotoSansKR-Hangul-Subset.otf` | Noto Sans KR | Hangul (2,350 KS X 1001 syllables + jamo) | `Sans/SubsetOTF/KR`, cut with pyftsubset |

The two CJK faces are *lazy* (`lazy_bundled!`): the browser build does not compile them in but
serves them as `fonts/<file>` (copied by trunk, see `app/index.html`) and fetches one the first
time slide text needs it. A new large script face should be lazy too, with its coverage ranges.

Licence files: `LiberationSans-LICENSE.txt` (both Liberation faces), `Carlito-LICENSE.txt`,
`Noto-LICENSE.txt` (the OFL text; the CJK face's copyright holder is Adobe, same licence).
