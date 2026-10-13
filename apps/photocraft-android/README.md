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
3. For pen pressure, check **Preferences › Tools › Use Tablet Pressure**, and set a brush's
   size or opacity to follow pen pressure in Brush Settings.

## How it works

- `src/lib.rs`: the entry point (`android_main`) and the shell around `PhotocraftApp`.
- `src/picker.rs`: Open / Choose Folder / Save As, drawn with egui over filesystem paths
  (rfd has no Android backend).
- `src/services.rs`: files, preferences, presets and crash recovery in app storage.
- `android/AndroidManifest.xml`: a NativeActivity app with no Java code.
- `android/build-apk.sh`: cargo builds the library; aapt2, zipalign and apksigner package it.

Pen pressure arrives as winit touch force, the same path Windows pens use.

## Known gaps

- No OS clipboard, share sheet, or Android document picker yet (cloud drives and USB storage
  that aren't mounted as paths don't show up).
- The interface is the desktop layout; a Bluetooth keyboard helps a lot.
- Fingers report a touch force too, so finger painting follows how hard the screen thinks you
  pressed. Turn off *Use Tablet Pressure* if that gets in the way.
- Pen tilt, barrel rotation and the S Pen button aren't read yet.
- Only arm64-v8a is built.
