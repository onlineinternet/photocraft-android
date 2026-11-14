// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Where an AI Pixel Studio key is kept. On a device the activity seals it with the Android
//! Keystore. When that call cannot run (a host test, or a build with no Java VM), the same key
//! is a file in the app's private directory, mode 0600. The bytes are never logged.

use photocraft_aipixel::secrets::SecretStore;
use std::path::{Path, PathBuf};

pub struct Store {
    pub dir: PathBuf,
}

impl SecretStore for Store {
    fn load(&self, name: &str) -> Result<Option<String>, String> {
        match crate::android_system::secret_get(name) {
            Ok(value) => Ok(value),
            Err(err) if jni_unavailable(&err) => file_load(&self.dir, name),
            Err(err) => Err(err),
        }
    }

    fn store(&self, name: &str, value: &str) -> Result<(), String> {
        match crate::android_system::secret_put(name, value.as_bytes()) {
            Ok(()) => Ok(()),
            Err(err) if jni_unavailable(&err) => file_store(&self.dir, name, value),
            Err(err) => Err(err),
        }
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        match crate::android_system::secret_clear(name) {
            Ok(()) => Ok(()),
            Err(err) if jni_unavailable(&err) => file_remove(&self.dir, name),
            Err(err) => Err(err),
        }
    }
}

fn jni_unavailable(err: &str) -> bool {
    err.contains("not running on Android") || err.contains("no Java VM")
}

fn path_for(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let ok = !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if !ok {
        return Err("that secret can't be stored".into());
    }
    Ok(dir.join(name))
}

fn file_load(dir: &Path, name: &str) -> Result<Option<String>, String> {
    let path = path_for(dir, name)?;
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() > 512 => Err("the saved key isn't text".into()),
        Ok(bytes) => {
            let text = String::from_utf8(bytes).map_err(|_| "the saved key isn't text".to_string())?;
            if text.is_empty() { Ok(None) } else { Ok(Some(text)) }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("couldn't read the saved key ({err})")),
    }
}

fn file_store(dir: &Path, name: &str, value: &str) -> Result<(), String> {
    let path = path_for(dir, name)?;
    std::fs::create_dir_all(dir).map_err(|err| format!("couldn't save the key ({err})"))?;
    restrict(dir, 0o700)?;
    std::fs::write(&path, value.as_bytes()).map_err(|err| format!("couldn't save the key ({err})"))?;
    restrict(&path, 0o600)?;
    Ok(())
}

fn file_remove(dir: &Path, name: &str) -> Result<(), String> {
    let path = path_for(dir, name)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("couldn't clear the key ({err})")),
    }
}

fn restrict(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|err| format!("couldn't protect the key ({err})"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_without_jni_keeps_the_key_in_a_private_file() {
        let dir = std::env::temp_dir().join(format!("layerwise-ai-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store { dir: dir.clone() };
        store.store("retro.token", "test-token").expect("store");
        assert_eq!(store.load("retro.token").expect("load").as_deref(), Some("test-token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("retro.token")).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the key file is owner-only");
        }
        store.remove("retro.token").expect("clear");
        assert!(store.load("retro.token").expect("gone").is_none());
        assert!(store.store("../retro.token", "nope").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
