# Slides are drawn by serviceFlow's own renderer, not by the GUI toolkit's text

Projected output is laid out and drawn by a dedicated renderer that does not depend on the GUI toolkit:

- HarfRust for text shaping.
- `craft-text` for bidirectional text and line breaking.
- `vello_cpu` for drawing.

The GUI shows the finished frames as textures.

egui's own text painter cannot do proper outlines or shadows, auto-scaling text to fit, or OpenSong's styling of translation lines, and projected lyrics need all of these.

## Consequences

- The same renderer produces output frames, control-window thumbnails and the editor preview, and later stage displays and the OBS overlay. All of these therefore always match.
- Rendering on the CPU behaves the same in the browser as on desktop, and does not depend on GPU support on old church laptops.
- Layout is tested through snapshots of glyph positions rather than pixels.
