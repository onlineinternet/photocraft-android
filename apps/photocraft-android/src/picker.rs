//! The Open, Choose Folder and Save As dialogs on Android.
//!
//! The desktop app asks the OS through rfd, which has no Android backend, and Android's own
//! document picker (the Storage Access Framework) answers through Java callbacks a NativeActivity
//! can't receive without a Java class of its own. So this shell draws its own picker with egui
//! over plain filesystem paths, which works once the app has Android's "All files access"
//! (`MANAGE_EXTERNAL_STORAGE`, granted by the user in Settings; see `android/AndroidManifest.xml`).
//! Without it, shared storage lists as empty or unreadable and the picker says how to allow it.
//!
//! Rows are tall and the buttons big because this is used with a finger or a pen.

use photocraft_ui_egui::{FileDialogAnswer, FileDialogReply, FileDialogRequest};
use std::path::{Path, PathBuf};

/// Everything File › Open reads (the desktop app's list, `apps/photocraft/src/services.rs`).
pub const OPEN_EXTS: &[&str] = &[
    "pcraft", "pdn", "ora", "kra", "krz", "psd", "psb", "psdt", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "tga", "ico", "qoi", "exr", "hdr",
    "pbm", "pgm", "ppm", "pam", "pfm", "pcx", "dds", "xpm", "fits", "fit", "fts", "dpx", "heic", "heif", "hif", "dng", "cr2", "cr3", "nef", "nrw", "arw",
    "pef", "orf", "rw2", "raf", "abr", "grd", "svg", "svgz", "aco", "ase", "gpl", "pal", "hex", "kys", "af", "afdesign", "afphoto", "afpub",
];

/// Shortcuts along the top of the picker, relative to shared storage.
const PLACES: &[(&str, &str)] = &[("Storage", ""), ("Pictures", "Pictures"), ("Documents", "Documents"), ("Download", "Download"), ("Camera", "DCIM/Camera")];

const ROW_HEIGHT: f32 = 44.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Files to read; an empty list shows every file.
    Open {
        extensions: Vec<String>,
    },
    Folder,
    Save,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Does `name` end in one of `extensions` (with or without the dot, any case)?
pub fn matches_ext(name: &str, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(e)))
}

/// The folders and matching files in `dir`, folders first, each group by name. Hidden entries
/// (a leading dot) are left out, as Android's own Files app does.
pub fn list(dir: &Path, mode: &Mode) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let is_dir = path.is_dir();
        if !is_dir {
            match mode {
                Mode::Folder => continue,
                Mode::Open { extensions } if !matches_ext(&name, extensions) => continue,
                _ => {}
            }
        }
        entries.push(Entry { name, path, is_dir });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

/// A file name the user typed, checked: not empty and no folder separators.
pub fn valid_name(name: &str) -> Option<&str> {
    let name = name.trim();
    (!name.is_empty() && !name.contains('/') && name != "." && name != "..").then_some(name)
}

/// Where a Save dialog starts and the name it suggests: the folder of a suggested path that
/// exists, else `fallback`; the suggestion's file name.
pub fn save_start(suggested: &str, fallback: &Path) -> (PathBuf, String) {
    let path = Path::new(suggested);
    let name = path.file_name().map_or_else(|| suggested.to_string(), |n| n.to_string_lossy().into_owned());
    let dir = path.parent().filter(|p| p.is_absolute() && p.is_dir()).map_or_else(|| fallback.to_path_buf(), Path::to_path_buf);
    (dir, name)
}

/// One open dialog.
struct Picker {
    mode: Mode,
    dir: PathBuf,
    entries: Result<Vec<Entry>, String>,
    /// The Save dialog's file name field.
    name: String,
    /// Save over this existing file? (asked once before answering)
    confirm_overwrite: Option<PathBuf>,
    reply: FileDialogReply,
}

impl Picker {
    fn go(&mut self, dir: PathBuf) {
        self.entries = list(&dir, &self.mode);
        self.dir = dir;
        self.confirm_overwrite = None;
    }
}

/// Where to find the switch by hand, for when the button can't open it.
const ACCESS_PATH: &str = "Settings › Apps › ⋮ › Special access › All files access › PhotoCraft (unofficial)";

/// "All files access", asked about at most once a second while a picker is open, so the list
/// fills in by itself when the user comes back from Settings with it turned on.
pub struct Access {
    /// Asks Android; `None` when it can't be asked (off Android).
    pub check: fn() -> Option<bool>,
    /// Opens the settings page with the switch.
    pub open_settings: fn() -> Result<(), String>,
    granted: Option<bool>,
    checked_at: Option<f64>,
    /// Why the settings page didn't open.
    error: Option<String>,
}

impl Default for Access {
    fn default() -> Self {
        Self::new(crate::android_system::has_all_files_access, crate::android_system::open_all_files_settings)
    }
}

impl Access {
    pub fn new(check: fn() -> Option<bool>, open_settings: fn() -> Result<(), String>) -> Self {
        Self { check, open_settings, granted: None, checked_at: None, error: None }
    }

    /// Re-ask at time `now` (seconds) if a second has passed. True when access was just granted.
    pub fn refresh(&mut self, now: f64) -> bool {
        if self.checked_at.is_some_and(|t| now - t < 1.0 && now >= t) {
            return false;
        }
        self.checked_at = Some(now);
        let was = self.granted;
        self.granted = (self.check)();
        if self.granted == Some(true) {
            self.error = None;
        }
        was == Some(false) && self.granted == Some(true)
    }

    /// Known to be missing (so the picker offers the button).
    pub fn missing(&self) -> bool {
        self.granted == Some(false)
    }
}

/// Android's own pickers (`android_system.rs`), swappable for tests.
pub struct System {
    pub open: fn(bool, &[&str]) -> Result<(), String>,
    pub create: fn(&str, &str) -> Result<(), String>,
    pub take: fn() -> Option<crate::android_system::Picked>,
}

impl Default for System {
    fn default() -> Self {
        use crate::android_system as a;
        Self { open: a::open_document, create: a::create_document, take: a::take_picked }
    }
}

/// A content:// file to save to, as a path the app can carry around: the URI, a slash and the
/// file's name, so the name (and its extension, which picks the format) reads like a path's.
pub fn saf_path(uri: &str, name: &str) -> String {
    let name: String = name.chars().map(|c| if c == '/' || c.is_control() { '_' } else { c }).collect();
    format!("{uri}/{}", if name.trim().is_empty() { "Untitled" } else { &name })
}

/// The URI inside a [`saf_path`], or `None` for a plain file path.
pub fn saf_uri(path: &str) -> Option<&str> {
    path.starts_with("content://").then(|| path.rsplit_once('/').map_or(path, |(uri, _)| uri))
}

/// A MIME type for saving `name` (Android's save picker wants one; unknown types are fine).
pub fn mime_for(name: &str) -> &'static str {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "psd" | "psb" => "image/vnd.adobe.photoshop",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" | "gpl" | "hex" | "pal" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// What the app gets for one of Android's picker answers.
pub fn answer_for(picked: crate::android_system::Picked, multiple: bool) -> Option<FileDialogAnswer> {
    use crate::android_system::Picked;
    match picked {
        Picked::Cancel => None,
        Picked::Save { uri, name } => Some(FileDialogAnswer::SaveTo(saf_path(&uri, &name))),
        Picked::Open { mut files, error } => {
            if files.is_empty() {
                return error.map(|e| FileDialogAnswer::Unavailable(format!("Couldn't open {e}")));
            }
            if multiple && files.len() > 1 {
                Some(FileDialogAnswer::ContentsMany(files))
            } else {
                let (name, bytes) = files.swap_remove(0);
                Some(FileDialogAnswer::Contents(name, bytes))
            }
        }
    }
}

/// The shell's dialog state: at most one picker at a time, plus where the next one starts.
pub struct Host {
    /// Shared storage (`/storage/emulated/0`).
    storage: PathBuf,
    last_dir: Option<PathBuf>,
    active: Option<Picker>,
    pub access: Access,
    pub system: System,
    /// Waiting for Android's picker to answer this (and whether it may answer several files).
    waiting: Option<(FileDialogReply, bool)>,
}

impl Host {
    pub fn new(storage: PathBuf) -> Self {
        Self { storage, last_dir: None, active: None, access: Access::default(), system: System::default(), waiting: None }
    }

    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }

    fn start_dir(&self, initial: Option<&str>) -> PathBuf {
        let pictures = self.storage.join("Pictures");
        initial
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| self.last_dir.clone().filter(|p| p.is_dir()))
            .or_else(|| pictures.is_dir().then_some(pictures))
            .unwrap_or_else(|| self.storage.clone())
    }

    /// Show a dialog for `request`. A dialog still open is cancelled (its reply is dropped).
    pub fn request(&mut self, request: FileDialogRequest, reply: FileDialogReply) {
        // Files to open and places to save go to Android's own pickers (thumbnails, Drive, no All
        // files access needed); this picker is the fallback, and still chooses folders.
        self.waiting = None;
        let system = match &request {
            FileDialogRequest::Open { multiple, .. } => (self.system.open)(*multiple, &[]).map(|()| *multiple),
            FileDialogRequest::Save { suggested } => {
                let name = Path::new(suggested).file_name().map_or_else(|| suggested.clone(), |n| n.to_string_lossy().into_owned());
                (self.system.create)(&name, mime_for(&name)).map(|()| false)
            }
            FileDialogRequest::Folder { .. } => Err("folders use the built-in picker".into()),
        };
        match system {
            Ok(multiple) => {
                self.active = None;
                self.waiting = Some((reply, multiple));
                return;
            }
            Err(e) => log::info!("Android's picker isn't available ({e}); using the built-in one"),
        }
        let (mode, dir, name) = match request {
            FileDialogRequest::Open { initial_dir, extensions, .. } => {
                let extensions = extensions.unwrap_or_else(|| OPEN_EXTS.iter().map(|e| (*e).to_string()).collect());
                (Mode::Open { extensions }, self.start_dir(initial_dir.as_deref()), String::new())
            }
            FileDialogRequest::Folder { initial_dir } => (Mode::Folder, self.start_dir(initial_dir.as_deref()), String::new()),
            FileDialogRequest::Save { suggested } => {
                let fallback = self.start_dir(None);
                let (dir, name) = save_start(&suggested, &fallback);
                (Mode::Save, dir, name)
            }
        };
        let entries = list(&dir, &mode);
        self.active = Some(Picker { mode, dir, entries, name, confirm_overwrite: None, reply });
    }

    /// Draw the open dialog (if any); answers it when the user chooses or cancels.
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.waiting.is_some() {
            match (self.system.take)() {
                Some(picked) => {
                    if let Some((reply, multiple)) = self.waiting.take() {
                        reply.send(answer_for(picked, multiple));
                    }
                }
                // The answer arrives from Java's side: look again shortly.
                None => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        let Some(picker) = self.active.as_mut() else { return };
        let mut answer: Option<Option<FileDialogAnswer>> = None;
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.92).min(760.0);
        let height = (screen.height() * 0.82).min(900.0);
        let storage = self.storage.clone();
        let now = ctx.input(|i| i.time);
        if self.access.refresh(now) {
            // Back from Settings with access: show what was hidden.
            let dir = picker.dir.clone();
            picker.go(dir);
        }
        if self.access.missing() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        let access = &mut self.access;

        let modal = egui::Modal::new(egui::Id::new("android_file_picker")).show(ctx, |ui| {
            ui.set_width(width);
            let title = match picker.mode {
                Mode::Open { .. } => "Open",
                Mode::Folder => "Choose Folder",
                Mode::Save => "Save As",
            };
            ui.heading(title);
            ui.add_space(4.0);
            if access.missing() {
                // Without All files access the lists come up empty: say so, and offer the switch.
                ui.horizontal_wrapped(|ui| {
                    ui.label("PhotoCraft can't see your files until you allow \"All files access\".");
                    if ui.add(egui::Button::new("Allow access").min_size(egui::vec2(0.0, 40.0))).clicked() {
                        access.error = (access.open_settings)().err().map(|e| format!("Couldn't open Settings ({e}). Turn it on at {ACCESS_PATH}."));
                    }
                });
                if let Some(e) = &access.error {
                    ui.colored_label(ui.visuals().warn_fg_color, e);
                }
                ui.add_space(4.0);
            }

            // Places, then the current folder with an Up button.
            ui.horizontal_wrapped(|ui| {
                for (label, rel) in PLACES {
                    let target = if rel.is_empty() { storage.clone() } else { storage.join(rel) };
                    if target.is_dir() && ui.add(egui::Button::new(*label).min_size(egui::vec2(0.0, 36.0))).clicked() {
                        picker.go(target);
                    }
                }
            });
            ui.horizontal(|ui| {
                let parent = picker.dir.parent().map(Path::to_path_buf);
                if ui.add_enabled(parent.is_some(), egui::Button::new("⬆ Up").min_size(egui::vec2(72.0, 36.0))).clicked()
                    && let Some(p) = parent
                {
                    picker.go(p);
                }
                ui.label(egui::RichText::new(picker.dir.display().to_string()).monospace());
            });
            ui.separator();

            let list_height = (height - 220.0).max(160.0);
            let mut tapped: Option<Entry> = None;
            match &picker.entries {
                Err(e) => {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().warn_fg_color, format!("Can't read this folder: {e}"));
                    ui.label(format!("PhotoCraft needs \"All files access\" to open and save in your storage: {ACCESS_PATH}."));
                    ui.add_space(list_height.min(120.0));
                }
                Ok(entries) => {
                    egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
                        if entries.is_empty() {
                            ui.add_space(8.0);
                            ui.label("Nothing to show here.");
                            if picker.dir.starts_with(&storage) {
                                ui.label(format!("If your files should be here, allow \"All files access\": {ACCESS_PATH}."));
                            }
                        }
                        for entry in entries {
                            let label = if entry.is_dir { format!("📁  {}", entry.name) } else { format!("🖼  {}", entry.name) };
                            let row =
                                ui.add_sized([ui.available_width(), ROW_HEIGHT], egui::Button::new(label).frame(false).wrap_mode(egui::TextWrapMode::Truncate));
                            if row.clicked() {
                                tapped = Some(entry.clone());
                            }
                        }
                    });
                }
            }
            if let Some(entry) = tapped {
                if entry.is_dir {
                    picker.go(entry.path);
                } else {
                    match picker.mode {
                        Mode::Open { .. } => answer = Some(Some(FileDialogAnswer::Paths(vec![entry.path.display().to_string()]))),
                        Mode::Save => {
                            picker.name = entry.name;
                            picker.confirm_overwrite = None;
                        }
                        Mode::Folder => {}
                    }
                }
            }
            ui.separator();

            if picker.mode == Mode::Save {
                ui.horizontal(|ui| {
                    ui.label("File name:");
                    let edit = ui.add(egui::TextEdit::singleline(&mut picker.name).desired_width(ui.available_width()).min_size(egui::vec2(0.0, 36.0)));
                    if edit.changed() {
                        picker.confirm_overwrite = None;
                    }
                });
                if let Some(target) = &picker.confirm_overwrite {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("{} already exists. Tap Replace to save over it.", target.file_name().unwrap_or_default().to_string_lossy()),
                    );
                }
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let big = egui::vec2(120.0, 40.0);
                if ui.add(egui::Button::new("Cancel").min_size(big)).clicked() {
                    answer = Some(None);
                }
                match picker.mode {
                    Mode::Folder => {
                        if ui.add_enabled(picker.entries.is_ok(), egui::Button::new("Use This Folder").min_size(big)).clicked() {
                            answer = Some(Some(FileDialogAnswer::FolderTo(picker.dir.display().to_string())));
                        }
                    }
                    Mode::Save => {
                        let target = valid_name(&picker.name).map(|n| picker.dir.join(n));
                        let replace = target.is_some() && picker.confirm_overwrite == target;
                        let label = if replace { "Replace" } else { "Save" };
                        if ui.add_enabled(target.is_some(), egui::Button::new(label).min_size(big)).clicked()
                            && let Some(target) = target
                        {
                            if target.exists() && !replace {
                                picker.confirm_overwrite = Some(target);
                            } else {
                                answer = Some(Some(FileDialogAnswer::SaveTo(target.display().to_string())));
                            }
                        }
                    }
                    Mode::Open { .. } => {
                        ui.label("Tap a file to open it.");
                    }
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(None);
        }
        if let Some(answer) = answer
            && let Some(picker) = self.active.take()
        {
            self.last_dir = Some(picker.dir.clone());
            picker.reply.send(answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("photocraft-android-picker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extensions_match_any_case_with_or_without_the_dot() {
        let exts = vec!["psd".to_string(), ".PNG".to_string()];
        assert!(matches_ext("a.PSD", &exts));
        assert!(matches_ext("b.png", &exts));
        assert!(!matches_ext("c.jpg", &exts));
        assert!(!matches_ext("noext", &exts));
        assert!(matches_ext("anything", &[]));
    }

    #[test]
    fn listing_puts_folders_first_hides_dotfiles_and_filters_by_mode() {
        let dir = temp("list");
        std::fs::create_dir(dir.join("zeta")).unwrap();
        std::fs::create_dir(dir.join("Alpha")).unwrap();
        std::fs::create_dir(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("b.psd"), b"x").unwrap();
        std::fs::write(dir.join("A.png"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();

        let open = Mode::Open { extensions: vec!["psd".into(), "png".into()] };
        let names: Vec<_> = list(&dir, &open).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Alpha", "zeta", "A.png", "b.psd"]);

        let folders: Vec<_> = list(&dir, &Mode::Folder).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(folders, ["Alpha", "zeta"]);

        let save: Vec<_> = list(&dir, &Mode::Save).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(save, ["Alpha", "zeta", "A.png", "b.psd", "notes.txt"]);

        assert!(list(&dir.join("missing"), &Mode::Folder).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_with_separators_or_nothing_are_refused() {
        assert_eq!(valid_name(" art.psd "), Some("art.psd"));
        assert_eq!(valid_name(""), None);
        assert_eq!(valid_name("   "), None);
        assert_eq!(valid_name("a/b.psd"), None);
        assert_eq!(valid_name(".."), None);
    }

    #[test]
    fn save_starts_beside_an_existing_document_or_in_the_fallback() {
        let dir = temp("save");
        let doc = dir.join("art.psd");
        assert_eq!(save_start(&doc.display().to_string(), Path::new("/fallback")), (dir.clone(), "art.psd".to_string()));
        assert_eq!(save_start("Untitled-1.psd", Path::new("/fallback")), (PathBuf::from("/fallback"), "Untitled-1.psd".to_string()));
        assert_eq!(save_start("/no/such/dir/x.png", Path::new("/fallback")), (PathBuf::from("/fallback"), "x.png".to_string()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    static GRANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    fn check() -> Option<bool> {
        Some(GRANTED.load(std::sync::atomic::Ordering::SeqCst))
    }

    #[test]
    fn access_is_rechecked_each_second_and_reports_when_granted() {
        let mut a = Access::new(check, || Ok(()));
        assert!(!a.missing(), "unknown until asked");
        assert!(!a.refresh(0.0));
        assert!(a.missing());
        GRANTED.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!a.refresh(0.5), "not asked again within a second");
        assert!(a.missing());
        assert!(a.refresh(1.2), "granted while away in Settings");
        assert!(!a.missing());
        assert!(!a.refresh(2.5), "only the change is reported");
        // Off Android nothing can be asked, so no button.
        let mut off = Access::new(|| None, || Err("not running on Android".into()));
        off.refresh(0.0);
        assert!(!off.missing());
    }

    #[test]
    fn android_picker_answers_become_the_apps() {
        use crate::android_system::Picked;
        assert_eq!(answer_for(Picked::Cancel, false), None);
        let one = Picked::Open { files: vec![("a.png".into(), vec![1])], error: None };
        assert_eq!(answer_for(one, true), Some(FileDialogAnswer::Contents("a.png".into(), vec![1])));
        let two = Picked::Open { files: vec![("a.png".into(), vec![1]), ("b.psd".into(), vec![2])], error: None };
        assert!(matches!(answer_for(two.clone(), true), Some(FileDialogAnswer::ContentsMany(f)) if f.len() == 2));
        assert!(matches!(answer_for(two, false), Some(FileDialogAnswer::Contents(n, _)) if n == "a.png"));
        let failed = Picked::Open { files: vec![], error: Some("big.psd: too large".into()) };
        assert!(matches!(answer_for(failed, false), Some(FileDialogAnswer::Unavailable(m)) if m.contains("big.psd")));
        assert_eq!(answer_for(Picked::Open { files: vec![], error: None }, false), None);
        let save = Picked::Save { uri: "content://docs/document/42".into(), name: "Art.psd".into() };
        let Some(FileDialogAnswer::SaveTo(path)) = answer_for(save, false) else { panic!("a place to save") };
        assert_eq!(path, "content://docs/document/42/Art.psd");
        assert_eq!(Path::new(&path).extension().and_then(|e| e.to_str()), Some("psd"), "the format comes from the name");
        assert_eq!(saf_uri(&path), Some("content://docs/document/42"));
        assert_eq!(saf_uri("/storage/emulated/0/a.png"), None);
        assert_eq!(saf_path("content://x", "a/b"), "content://x/a_b");
        assert_eq!(saf_path("content://x", " "), "content://x/Untitled");
        assert_eq!(mime_for("x.PNG"), "image/png");
        assert_eq!(mime_for("x.kra"), "application/octet-stream");
    }
}
