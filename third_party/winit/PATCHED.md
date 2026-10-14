# winit 0.30.13, patched for PhotoCraft on Android

This is winit 0.30.13 exactly as published on crates.io (Apache-2.0, see `LICENSE`), used through
`[patch.crates-io]` in the workspace `Cargo.toml`, with one addition for the Android port
(`apps/photocraft-android`):

- `src/platform/android.rs`: a `pointer` module with `set_pointer_hook`, which receives a
  `PointerSample` for every pointer of every Android motion event: the tool (finger, stylus,
  eraser, mouse), pressure, tilt, orientation, the stylus barrel button, and hover.
- `src/platform_impl/android/mod.rs`: `emit_pointer_samples`, called at the top of the motion
  event handler.

Upstream winit 0.30 drops all of that when it turns Android pointers into touches, so the app
couldn't tell a pen from a finger. Nothing else changed: other platforms compile the same code as
upstream. To update, copy a newer winit 0.30.x over this folder and reapply the two additions
(search for "PhotoCraft patch").
