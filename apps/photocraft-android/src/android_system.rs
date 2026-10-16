//! The Android system calls the shell makes through JNI:
//!
//! - whether the app has "All files access" (`Environment.isExternalStorageManager()`), and
//! - opening Android's settings page where the user turns it on, straight at this app's switch
//!   (`Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION`), so nobody has to dig for it under
//!   Special app access;
//! - Android's own document, photo and "save to" pickers, through the app's one Java class
//!   (`android/java/.../PhotoCraftActivity.java`), which receives their answers; the shell polls
//!   for them with [`take_picked`] and writes saves with [`write_uri`].
//!
//! The Java VM comes from `android-activity`, which sets up the same `jni` crate's VM singleton
//! before `android_main` runs. The activity object is registered by `android_main` ([`init`]).
//! On other platforms (host tests) nothing is registered and every call reports that it can't run.

use jni::objects::{JByteArray, JObject, JObjectArray, JString, JValue};
use jni::{Env, JavaVM, jni_sig, jni_str};
use std::sync::OnceLock;

/// `ANativeActivity::clazz`, the activity's global JNI reference, as an address.
static ACTIVITY: OnceLock<usize> = OnceLock::new();

/// Remember the activity (`AndroidApp::activity_as_ptr`). Called once from `android_main`.
pub fn init(activity: *mut std::ffi::c_void) {
    if !activity.is_null() {
        let _ = ACTIVITY.set(activity as usize);
    }
}

/// Run `f` with a JNI environment and the activity.
fn with_activity<T>(f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>) -> Result<T, String> {
    let activity = *ACTIVITY.get().ok_or("not running on Android")?;
    let vm = JavaVM::singleton().map_err(|e| format!("no Java VM: {e}"))?;
    vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        // SAFETY: `activity` is NativeActivity's `clazz`, a global reference that stays valid while
        // the app runs (android-activity keeps it for the activity's lifetime and this shell only
        // runs inside it). Wrapping it in a plain `JObject` takes no ownership: nothing deletes
        // it, and it is only used inside this call.
        #[allow(unsafe_code)]
        let activity = unsafe { JObject::from_raw(env, activity as jni::sys::jobject) };
        f(env, &activity)
    })
    .map_err(|e| e.to_string())
}

/// Whether the app may read and write all of shared storage. `None` when it can't be asked
/// (not on Android, or the call failed).
pub fn has_all_files_access() -> Option<bool> {
    with_activity(|env, _| env.call_static_method(jni_str!("android/os/Environment"), jni_str!("isExternalStorageManager"), jni_sig!("()Z"), &[])?.z()).ok()
}

/// Open the settings page with this app's "All files access" switch (or, if a device lacks that
/// page, the list of all apps with that access).
pub fn open_all_files_settings() -> Result<(), String> {
    with_activity(|env, activity| {
        let start = |env: &mut Env, action: &str, package: Option<&JObject>| -> jni::errors::Result<()> {
            let action = env.new_string(action)?;
            let intent = match package {
                Some(name) => {
                    let scheme = env.new_string("package")?;
                    let none = JObject::null();
                    let uri = env
                        .call_static_method(
                            jni_str!("android/net/Uri"),
                            jni_str!("fromParts"),
                            jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;"),
                            &[JValue::Object(&scheme), JValue::Object(name), JValue::Object(&none)],
                        )?
                        .l()?;
                    env.new_object(
                        jni_str!("android/content/Intent"),
                        jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
                        &[JValue::Object(&action), JValue::Object(&uri)],
                    )?
                }
                None => env.new_object(jni_str!("android/content/Intent"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&action)])?,
            };
            env.call_method(activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)])?;
            Ok(())
        };
        let package = env.call_method(activity, jni_str!("getPackageName"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
        match start(env, "android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION", Some(&package)) {
            Ok(()) => Ok(()),
            // No per-app page on this device (the call threw): the list of apps instead.
            Err(_) => {
                env.exception_clear();
                start(env, "android.settings.MANAGE_ALL_FILES_ACCESS_PERMISSION", None)
            }
        }
    })
}

/// A file Android's picker handed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub uri: Option<String>,
}

/// What one of Android's pickers answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// Files chosen to open: name, contents, and the URI to reopen and save it by (`None` when
    /// Android only lends the file, as the photo picker does); `error` names one that couldn't be
    /// read.
    Open {
        files: Vec<PickedFile>,
        error: Option<String>,
    },
    /// Where to save: a `content://` URI, and the name the file got.
    Save {
        uri: String,
        name: String,
    },
    Cancel,
}

/// Android's document picker for files to open, filtered to `mime_types` (all files if empty).
pub fn open_document(multiple: bool, mime_types: &[&str]) -> Result<(), String> {
    with_activity(|env, activity| {
        let empty = env.new_string("")?;
        let n = i32::try_from(mime_types.len()).unwrap_or(0);
        let mimes = env.new_object_array(n, jni_str!("java/lang/String"), &empty)?;
        for (i, m) in mime_types.iter().take(usize::try_from(n).unwrap_or(0)).enumerate() {
            let m = env.new_string(m)?;
            mimes.set_element(env, i, &m)?;
        }
        env.call_method(
            activity,
            jni_str!("openDocument"),
            jni_sig!("(Z[Ljava/lang/String;)V"),
            &[JValue::Bool(jni::sys::jboolean::from(multiple)), JValue::Object(&mimes)],
        )?;
        Ok(())
    })
}

/// Android's photo picker (the document picker for images before Android 13).
pub fn pick_photos(multiple: bool) -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(activity, jni_str!("pickPhotos"), jni_sig!("(Z)V"), &[JValue::Bool(jni::sys::jboolean::from(multiple))])?;
        Ok(())
    })
}

/// Android's "save to" picker, suggesting `name`.
pub fn create_document(name: &str, mime_type: &str) -> Result<(), String> {
    with_activity(|env, activity| {
        let name = env.new_string(name)?;
        let mime = env.new_string(mime_type)?;
        env.call_method(
            activity,
            jni_str!("createDocument"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
            &[JValue::Object(&name), JValue::Object(&mime)],
        )?;
        Ok(())
    })
}

fn string_at(env: &mut Env, array: &JObjectArray, i: usize) -> jni::errors::Result<Option<String>> {
    let o = array.get_element(env, i)?;
    if o.is_null() {
        return Ok(None);
    }
    let s = env.cast_local::<JString>(o)?;
    Ok(Some(s.try_to_string(env)?))
}

/// The pickers' answer, once it has arrived (each answer is taken once).
pub fn take_picked() -> Option<Picked> {
    let r = with_activity(|env, activity| -> jni::errors::Result<Option<Picked>> {
        let o = env.call_method(activity, jni_str!("takeAnswer"), jni_sig!("()[Ljava/lang/Object;"), &[])?.l()?;
        if o.is_null() {
            return Ok(None);
        }
        let a = env.cast_local::<JObjectArray>(o)?;
        let kind = string_at(env, &a, 0)?.unwrap_or_default();
        Ok(Some(match kind.as_str() {
            "save" => match (string_at(env, &a, 1)?, string_at(env, &a, 2)?) {
                (Some(uri), name) => Picked::Save { uri, name: name.unwrap_or_else(|| "Untitled".into()) },
                (None, _) => Picked::Cancel,
            },
            "open" => {
                let names = a.get_element(env, 1)?;
                let names = env.cast_local::<JObjectArray>(names)?;
                let datas = a.get_element(env, 2)?;
                let datas = env.cast_local::<JObjectArray>(datas)?;
                let error = string_at(env, &a, 3)?;
                let kept = a.get_element(env, 4)?;
                let kept = if kept.is_null() { None } else { Some(env.cast_local::<JObjectArray>(kept)?) };
                let n = if names.is_null() || datas.is_null() { 0 } else { names.len(env)?.min(datas.len(env)?) };
                let mut files = Vec::with_capacity(n.min(256));
                for i in 0..n {
                    let name = string_at(env, &names, i)?.unwrap_or_else(|| "Untitled".into());
                    let bytes = datas.get_element(env, i)?;
                    let bytes = env.cast_local::<JByteArray>(bytes)?;
                    let uri = match &kept {
                        Some(k) if i < k.len(env)? => string_at(env, k, i)?,
                        _ => None,
                    };
                    if !bytes.is_null() {
                        files.push(PickedFile { name, bytes: env.convert_byte_array(&bytes)?, uri });
                    }
                }
                Picked::Open { files, error }
            }
            _ => Picked::Cancel,
        }))
    });
    match r {
        Ok(p) => p,
        Err(e) => {
            log::warn!("reading the picker's answer: {e}");
            None
        }
    }
}

/// Read a file by a `content://` URI kept from Android's picker (Open Recent).
pub fn read_uri(uri: &str) -> Result<Vec<u8>, String> {
    with_activity(|env, activity| {
        let uri = env.new_string(uri)?;
        let data = env.call_method(activity, jni_str!("readUri"), jni_sig!("(Ljava/lang/String;)[B"), &[JValue::Object(&uri)])?.l()?;
        let data = env.cast_local::<JByteArray>(data)?;
        if data.is_null() {
            return Ok(Vec::new());
        }
        env.convert_byte_array(&data)
    })
    .map_err(|e| if e.contains("Exception") { "can't be read any more (moved, deleted or access was taken back)".to_string() } else { e })
}

/// Write `bytes` to a `content://` URI the save picker answered.
pub fn write_uri(uri: &str, bytes: &[u8]) -> Result<(), String> {
    let r = with_activity(|env, activity| {
        let uri = env.new_string(uri)?;
        let data = env.byte_array_from_slice(bytes)?;
        let err = env
            .call_method(activity, jni_str!("writeUri"), jni_sig!("(Ljava/lang/String;[B)Ljava/lang/String;"), &[JValue::Object(&uri), JValue::Object(&data)])?
            .l()?;
        if err.is_null() {
            return Ok(None);
        }
        let err = env.cast_local::<JString>(err)?;
        Ok(Some(err.try_to_string(env)?))
    })?;
    match r {
        None => Ok(()),
        Some(e) => Err(format!("couldn't save: {e}")),
    }
}

/// A Java method's "null on success, else why not" answer.
fn error_of<'a>(env: &mut Env<'a>, answer: JObject<'a>) -> jni::errors::Result<Option<String>> {
    if answer.is_null() {
        return Ok(None);
    }
    let s = env.cast_local::<JString>(answer)?;
    Ok(Some(s.try_to_string(env)?))
}

/// Open Android's share sheet with a PNG called `name`.
pub fn share_image(png: &[u8], name: &str) -> Result<(), String> {
    let why = with_activity(|env, activity| {
        let data = env.byte_array_from_slice(png)?;
        let name = env.new_string(name)?;
        let r = env
            .call_method(
                activity,
                jni_str!("shareImage"),
                jni_sig!("([BLjava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&data), JValue::Object(&name)],
            )?
            .l()?;
        error_of(env, r)
    })?;
    why.map_or(Ok(()), |e| Err(format!("couldn't share: {e}")))
}

/// Put a PNG on Android's clipboard.
pub fn set_clipboard_image(png: &[u8]) -> Result<(), String> {
    let why = with_activity(|env, activity| {
        let data = env.byte_array_from_slice(png)?;
        let r = env.call_method(activity, jni_str!("setClipboardImage"), jni_sig!("([B)Ljava/lang/String;"), &[JValue::Object(&data)])?.l()?;
        error_of(env, r)
    })?;
    why.map_or(Ok(()), Err)
}

/// The image on Android's clipboard, still encoded (PNG, JPEG, …), if there is one.
pub fn clipboard_image() -> Option<Vec<u8>> {
    with_activity(|env, activity| {
        let r = env.call_method(activity, jni_str!("clipboardImage"), jni_sig!("()[B"), &[])?.l()?;
        if r.is_null() {
            return Ok(None);
        }
        let bytes = env.cast_local::<JByteArray>(r)?;
        Ok(Some(env.convert_byte_array(&bytes)?))
    })
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_android_the_calls_report_they_cant_run() {
        assert_eq!(has_all_files_access(), None);
        assert!(open_all_files_settings().is_err());
        assert!(open_document(true, &["image/png"]).is_err());
        assert!(pick_photos(false).is_err());
        assert!(create_document("a.png", "image/png").is_err());
        assert_eq!(take_picked(), None);
        assert!(read_uri("content://x").is_err());
        assert!(share_image(b"png", "a.png").is_err());
        assert!(set_clipboard_image(b"png").is_err());
        assert_eq!(clipboard_image(), None);
        assert!(write_uri("content://x", b"data").is_err());
        init(std::ptr::null_mut());
        assert_eq!(has_all_files_access(), None, "a null activity is never registered");
    }
}
