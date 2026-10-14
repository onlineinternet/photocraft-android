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
2. Grant file access: **Settings › Apps › PhotoCraft (unofficial) › Permissions › Files and
   media › Allow management of all files** (on Samsung, also reachable from *Settings › Apps ›
   Special access › All files access*). Without it, File › Open and Save As can't see your
   storage.
3. Fonts: the Type tool lists Android's system fonts. To add your own, put `.ttf` / `.otf`
   files in a `Fonts` folder at the top of internal storage and restart the app.
4. For pen pressure, check **Preferences › Tools › Use Tablet Pressure**, and set a brush's
   size or opacity to follow pen pressure in Brush Settings.

## Touch features

- **Edge keyboard:** tap the `‹` tab on the right edge. Ctrl, Shift, Alt and Space latch (tap
  once to hold, again to release), so Alt + pen picks a color, Space + drag pans, Shift draws
  straight lines. The tab lights up while a key is latched. Below them: Undo/Redo, Brush,
  Eraser, Picker, Move, Lasso, brush size, swap colors, Transform, Deselect, and
  **Panels** (hide the right dock), **Hide UI** (everything but the canvas) and **Search** (run
  any menu command by name, handy when the menu bar doesn't fit).
- **Apply / Cancel** buttons appear at the bottom while Free Transform or a crop is waiting,
  and a tap has to move 10 points before it drags the transform box, so a tap inside it doesn't
  nudge it and a tap outside applies it. The edge keyboard also has **Enter** and **Esc**.
- **Two-finger tap** undoes, **three-finger tap** redoes. Pinch zooms and two fingers pan.
- On screens narrower than 1000 points (phones), the right dock starts hidden.

## How it works

- `src/lib.rs`: the entry point (`android_main`) and the shell around `PhotocraftApp`.
- `src/picker.rs`: Open / Choose Folder / Save As, drawn with egui over filesystem paths
  (rfd has no Android backend).
- `src/services.rs`: files, preferences, presets and crash recovery in app storage.
- `src/edge_keys.rs`: the edge keyboard.
- `src/gestures.rs`: holds a lone finger's mouse events back for 80 ms, so a second finger
  turns them into a gesture instead of a dab; recognizes the undo/redo taps.
- `android/AndroidManifest.xml`: a NativeActivity app with no Java code.
- `android/build-apk.sh`: cargo builds the library; aapt2, zipalign and apksigner package it.

Pen pressure arrives as winit touch force, the same path Windows pens use.

## Known gaps

- No OS clipboard, share sheet, or Android document picker yet (cloud drives and USB storage
  that aren't mounted as paths don't show up).
- The interface is the desktop layout; on a phone the menu bar runs off the screen (use Search).
- Pen and finger can't be told apart yet (winit drops Android's tool type), so there's no
  "fingers navigate, pen draws" mode, and every touch starts 80 ms late.
- Fingers report a touch force too, so finger painting follows how hard the screen thinks you
  pressed. Turn off *Use Tablet Pressure* if that gets in the way.
- Pen tilt, barrel rotation and the S Pen button aren't read yet.
- Only arm64-v8a is built.
