# PhotoCraft Android: to-do

What's planned for the Android port, in rough order. Tick items off as they land; add new ones
where they fit. Done items move to the bottom with the date.

## Now

- [ ] Try the 2026-10-22 fixes on a phone and the tablet: a quick finger swipe paints the whole
      line; number boxes bring up the number pad and refuse letters; Help › Search menus in
      landscape leaves the menu visible; About and Settings › Apps show 0.10.0; menus and
      tooltips show no shortcuts until a keyboard is attached (and show them once one is).
- [ ] Drop the All files access permission (Google Play is strict about it): Open Recent and the
      built-in file browser would rely on picker files only.
- [ ] Big photos: a 24 MP photo takes about 25 s to open (tester's emulator).
- [ ] Ask about JPEG quality when saving with File › Save (Export As already has it).
- [ ] Try two-finger zoom/pan/rotate again with no snapping: still jerky? (Next suspects: the
      canvas draws one frame behind the fingers, and the GPU canvas rounds its position to whole
      pixels.)
- [ ] Try importing a Lospec palette (.gpl, .pal or .hex) through File › Open or Swatches.
- [ ] Try File › Open and Save As through Android's pickers (Drive, Downloads, thumbnails),
      then File › Save on a file opened that way and Open Recent after restarting the app.
- [ ] Try the Open Photo and Share edge keys, and Copy/Paste to and from another app.
- [ ] Try the palette (Swatches panel in Pixel Art Mode): Recolor, Ramp, the Pencil's Ink and
      Palette only.

## Touch and layout (Clip Studio Paint style)

- [ ] A per-document "no rotation" setting, if the global Touch rotate toggle isn't enough.
- [ ] Phone layout: bottom toolbar, tool groups that pop out, dialogs as bottom sheets.
- [ ] Panels on phones and the tablet: undecided how they should work yet.
- [ ] Palm rejection beyond "Finger draws" off.
- [ ] Load system fonts for non-Latin scripts.
- [ ] Get rid of the one-frame flash of a dab before a two-finger gesture takes it back.

## Pixel art

- [ ] Palette: drag to reorder entries; edit an entry with the colour picker; indexed-colour
      export (PNG-8 / GIF with exactly the palette).
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

- 2026-10-22: From a tester's report: quick strokes that arrived whole in one frame painted
  nothing or a dot (the shell now holds the release back a frame, `split_stroke_end`); number
  fields show the number pad and take only number characters; the keyboard no longer fills a
  landscape screen (PhotoCraftActivity's KeyboardTarget view); the APK's version follows the
  workspace (it said 0.8.2); keyboard shortcuts and key hints hidden unless a hardware keyboard
  is attached (`shortcuts::key_hints`); status bar text no longer runs under the job progress.
- 2026-10-16: Pixel art phase 2: the document palette (`Document::palette`, `palette.*`
  commands, saved in `.pcraft`), palette-linked Recolor, Ramp, From Layer / From Swatches, and
  the Pencil's shading ink (`"shade"`) and Palette only (`"paletteLock"`).
- 2026-10-16: Android clipboard (copies out as PNG through ImageProvider, images paste in) and
  a Share edge key (flattened PNG to the share sheet).
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
