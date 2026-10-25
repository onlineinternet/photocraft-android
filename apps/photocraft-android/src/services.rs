//! Android platform services: files through plain paths and this shell's own picker
//! ([`crate::picker`]), preferences and crash recovery in the app's private storage.
//!
//! Mirrors the desktop services (`apps/photocraft/src/services.rs`) without the parts Android
//! has no counterpart for here: rfd dialogs, the OS clipboard, the beep, the control server.

use crate::picker;
use photocraft_doc::Document;
use photocraft_format::RecoveryStore;
use photocraft_ui_egui::{Recoverable, Services};
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// The picker shared by the `file_dialog` service (which opens it) and the shell (which draws it).
pub type SharedPicker = Rc<RefCell<picker::Host>>;

/// Where the app keeps its own files (preferences, presets, recovery, window layout).
#[derive(Clone, Debug)]
pub struct Dirs {
    /// The app's private files directory (`/data/user/0/<package>/files`).
    pub data: PathBuf,
}

impl Dirs {
    pub fn prefs_file(&self) -> PathBuf {
        self.data.join("preferences.json")
    }
    pub fn presets_dir(&self) -> PathBuf {
        self.data.join("Presets")
    }
    pub fn recovery_dir(&self) -> PathBuf {
        self.data.join("Recovery")
    }
    pub fn ui_state_file(&self) -> PathBuf {
        self.data.join("ui.ron")
    }
    /// The Android shell's own touch settings ([`crate::pen::TouchSettings`]).
    pub fn touch_file(&self) -> PathBuf {
        self.data.join("touch.json")
    }
}

/// Run `f`; a panic inside it becomes an `Err` naming `what` (the desktop's crash guard).
pub fn guard<T>(what: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(format!("{what} failed with an internal error (logged); your open documents are unchanged")))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // A place Android's save picker chose (picker.rs `saf_path`): written through its provider.
    if let Some(uri) = path.to_str().and_then(crate::picker::saf_uri) {
        return crate::android_system::write_uri(uri, bytes);
    }
    photocraft_format::atomic_write(path, bytes).map_err(|e| e.to_string())
}

fn export_options(settings: &photocraft_ui_egui::ExportSettings) -> photocraft_io::ExportOptions {
    let mut opts = photocraft_io::ExportOptions::default();
    if let Some(q) = settings.jpeg_quality {
        opts.encode.jpeg_quality = q;
    }
    opts.encode.webp_lossless = settings.webp_lossless;
    if let Some(q) = settings.webp_quality {
        opts.encode.webp_quality = q;
    }
    opts.tiff_layers = settings.tiff_layers;
    opts.xmp = if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None };
    opts.tga_bits = settings.tga_bits;
    opts.image_previews = settings.image_previews;
    opts.psd_maximize_compatibility = settings.psd_maximize_compatibility;
    opts
}

/// The share of a save's progress bar that encoding fills; writing fills the rest.
const ENCODE_SHARE: f32 = 0.85;

/// A save on a worker thread (`Services::save_file`): encode, then write crash-safely.
fn save_file(
    doc: &Document,
    path: &str,
    settings: &photocraft_ui_egui::ExportSettings,
    ctl: &photocraft_ui_egui::jobs_ui::SaveCtl,
) -> Result<Vec<String>, String> {
    ctl.progress(0.0, "Encoding");
    let opts = export_options(settings);
    let cancelled = || ctl.check().is_err();
    let progress = |f: f32| ctl.progress(f * ENCODE_SHARE, "");
    let interrupt = photocraft_raster::Interrupt::new(&cancelled, &progress);
    let r = guard("Save", || photocraft_io::export_with(doc, path, &opts, &interrupt).map_err(|e| e.to_string()))?;
    ctl.commit()?;
    ctl.progress(ENCODE_SHARE, "Writing");
    if crate::picker::saf_uri(path).is_some() {
        write_atomic(Path::new(path), &r.bytes)?;
    } else {
        photocraft_format::atomic_write_progress(Path::new(path), &r.bytes, &|f| ctl.progress(ENCODE_SHARE + (0.99 - ENCODE_SHARE) * f, ""))
            .map_err(|e| e.to_string())?;
    }
    ctl.progress(1.0, "");
    Ok(r.warnings)
}

type SharedRecovery = Rc<RefCell<Option<RecoveryStore>>>;

fn with_store<R>(store: &SharedRecovery, f: impl FnOnce(&mut RecoveryStore) -> R) -> Result<R, String> {
    let mut slot = store.try_borrow_mut().map_err(|_| "crash recovery is busy".to_string())?;
    Ok(f(slot.as_mut().ok_or("no app storage")?))
}

/// The platform services for the Android shell.
pub fn android(dirs: &Dirs, picker: SharedPicker) -> Services {
    let recover_dir = dirs.recovery_dir();
    let store: SharedRecovery = Rc::new(RefCell::new(Some(RecoveryStore::new(recover_dir.clone()))));
    let (s1, s2, s3) = (store.clone(), store.clone(), store.clone());
    let prefs_load = dirs.prefs_file();
    let prefs_save = dirs.prefs_file();
    Services {
        import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
            guard("Open", || photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))
        })),
        export: Some(Box::new(|doc: &Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
            let opts = export_options(settings);
            guard("Export", || photocraft_io::export(doc, path, &opts).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string()))
        })),
        save_file: Some(Arc::new(save_file)),
        file_dialog: Some(Box::new(move |request, _parent, reply| picker.borrow_mut().request(request, reply))),
        write: Some(Box::new(|path: &str, bytes: &[u8]| write_atomic(Path::new(path), bytes))),
        // Copy and Paste reach Android's clipboard: copies go out as PNG, images copied in other
        // apps (or a screenshot's Copy) paste in.
        clipboard_set_image: Some(Box::new(|w: u32, h: u32, rgba: &[u8]| {
            let img = photocraft_codecs::Image::from_u8(w, h, photocraft_codecs::ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
            let png =
                photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &photocraft_codecs::EncodeOptions::default()).map_err(|e| e.to_string())?;
            crate::android_system::set_clipboard_image(&png)
        })),
        // No image on the clipboard is `Ok(None)`; one that can't be decoded is an error, which
        // leaves PhotoCraft's own copy alone.
        clipboard_get_image: Some(Box::new(|| {
            let Some(bytes) = crate::android_system::clipboard_image() else { return Ok(None) };
            let img = guard("Paste", || photocraft_codecs::decode(&bytes).map_err(|e| e.to_string()))?;
            Ok(Some((img.width(), img.height(), img.to_rgba8())))
        })),
        // Files opened through Android's picker reopen by their URI (Open Recent).
        read_uri: Some(Box::new(|path: &str| match crate::picker::saf_uri(path) {
            Some(uri) => crate::android_system::read_uri(uri),
            None => Err("not a file Android's picker opened".into()),
        })),
        encode_png: Some(Box::new(|w, h, rgba| {
            let img = photocraft_codecs::Image::from_u8(w, h, photocraft_codecs::ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
            photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &photocraft_codecs::EncodeOptions::default()).map_err(|e| e.to_string())
        })),
        load_prefs: Some(Box::new(move || std::fs::read_to_string(&prefs_load).ok())),
        save_prefs: Some(Box::new(move |text: &str| write_atomic(&prefs_save, text.as_bytes()))),
        append_text: Some(Box::new(|path: &str, text: &str| {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
            f.write_all(text.as_bytes()).map_err(|e| e.to_string())
        })),
        // Crash recovery matters more here than on a desktop: Android ends background apps.
        autosave: Some(Box::new(move |doc: &Arc<Document>, revision: u64, path: Option<&str>| {
            with_store(&s1, |s| s.autosave_checked(doc, revision, path.map(str::to_string)))?
        })),
        autosave_results: Some(Box::new(move || with_store(&s3, |s| s.take_completed()).unwrap_or_default())),
        discard_autosave: Some(Box::new(move |id: u64| {
            let _ = with_store(&s2, |s| s.discard(id));
        })),
        recover: Some(Box::new(move || {
            photocraft_format::list_recovery(&recover_dir)
                .into_iter()
                .map(|e| Recoverable {
                    key: e.info.key.clone(),
                    name: e.info.document_name.clone(),
                    path: e.info.original_path.clone(),
                    load: Box::new(move || photocraft_format::recover(&e).map_err(|e| e.to_string())),
                })
                .collect()
        })),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| {
            let _ = with_store(&store, |s| s.adopt(id, key));
        })),
        preset_store: Some(photocraft_engine::preset_store::open_dir_async(dirs.presets_dir())),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_turns_a_panic_into_an_error() {
        let r: Result<(), String> = guard("Open", || panic!("boom"));
        assert!(r.unwrap_err().contains("Open failed"));
        assert_eq!(guard("Open", || Ok(3)), Ok(3));
    }

    #[test]
    fn preferences_round_trip_through_the_data_dir() {
        let data = std::env::temp_dir().join(format!("photocraft-android-services-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        let dirs = Dirs { data: data.clone() };
        let picker: SharedPicker = Rc::new(RefCell::new(picker::Host::new(data.clone())));
        let mut services = android(&dirs, picker);
        assert_eq!((services.load_prefs.as_mut().unwrap())(), None);
        (services.save_prefs.as_mut().unwrap())("{\"a\":1}").unwrap();
        assert_eq!((services.load_prefs.as_mut().unwrap())().as_deref(), Some("{\"a\":1}"));
        std::fs::remove_dir_all(data).unwrap();
    }
}
