# PhotoCraft Android: to-do

What's planned for the Android port, in rough order. Tick items off as they land; add new ones
where they fit. Done items move to the bottom with the date.

## Now

- [ ] Check the Pixel Art Mode grid fix on the tablet and phone: one checker square per empty
      pixel, pixel grid over every pixel (GPU canvas; the CPU fallback still draws the grid over
      painted pixels only).

## Touch and layout (Clip Studio Paint style)

- [ ] Two-finger canvas rotation (twist while pinching), with a snap back to 0°.
- [ ] Phone layout: bottom toolbar, tool groups that pop out, dialogs as bottom sheets.
- [ ] Panels on phones and the tablet: undecided how they should work yet.
- [ ] Palm rejection beyond "Finger draws" off.
- [ ] OS clipboard, share sheet and the Android document picker (cloud drives, USB storage).
- [ ] Load system fonts for non-Latin scripts.
- [ ] Get rid of the one-frame flash of a dab before a two-finger gesture takes it back.

## Pixel art

- [ ] Phase 2, palette: replace an exact colour across the layer or document (palette-linked
      recolor), colour ramps, a shading ink that steps along a ramp, palette-only painting.
- [ ] Import palettes as `.gpl` (GIMP), `.hex` and `.pal` (JASC), so every Lospec download works
      (`.ase` already imports through Swatches).
- [ ] Phase 3, animation: onion skin, frame tags, sprite-sheet export with JSON.
- [ ] Phase 4: tilemaps, 9-patch slices, tiled (wrap-around) painting.
- [ ] Saved New Document presets don't remember Pixel Art Mode yet.

## Community content

- Brushes: Photoshop `.abr` already imports; CSP `.sut` is out (store terms).
- Templates: Photoshop `.psdt` already opens.
- Tutorials: link out from Help (Lospec, Aseprite docs) rather than build any.

## Housekeeping

- Weekly upstream sync runs as a scheduled task; never push to storytold/photocraft.
- Only arm64-v8a is built.

## Done

- 2026-10-14: Pixel Art Mode phase 1: whole-pixel zoom, pixel-perfect pencil, Pixel Art New
  Document category, nearest-neighbour transforms, saved in `.pcraft`.
- 2026-10-14: Fingers paint at once; a quick second finger takes the dab back.
- Earlier: native APK built in CI, S Pen pressure/tilt/side button, edge keyboard, Apply/Cancel
  bar, two/three-finger undo/redo, phone-sized menus and dialogs, safe-area insets.
