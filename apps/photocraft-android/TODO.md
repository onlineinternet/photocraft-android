# PhotoCraft Android: to-do

What's planned for the Android port, in rough order. Tick items off as they land; add new ones
where they fit. Done items move to the bottom with the date.

## Now

- [ ] Try two-finger zoom/pan/rotate again with no snapping: still jerky? (Next suspects: the
      canvas draws one frame behind the fingers, and the GPU canvas rounds its position to whole
      pixels.)
- [ ] Try importing a Lospec palette (.gpl, .pal or .hex) through File › Open or Swatches.
- [ ] Try File › Open and Save As through Android's pickers (Drive, Downloads, thumbnails),
      then File › Save on a file opened that way and Open Recent after restarting the app.
- [ ] Try the Open Photo edge key (Android's photo grid).

## Touch and layout (Clip Studio Paint style)

- [ ] A per-document "no rotation" setting, if the global Touch rotate toggle isn't enough.
- [ ] Phone layout: bottom toolbar, tool groups that pop out, dialogs as bottom sheets.
- [ ] Panels on phones and the tablet: undecided how they should work yet.
- [ ] Palm rejection beyond "Finger draws" off.
- [ ] OS clipboard and share sheet.
- [ ] Load system fonts for non-Latin scripts.
- [ ] Get rid of the one-frame flash of a dab before a two-finger gesture takes it back.

## Pixel art

- [ ] Phase 2, palette: replace an exact colour across the layer or document (palette-linked
      recolor), colour ramps, a shading ink that steps along a ramp, palette-only painting.
- [ ] Phase 3, animation: onion skin, frame tags, sprite-sheet export with JSON.
- [ ] Phase 4: tilemaps, 9-patch slices, tiled (wrap-around) painting.

## Community content

- Brushes: Photoshop `.abr` already imports; CSP `.sut` is out (store terms).
- Templates: Photoshop `.psdt` already opens.
- Tutorials: link out from Help (Lospec, Aseprite docs) rather than build any.

## Housekeeping

- Weekly upstream sync runs as a scheduled task; never push to storytold/photocraft.
- Only arm64-v8a is built.

## Done

- 2026-10-16: Files from File › Open keep a persistable URI: File › Save writes back and Open
  Recent reopens them (`Services::read_uri`, `content://` paths). Open Photo edge key (Android's
  photo picker). Swatches › Export Swatches as GIMP Palette…. Saved New Document presets keep
  Pixel Art Mode.
- 2026-10-16: Shift / Ctrl / Alt latching keys back on the edge keyboard (for Magic Wand and
  other click modifiers); Straighten key (`view.rotateView {"snap": 90}`).
- 2026-10-15: Two-finger zoom, pan and rotate together around the fingers, read from raw
  touches (egui's pinch never started once the shell dropped the mouse, which broke pinch zoom);
  no dead zone or snapping (quarter turns only in Pixel Art Mode), "Touch rotate" toggle.
- 2026-10-15: File › Open and Save As use Android's document and save pickers (one Java class,
  PhotoCraftActivity, built by javac + d8 in build-apk.sh); the built-in browser stays for
  folders and as the fallback.
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
