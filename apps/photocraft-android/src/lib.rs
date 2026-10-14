//! PhotoCraft on Android: an unofficial community port.
//!
//! Runs the same [`photocraft_ui_egui::PhotocraftApp`] as the desktop and web builds, through
//! eframe on a NativeActivity (no Java code) with wgpu on Vulkan. Android loads
//! `libphotocraft_android.so` and calls [`android_main`]; everything else here is plain Rust that
//! also builds and tests on a desktop host.
//!
//! Differences from the desktop app:
//! - File › Open, Save As and folder choices use this shell's own touch-friendly picker over
//!   filesystem paths ([`picker`]), which needs Android's "All files access" permission;
//! - preferences, brush presets, crash recovery and the window layout live in the app's private
//!   storage ([`services::Dirs`]);
//! - an edge keyboard with latched Ctrl/Shift/Alt/Space and one-tap commands ([`edge_keys`]),
//!   and two- and three-finger taps for Undo and Redo ([`gestures`]);
//! - Apply / Cancel buttons while a transform or crop is pending ([`apply_bar`]), and a drag
//!   threshold so a tap doesn't nudge the transform box ([`TOUCH_DRAG_SLOP`]);
//! - the interface stays inside the area the status and navigation bars leave ([`insets`]);
//! - the right-hand dock starts hidden on narrow screens ([`NARROW_WIDTH`]);
//! - no OS clipboard, control server, beep or screen color picker yet.
//!
//! Pen pressure needs nothing extra: winit reports every Android pointer's pressure as a touch
//! force, which the canvas reads exactly like a Windows pen (`photocraft_ui_egui::stylus`).

pub mod apply_bar;
pub mod edge_keys;
pub mod gestures;
pub mod insets;
pub mod picker;
pub mod services;

use photocraft_doc::Document;
use photocraft_engine::Session;
use photocraft_ui_egui::PhotocraftApp;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// Shared storage on Android: what the Files app calls "Internal storage".
pub const SHARED_STORAGE: &str = "/storage/emulated/0";

/// Narrower than this (points), the right-hand dock starts hidden: a phone, or a tablet in
/// portrait split screen. The edge keyboard's Panels key brings it back.
pub const NARROW_WIDTH: f32 = 1000.0;

/// How far a press moves (points) before Free Transform takes it for a drag.
pub const TOUCH_DRAG_SLOP: f32 = 10.0;

/// The active document as a touch session started: what a gesture undoes back to first.
struct Snapshot {
    index: usize,
    doc: Arc<Document>,
}

/// A recognized tap, run once the app has seen the frame it ended in.
struct PendingTap {
    tap: gestures::Tap,
    frame: u64,
    snapshot: Option<Snapshot>,
}

/// The app plus the touch-screen pieces drawn on top: the file picker and the edge keyboard.
pub struct AndroidShell {
    app: PhotocraftApp,
    picker: services::SharedPicker,
    edge: edge_keys::EdgeKeys,
    gate: gestures::Gate,
    snapshot: Option<Snapshot>,
    pending_tap: Option<PendingTap>,
    frame: u64,
    layout_checked: bool,
    /// The activity's visible area, for the safe-area insets (`None` off Android).
    content_rect: Option<insets::ContentRectFn>,
}

fn snapshot(app: &PhotocraftApp) -> Option<Snapshot> {
    let index = app.session.active_index()?;
    Some(Snapshot { index, doc: app.session.active()?.doc.clone() })
}

/// Has the active document changed since `snapshot`?
fn changed_since(app: &PhotocraftApp, snapshot: &Snapshot) -> bool {
    app.session.active_index() == Some(snapshot.index) && app.session.active().is_some_and(|d| !Arc::ptr_eq(&d.doc, &snapshot.doc))
}

/// Undo (two fingers) or redo (three). If the gesture's first finger got through as a mouse
/// before the second landed (a dab, a deselect), that is undone first and dropped from Redo.
fn run_tap(app: &mut PhotocraftApp, ctx: &egui::Context, tap: gestures::Tap, snapshot: Option<Snapshot>) {
    if let Some(s) = &snapshot {
        let mut stray = false;
        for _ in 0..4 {
            if !changed_since(app, s) || !app.session.undo() {
                break;
            }
            stray = true;
        }
        if stray && let Some(d) = app.session.active_mut() {
            d.history.clear_redo();
        }
    }
    let id = match tap {
        gestures::Tap::Two => "edit.undo",
        gestures::Tap::Three => "edit.redo",
    };
    if let Err(e) = photocraft_ui_egui::menus::invoke(app, ctx, id, serde_json::json!({})) {
        log::info!("gesture {id}: {e}");
    }
}

impl eframe::App for AndroidShell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frame += 1;
        self.edge.sync(&mut self.app);
        if !self.layout_checked {
            self.layout_checked = true;
            if ctx.content_rect().width() < NARROW_WIDTH {
                self.app.ui.panels.dock = false;
            }
        }
        if self.pending_tap.as_ref().is_some_and(|p| self.frame > p.frame)
            && let Some(p) = self.pending_tap.take()
        {
            run_tap(&mut self.app, ctx, p.tap, p.snapshot);
        }
        self.edge.run_queued(&mut self.app, ctx);
        self.app.logic(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Keep the interface out from under the status and navigation bars.
        if let Some(read) = &self.content_rect
            && let Some(content) = read()
            && let Some(screen) = raw_input.screen_rect.or_else(|| Some(ctx.viewport_rect()))
        {
            let ppp = ctx.pixels_per_point();
            if let Some(m) = insets::insets(content, [screen.width() * ppp, screen.height() * ppp], ppp) {
                raw_input.safe_area_insets = Some(egui::SafeAreaInsets(m));
            }
        }
        let now = raw_input.time.unwrap_or(0.0);
        let filtered = self.gate.filter(std::mem::take(&mut raw_input.events), now);
        raw_input.events = filtered.events;
        if filtered.session_started {
            self.snapshot = snapshot(&self.app);
        }
        if let Some(tap) = filtered.tap {
            // `frame` is still the last frame's number here: wait for the one after this.
            self.pending_tap = Some(PendingTap { tap, frame: self.frame + 1, snapshot: self.snapshot.take() });
            ctx.request_repaint();
        }
        if filtered.holding {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(gestures::HOLD_SECS));
        }
        self.edge.raw_input(raw_input);
        self.app.raw_input_hook(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The bars' strips get the panel color; the app lays out in what's left.
        let visible = ctx.content_rect();
        ui.painter().rect_filled(ctx.viewport_rect(), 0.0, ui.visuals().panel_fill);
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(visible));
        inner.set_clip_rect(visible);
        self.app.ui(&mut inner, frame);
        if !self.picker.borrow().is_open() {
            apply_bar::show(&ctx, &mut self.app, &mut self.edge);
            self.edge.show(&ctx, &mut self.app);
        }
        self.picker.borrow_mut().show(&ctx);
    }
}

/// eframe's options for Android (`android_app` is set by the caller on a device).
pub fn native_options(dirs: &services::Dirs) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("PhotoCraft"),
        persistence_path: Some(dirs.ui_state_file()),
        ..Default::default()
    };
    // The adapter's real texture limits (egui asks for 8192 px), so big documents stay on the GPU.
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut options.wgpu_options.wgpu_setup);
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        // Vulkan first; GLES only for devices without a usable Vulkan driver.
        create.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL;
    }
    options
}

/// Build the app once eframe has a window and a GPU device.
pub fn create_app(cc: &eframe::CreationContext<'_>, dirs: &services::Dirs, storage: PathBuf, content_rect: Option<insets::ContentRectFn>) -> AndroidShell {
    let picker: services::SharedPicker = Rc::new(RefCell::new(picker::Host::new(storage)));
    let mut app = PhotocraftApp::new(Session::new(), services::android(dirs, picker.clone()));
    // Long commands and file opens run as background jobs with progress and Cancel.
    app.background_jobs = true;
    // A finger tap wanders a few points: not enough to nudge or rotate a Free Transform box.
    app.touch_drag_slop = TOUCH_DRAG_SLOP;
    // Touch first: no keyboard shortcuts on the Home screen's buttons, finger-sized controls.
    app.touch_ui = true;
    if let Some(rs) = cc.wgpu_render_state.clone() {
        let info = rs.adapter.get_info();
        log::info!("photocraft-android: wgpu {:?} on {} ({:?})", info.backend, info.name, info.device_type);
        app.perf.gpu_info.set_adapter(&info);
        let wants_gpu = app.session.prefs().performance.effective_rendering_mode() != photocraft_engine::prefs::RenderingMode::Cpu;
        if info.device_type != eframe::wgpu::DeviceType::Cpu && wants_gpu {
            app.set_wgpu(rs);
        } else {
            // The window still draws with wgpu: record its errors instead of panicking.
            let _ = photocraft_ui_egui::gpu_canvas::DeviceHealth::watch(&rs.device);
        }
    }
    AndroidShell {
        app,
        picker,
        edge: edge_keys::EdgeKeys::default(),
        gate: gestures::Gate::default(),
        snapshot: None,
        pending_tap: None,
        frame: 0,
        layout_checked: false,
        content_rect,
    }
}

/// Log every panic before the default hook runs (Android's logcat is the only console).
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("PhotoCraft internal error: {info}");
        default(info);
    }));
}

/// Run the app: on Android with the activity's [`AndroidApp`](winit::platform::android::activity::AndroidApp).
pub fn run(options: eframe::NativeOptions, dirs: services::Dirs, storage: PathBuf, content_rect: Option<insets::ContentRectFn>) -> eframe::Result {
    install_panic_hook();
    if let Err(e) = std::fs::create_dir_all(&dirs.data) {
        log::warn!("couldn't create {}: {e}", dirs.data.display());
    }
    eframe::run_native("PhotoCraft", options, Box::new(move |cc| Ok(Box::new(create_app(cc, &dirs, storage, content_rect)))))
}

/// The entry point Android's NativeActivity calls (through android-activity's glue) on its own
/// thread once the activity starts.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("photocraft"));
    let data = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp/photocraft"));
    let dirs = services::Dirs { data };
    let mut options = native_options(&dirs);
    let activity = app.clone();
    let content_rect: insets::ContentRectFn = Box::new(move || {
        let r = activity.content_rect();
        Some([r.left, r.top, r.right, r.bottom])
    });
    options.android_app = Some(app);
    if let Err(e) = run(options, dirs, PathBuf::from(SHARED_STORAGE), Some(content_rect)) {
        log::error!("PhotoCraft couldn't start: {e}");
    }
}
