// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Errors a provider can hand back. The text is what the user sees, so it never includes a key.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiErrorKind {
    BadParams,
    Auth,
    Credits,
    Offline,
    Timeout,
    Workflow,
    Cancelled,
    Unsupported,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiError {
    pub kind: AiErrorKind,
    pub message: String,
}

impl AiError {
    pub fn new(kind: AiErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::BadParams, message)
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Auth, message)
    }

    pub fn credits(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Credits, message)
    }

    pub fn offline(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Offline, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Timeout, message)
    }

    pub fn workflow(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Workflow, message)
    }

    pub fn cancelled() -> Self {
        Self::new(AiErrorKind::Cancelled, "Cancelled.")
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Unsupported, message)
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Other, message)
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == AiErrorKind::Cancelled
    }
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AiError {}

/// Keep a server's answer short, and take a saved key out of it if the server echoed one.
pub fn clip_secret(text: &str, secret: &str) -> String {
    let clipped: String = text.chars().take(180).collect();
    if secret.len() < 4 {
        return clipped;
    }
    clipped.replace(secret, "••••")
}
