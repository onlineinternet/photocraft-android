//! The few Android system calls the shell makes through JNI, with no Java code of its own:
//!
//! - whether the app has "All files access" (`Environment.isExternalStorageManager()`), and
//! - opening Android's settings page where the user turns it on, straight at this app's switch
//!   (`Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION`), so nobody has to dig for it under
//!   Special app access.
//!
//! The Java VM comes from `android-activity`, which sets up the same `jni` crate's VM singleton
//! before `android_main` runs. The activity object is registered by `android_main` ([`init`]).
//! On other platforms (host tests) nothing is registered and every call reports that it can't run.

use jni::objects::{JObject, JValue};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_android_the_calls_report_they_cant_run() {
        assert_eq!(has_all_files_access(), None);
        assert!(open_all_files_settings().is_err());
        init(std::ptr::null_mut());
        assert_eq!(has_all_files_access(), None, "a null activity is never registered");
    }
}
