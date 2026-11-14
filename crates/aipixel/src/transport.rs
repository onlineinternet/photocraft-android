// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! The HTTP and WebSocket seam. Tests install a scripted transport; the app uses the native one.
//! Nothing here records a header value, only which kind of auth was attached.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use crate::error::AiError;

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum WsEvent {
    Text(String),
    Timeout,
    Closed,
}

pub trait WsConn: Send {
    fn recv(&mut self, timeout: Duration) -> Result<WsEvent, AiError>;
}

pub trait Transport: Send + Sync {
    fn round_trip(&self, req: &HttpRequest) -> Result<HttpResponse, AiError>;
    fn open_ws(&self, url: &str, headers: &[(String, String)]) -> Result<Box<dyn WsConn>, AiError>;
}

fn slot() -> &'static Mutex<Option<Arc<dyn Transport>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<dyn Transport>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// One lock for tests that install a transport or a known secret. The slots are process-wide.
#[cfg(test)]
pub fn test_gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn install(transport: Option<Arc<dyn Transport>>) {
    *slot().lock().unwrap_or_else(PoisonError::into_inner) = transport;
}

pub fn current() -> Arc<dyn Transport> {
    if let Some(installed) = slot().lock().unwrap_or_else(PoisonError::into_inner).clone() {
        return installed;
    }
    Arc::new(default_transport())
}

#[cfg(not(target_arch = "wasm32"))]
fn default_transport() -> crate::native::NativeTransport {
    crate::native::NativeTransport
}

#[cfg(target_arch = "wasm32")]
fn default_transport() -> Unavailable {
    Unavailable
}

#[cfg(target_arch = "wasm32")]
struct Unavailable;

#[cfg(target_arch = "wasm32")]
impl Transport for Unavailable {
    fn round_trip(&self, _req: &HttpRequest) -> Result<HttpResponse, AiError> {
        Err(AiError::unsupported("AI Pixel Studio isn't available in this build."))
    }

    fn open_ws(&self, _url: &str, _headers: &[(String, String)]) -> Result<Box<dyn WsConn>, AiError> {
        Err(AiError::unsupported("AI Pixel Studio isn't available in this build."))
    }
}

/// What a test is allowed to remember about a request: the auth scheme, never the secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub method: String,
    pub url: String,
    pub auth: Vec<String>,
}

pub struct MockTransport {
    inner: Mutex<MockState>,
}

struct MockState {
    http: VecDeque<Result<HttpResponse, AiError>>,
    ws: Vec<String>,
    seen: Vec<Seen>,
}

impl MockTransport {
    pub fn new(http: Vec<Result<HttpResponse, AiError>>, ws: Vec<String>) -> Self {
        Self { inner: Mutex::new(MockState { http: http.into(), ws, seen: Vec::new() }) }
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).seen.clone()
    }
}

impl Transport for MockTransport {
    fn round_trip(&self, req: &HttpRequest) -> Result<HttpResponse, AiError> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.seen.push(seen_of(req.method.clone(), req.url.clone(), &req.headers));
        guard.http.pop_front().unwrap_or_else(|| Err(AiError::other("mock HTTP has no scripted response")))
    }

    fn open_ws(&self, url: &str, headers: &[(String, String)]) -> Result<Box<dyn WsConn>, AiError> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.seen.push(seen_of("WS".into(), url.to_string(), headers));
        Ok(Box::new(MockWs { messages: guard.ws.clone(), index: 0, closed: false }))
    }
}

fn seen_of(method: String, url: String, headers: &[(String, String)]) -> Seen {
    let mut auth = Vec::new();
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if lower == "authorization" {
            let scheme = value.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            if scheme == "bearer" || scheme == "basic" {
                auth.push(scheme);
            }
        } else if lower == "x-rd-token" && !value.is_empty() {
            auth.push("rd-token".into());
        }
    }
    Seen { method, url, auth }
}

struct MockWs {
    messages: Vec<String>,
    index: usize,
    closed: bool,
}

impl WsConn for MockWs {
    fn recv(&mut self, _timeout: Duration) -> Result<WsEvent, AiError> {
        if self.closed {
            return Ok(WsEvent::Closed);
        }
        if let Some(text) = self.messages.get(self.index) {
            self.index += 1;
            Ok(WsEvent::Text(text.clone()))
        } else {
            self.closed = true;
            Ok(WsEvent::Closed)
        }
    }
}
