# PhotoCraft on Android (unofficial)

An unofficial community port: the same PhotoCraft app as the desktop and web builds, as a
native Android APK for arm64 tablets and phones. Not affiliated with or supported by the
ArtCraft team, and it carries none of the ArtCraft branding.

## Build

**On GitHub:** run the **Android APK** workflow (Actions tab, or push to the `android`
branch). The APK is attached to the run under *Artifacts*.

**On Linux:** install Rust's `aarch64-linux-android` target, the Android NDK, and the SDK's
`build-tools` and `platforms;android-35`, then from the workspace root:

```sh
ANDROID_HOME=~/Android/Sdk ANDROID_NDK_HOME=~/Android/Sdk/ndk/27.2.12479018 \
  apps/photocraft-android/android/build-apk.sh
```

The APK lands in `target/android/photocraft-android-arm64.apk`.

## Install and first run

1. Copy the APK to the device and open it (allow installing from that app when asked).
2. Grant file access: open File › Open and tap **Allow access**, which goes straight to
   PhotoCraft's "All files access" switch; turn it on and come back, and the list fills in. By
   hand it's *Settings › Apps › ⋮ › Special access › All files access* (it isn't on the app's
   Permissions page). Without it, File › Open and Save As can't see your storage.
3. Fonts: the Type tool lists Android's system fonts. To add your own, put `.ttf` / `.otf`
   files in a `Fonts` folder at the top of internal storage and restart the app.
4. For pen pressure, check **Preferences › Tools › Use Tablet Pressure**, and set a brush's
   size or opacity to follow pen pressure in Brush Settings.

## Touch features

- **Edge keyboard:** tap the `‹` tab on the right edge for one-tap shortcuts: Undo, Redo,
  Copy, Paste, Select All, Deselect, Inverse, Transform, brush Size −/+, Swap colors, New Layer,
  and **Panels** (hide the right dock), **Hide UI** (everything but the canvas), **Search**
  (run any menu command by name) and **Pixel Art** (Pixel Art Mode, below).
- **Apply / Cancel** buttons appear at the bottom while Free Transform or a crop is waiting,
  and a tap has to move 10 points before it drags the transform box, so a tap inside it doesn't
  nudge it and a tap outside applies it.
- **Pen and finger are told apart** (a pointer hook in the vendored winit,
  `third_party/winit/PATCHED.md`): the S Pen paints with pressure and tilt and starts at once;
  its side button holds Alt (Alt + tap picks a color); hovering moves the brush outline.
- **Finger draws** (edge keyboard toggle): on, a finger paints, with **Finger pressure**
  simulated from speed; off, one finger pans the canvas and two pinch to zoom, so only the pen
  draws. A tap outside the edge keyboard closes it.
- **Two-finger tap** undoes, **three-finger tap** redoes.
- **Two fingers** pinch to zoom, move to pan and twist to rotate, all at once and around the
  point between them, so the picture stays under your fingers. Small twists while pinching are
  ignored (8°), and letting go within 4° of 0°, 90°, 180° or 270° snaps to it; View › Reset View
  straightens it. In Pixel Art Mode it only turns in 90° steps and zooms by whole pixels. The
  **Touch rotate** edge key turns twisting off (pinch and pan still work).
- On screens narrower than 1000 points (phones), the right dock starts hidden, menus that don't
  fit the menu bar move into a **»** menu at its end, and dialogs fit the screen.
- Dragging a document tab never tears it off into a floating window (a floated one goes back with
  Window › Arrange › Consolidate All to Tabs).
- The interface stays clear of the status and navigation bars, and the Home screen has big
  stacked buttons with no keyboard shortcuts.
- The font list leaves out the Noto fonts Android ships for other writing systems (Myanmar,
  Ogham, …); fonts in your own `Fonts` folder are always listed.

## Pixel Art Mode

View › Pixel Art Mode (or the **Pixel Art** edge key) switches the open document to pixel work
and back at any time; it's saved with the document in `.pcraft` and is one undo step. While it's
on:

- every empty pixel is one checker square, lined up with the pixels, and the pixel grid covers
  every pixel (empty ones too), so you can always tell where each pixel is;
- the zoom is always a whole number of screen pixels per pixel (pinch and the wheel step through
  those levels, and Fit on Screen picks the largest that fits, above 100 % too), so every pixel is
  the same size and sharp;
- turning it on shows the pixel grid, turns anti-aliasing off, picks a 1 px Pencil without
  smoothing and brings up the Swatches panel;
- the 1 px Pencil is **pixel-perfect**: a freehand line drops the extra pixel at each L-shaped
  corner, like Aseprite (`paint.pencil` `"pixelPerfect"`);
- Free Transform starts with Nearest Neighbor interpolation.

Palettes from Lospec and other pixel-art tools open straight into Swatches: `.gpl` (GIMP),
`.pal` (JASC), `.hex` and `.ase` (File › Open, or Import Swatches in the Swatches panel menu).

File › New has a **Pixel Art** category (16×16, 32×32, 64×64, 320×180 on a transparent canvas)
that starts in the mode, and a **Pixel Art Mode** checkbox for any size.

## How it works

- `src/lib.rs`: the entry point (`android_main`) and the shell around `PhotocraftApp`.
- `src/picker.rs`: Open / Choose Folder / Save As, drawn with egui over filesystem paths
  (rfd has no Android backend).
- `src/android_system.rs`: the few Android calls made through JNI (All files access check and
  its settings page).
- `src/services.rs`: files, preferences, presets and crash recovery in app storage.
- `src/edge_keys.rs`: the edge keyboard.
- `src/pen.rs`: pen or finger, the stylus feed, finger pressure, navigating fingers.
- `src/gestures.rs`: lets a finger paint at once; if a second finger lands within 0.3 s, ends
  the stroke and takes it back (the gesture's accident); recognizes the undo/redo taps.
- `android/AndroidManifest.xml`: a NativeActivity app with no Java code.
- `android/build-apk.sh`: cargo builds the library; aapt2, zipalign and apksigner package it.

Pen samples reach the canvas through the same stylus feed the desktop tablet code uses.

## Known gaps

The plan and open work are in [TODO.md](TODO.md).


- No OS clipboard, share sheet, or Android document picker yet (cloud drives and USB storage
  that aren't mounted as paths don't show up).
- The interface is the desktop layout; panels are still desktop-sized.
- Text in non-Latin scripts may not render, since their system fonts aren't loaded.
- Starting a two-finger gesture can flash the first finger's dab for a frame before it's taken
  back.
- No palm rejection yet beyond "Finger draws" off.
- Only arm64-v8a is built.
