# winit 0.30.13 (storytold fork), patched for PhotoCraft on Android

This is storytold's winit fork (https://github.com/storytold/winit, Apache-2.0, see `LICENSE`) at
rev `a517b3846ee8457d84ee4975110bfda6797c44dc`, the revision upstream PhotoCraft pins in its
`[patch.crates-io]`, packaged with `cargo package` (so it builds outside the fork's workspace). The
fork is winit 0.30.13 plus the fixes listed in the fork's `PATCHES.md`. It is used through
`[patch.crates-io]` in the workspace `Cargo.toml`, with one addition for the Android port
(`apps/photocraft-android`):

- `src/platform/android.rs`: a `pointer` module with `set_pointer_hook`, which receives a
  `PointerSample` for every pointer of every Android motion event: the tool (finger, stylus,
  eraser, mouse), pressure, tilt, orientation, the stylus barrel button, and hover.
- `src/platform_impl/android/mod.rs`: `emit_pointer_samples`, called at the top of the motion
  event handler.

Upstream winit 0.30 drops all of that when it turns Android pointers into touches, so the app
couldn't tell a pen from a finger. Nothing else changed: other platforms compile the same code as
the storytold fork. To update, when upstream PhotoCraft moves its `winit` pin, run
`cargo package --no-verify` in a checkout of the fork at the new rev, unpack the `.crate` over
this folder (keeping this file) and reapply the two additions (search for "PhotoCraft patch").
