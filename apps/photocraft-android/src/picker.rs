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
    "pef", "orf", "rw2", "raf", "abr", "grd", "svg", "svgz", "aco", "ase", "kys", "af", "afdesign", "afphoto", "afpub",
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

/// The shell's dialog state: at most one picker at a time, plus where the next one starts.
pub struct Host {
    /// Shared storage (`/storage/emulated/0`).
    storage: PathBuf,
    last_dir: Option<PathBuf>,
    active: Option<Picker>,
}

impl Host {
    pub fn new(storage: PathBuf) -> Self {
        Self { storage, last_dir: None, active: None }
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
        let Some(picker) = self.active.as_mut() else { return };
        let mut answer: Option<Option<FileDialogAnswer>> = None;
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.92).min(760.0);
        let height = (screen.height() * 0.82).min(900.0);
        let storage = self.storage.clone();

        let modal = egui::Modal::new(egui::Id::new("android_file_picker")).show(ctx, |ui| {
            ui.set_width(width);
            let title = match picker.mode {
                Mode::Open { .. } => "Open",
                Mode::Folder => "Choose Folder",
                Mode::Save => "Save As",
            };
            ui.heading(title);
            ui.add_space(4.0);

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
                    ui.label("PhotoCraft needs \"All files access\" to open and save in your storage: Settings › Apps › PhotoCraft (unofficial) › Permissions, or Settings › Apps › Special app access › All files access.");
                    ui.add_space(list_height.min(120.0));
                }
                Ok(entries) => {
                    egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
                        if entries.is_empty() {
                            ui.add_space(8.0);
                            ui.label("Nothing to show here.");
                            if picker.dir.starts_with(&storage) {
                                ui.label("If your files should be here, allow \"All files access\": Settings › Apps › PhotoCraft (unofficial) › Permissions.");
                            }
                        }
                        for entry in entries {
                            let label = if entry.is_dir { format!("📁  {}", entry.name) } else { format!("🖼  {}", entry.name) };
                            let row = ui.add_sized([ui.available_width(), ROW_HEIGHT], egui::Button::new(label).frame(false).wrap_mode(egui::TextWrapMode::Truncate));
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
                    ui.colored_label(ui.visuals().warn_fg_color, format!("{} already exists. Tap Replace to save over it.", target.file_name().unwrap_or_default().to_string_lossy()));
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
}
