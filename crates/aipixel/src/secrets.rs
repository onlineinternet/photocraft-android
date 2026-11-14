// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! API keys. The process keeps a copy, and a [`SecretStore`] (the Android Keystore, or a private
//! file) keeps the durable copy. Values are never written to a log, a preference, or a command.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

pub const COMFY_BEARER: &str = "comfy.bearer";
pub const COMFY_BASIC: &str = "comfy.basic";
pub const RETRO_TOKEN: &str = "retro.token";

const KNOWN: &[&str] = &[COMFY_BEARER, COMFY_BASIC, RETRO_TOKEN];

pub trait SecretStore: Send + Sync {
    fn load(&self, name: &str) -> Result<Option<String>, String>;
    fn store(&self, name: &str, value: &str) -> Result<(), String>;
    fn remove(&self, name: &str) -> Result<(), String>;
}

struct State {
    cache: BTreeMap<String, String>,
    store: Option<std::sync::Arc<dyn SecretStore>>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State { cache: BTreeMap::new(), store: None }))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn install(store: std::sync::Arc<dyn SecretStore>) {
    lock().store = Some(store);
}

/// Read the known keys into the cache once, at startup. A store error is ignored so a broken
/// keystore never stops the app from opening.
pub fn load_known() {
    let store = lock().store.clone();
    let Some(store) = store else { return };
    for name in KNOWN {
        if let Ok(Some(value)) = store.load(name)
            && !value.is_empty()
        {
            lock().cache.insert((*name).to_string(), value);
        }
    }
}

pub fn is_set(name: &str) -> bool {
    get(name).is_some()
}

pub fn get(name: &str) -> Option<String> {
    if !valid_name(name) {
        return None;
    }
    let guard = lock();
    if let Some(value) = guard.cache.get(name) {
        return Some(value.clone());
    }
    let store = guard.store.clone();
    drop(guard);
    let value = store.and_then(|s| s.load(name).ok().flatten());
    let value = value.filter(|v| !v.is_empty())?;
    lock().cache.insert(name.to_string(), value.clone());
    Some(value)
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("that secret can't be stored".into());
    }
    if value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        return Err("the key contains a character that can't be stored".into());
    }
    if value.chars().count() > 512 {
        return Err("the key is longer than 512 characters".into());
    }
    let store = lock().store.clone();
    if value.is_empty() {
        return clear(name);
    }
    if let Some(store) = store.as_ref() {
        store.store(name, value)?;
    }
    lock().cache.insert(name.to_string(), value.to_string());
    Ok(())
}

pub fn clear(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("that secret can't be stored".into());
    }
    let store = lock().store.clone();
    if let Some(store) = store.as_ref() {
        store.remove(name)?;
    }
    lock().cache.remove(name);
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_in_memory_and_a_newline_is_refused() {
        let name = "test.retro.one";
        let _ = clear(name);
        assert!(!is_set(name));
        set(name, "test-token").expect("set");
        assert_eq!(get(name).as_deref(), Some("test-token"));
        assert!(is_set(name));
        assert!(set(name, "line\nbreak").is_err());
        assert_eq!(get(name).as_deref(), Some("test-token"));
        clear(name).expect("clear");
        assert!(get(name).is_none());
    }
}
