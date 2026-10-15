# PhotoCraft Android: to-do

What's planned for the Android port, in rough order. Tick items off as they land; add new ones
where they fit. Done items move to the bottom with the date.

## Now

- [ ] Try two-finger rotation on the tablet and phone: dead zone, snapping, the Touch rotate
      toggle, quarter turns in Pixel Art Mode.
- [ ] Try importing a Lospec palette (.gpl, .pal or .hex) through File › Open or Swatches.
- [ ] Try the Allow access button in the Open dialog (turn All files access off first).

## Touch and layout (Clip Studio Paint style)

- [ ] Rotate around the point between the fingers rather than the canvas centre.
- [ ] A per-document "no rotation" setting, if the global Touch rotate toggle isn't enough.
- [ ] Phone layout: bottom toolbar, tool groups that pop out, dialogs as bottom sheets.
- [ ] Panels on phones and the tablet: undecided how they should work yet.
- [ ] Palm rejection beyond "Finger draws" off.
- [ ] Android photo picker and document picker (cloud drives, USB storage, no All files access
      needed): needs a small Java class in the APK and a Java build step in CI.
- [ ] Thumbnails in the built-in file picker (decode small previews in the background).
- [ ] OS clipboard and share sheet.
- [ ] Load system fonts for non-Latin scripts.
- [ ] Get rid of the one-frame flash of a dab before a two-finger gesture takes it back.

## Pixel art

- [ ] Phase 2, palette: replace an exact colour across the layer or document (palette-linked
      recolor), colour ramps, a shading ink that steps along a ramp, palette-only painting.
- [ ] A Swatches menu item to export `.gpl`/`.hex`/`.pal` (the `swatches.export` command
      already can).
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

- 2026-10-15: Two-finger canvas rotation (12° dead zone, snaps within 8° of quarter turns,
  quarter turns only in Pixel Art Mode, "Touch rotate" edge-key toggle).
- 2026-10-15: "Allow access" button in the file picker opens Android's All files access switch;
  the list fills in when you come back.
- 2026-10-15: Lospec palettes: `.gpl`, `.pal` and `.hex` import into Swatches; `swatches.export` writes them.
- 2026-10-14: Pixel Art Mode checkerboard on whole pixels and pixel grid over every pixel
  (checked on the devices).
- 2026-10-14: Pixel Art Mode phase 1: whole-pixel zoom, pixel-perfect pencil, Pixel Art New
  Document category, nearest-neighbour transforms, saved in `.pcraft`.
- 2026-10-14: Fingers paint at once; a quick second finger takes the dab back.
- Earlier: native APK built in CI, S Pen pressure/tilt/side button, edge keyboard, Apply/Cancel
  bar, two/three-finger undo/redo, phone-sized menus and dialogs, safe-area insets.
