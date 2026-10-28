# PhotoCraft Android: to-do

What's planned for the Android port, in rough order. Tick items off as they land; add new ones
where they fit. Done items move to the bottom with the date.

## Now

- [ ] Try on a phone (upright and sideways): the Color Picker, Preferences, Layer Style and a
      filter dialog fit the screen; File › Save and Save As to a JPEG ask for the quality; a
      24 MP photo opens faster than before (time it); File › Open, Save and Open Recent still
      work with no All files access.
- [ ] Preferences on a phone: fits, but its right half needs a sideways scroll; a one-column
      layout would be better.
- [ ] Opening a big photo: profile the first frame after it opens on a device (about 0.6 s on a
      fast PC with the CPU canvas).
- [ ] Try the 2026-10-22 fixes on a phone and the tablet: a quick finger swipe paints the whole
      line; number boxes bring up the number pad and refuse letters; Help › Search menus in
      landscape leaves the menu visible; About and Settings › Apps show 0.10.0; menus and
      tooltips show no shortcuts until a keyboard is attached (and show them once one is).
- [ ] Try two-finger zoom/pan/rotate again with no snapping: still jerky? (Next suspects: the
      canvas draws one frame behind the fingers, and the GPU canvas rounds its position to whole
      pixels.)
- [ ] Try importing a Lospec palette (.gpl, .pal or .hex) through File › Open or Swatches.
- [ ] Try File › Open and Save As through Android's pickers (Drive, Downloads, thumbnails),
      then File › Save on a file opened that way and Open Recent after restarting the app.
- [ ] Try Open Photo and Share in Quick access, and Copy/Paste to and from another app.
- [ ] Try the palette (Swatches panel in Pixel Art Mode): Recolor, Ramp, the Pencil's Ink and
      Palette only.

## Touch and layout (Clip Studio Paint style)

- [ ] A per-document "no rotation" setting, if the global Touch rotate toggle isn't enough.
- [ ] Compare the two touch layouts on the phone and tablet: Grok's ("CSP chrome test 2") and
      the simpler one (branch `csp-simple`), then keep one.
- [ ] Translate the touch layout's new labels (none are in the 20 catalogs yet).
- [ ] Dialogs as bottom sheets. They fit a phone today; they are still floating windows.
- [x] Phone and tablet layout (2026-10-27): command bar, left tool rail, sub-tool list, bottom
      colour/brush strip, and Layers / colour wheel / tool-properties palettes. Docked on a
      landscape tablet, sheets on a phone. See `studio_chrome`. The right-edge tab is not drawn;
      its actions are labeled buttons, and the selected tool shows its own options (selection
      mode, constrain, sample) above the bottom strip. Slider values sit off the track.
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

- 2026-10-27: Clip Studio touch chrome (`crates/ui-egui/src/studio_chrome.rs`), on for the Android
  shell. Studio dark theme the first time the stock dark theme is still selected. Long-press
  eyedropper on a brush tool (a finger only, not the pen). Simpler pass (branch `csp-simple`):
  ten rail groups with the rest under More tools, a Quick access palette for the edge keyboard's
  shortcuts and the touch switches, Tool properties with only the tool, the option row only for
  the Shift/Alt stand-ins, a selection bar (Deselect, Invert, Transform, Copy, Clear), no stub rows.
- 2026-10-22: Every window fits a phone: on the touch app a dialog's body scrolls inside a window
  capped at the screen (dialogs.rs), floating windows go through `widgets::fit_window`, the
  Color Picker stacks under its field when narrow; `phone_fit_tests::every_window_fits_a_phone`
  (--ignored, about 2 min) opens every menu command at 360×740 and 740×360. All files access
  removed (manifest, picker, JNI). JPEG Options on Save / Save As (since
  replaced by upstream's own JPEG Options, which every platform asks). Opening a photo converts it into tiles in parallel bands (io flat.rs) with a faster
  8-bit RGB→RGBA copy (codecs): 255 → 50 ms for 24 MP on 2 cores.
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
