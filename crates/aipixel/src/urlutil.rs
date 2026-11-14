// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Base URLs. Cleartext HTTP is limited to loopback, LAN and Tailscale addresses. Anything else
//! has to be HTTPS. Credentials in the URL itself are refused; auth is a separate setting.

use crate::error::AiError;

pub fn join(base: &str, path_and_query: &str) -> Result<String, AiError> {
    let base = base.trim().trim_end_matches('/');
    validate_base(base)?;
    if path_and_query.chars().count() > 2048 {
        return Err(AiError::bad("the request path is too long"));
    }
    let path = if path_and_query.starts_with('/') { path_and_query.to_string() } else { format!("/{path_and_query}") };
    Ok(format!("{base}{path}"))
}

pub fn validate_base(base: &str) -> Result<(), AiError> {
    if base.is_empty() {
        return Err(AiError::bad("Enter the server address first."));
    }
    if base.chars().count() > 512 {
        return Err(AiError::bad("the server address is longer than 512 characters"));
    }
    if base.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AiError::bad("the server address contains a space or a control character"));
    }
    let (scheme, rest) = base.split_once("://").ok_or_else(|| AiError::bad("the server address needs http:// or https://"))?;
    if rest.contains('@') {
        return Err(AiError::bad("put the password in the auth setting, not in the address"));
    }
    let host = host_of(rest)?;
    if host.is_empty() {
        return Err(AiError::bad("the server address has no host"));
    }
    match scheme {
        "https" => Ok(()),
        "http" if host_allows_cleartext(host) => Ok(()),
        "http" => Err(AiError::bad(
            "Cleartext HTTP is only allowed for a LAN or Tailscale address (for example 192.168.x.x or 100.x.x.x). Use https:// for other servers.",
        )),
        _ => Err(AiError::bad("the server address needs http:// or https://")),
    }
}

pub fn ws_url(base: &str, client_id: &str) -> Result<String, AiError> {
    let http = join(base, &format!("/ws?clientId={}", query_encode(client_id)))?;
    if let Some(rest) = http.strip_prefix("https://") {
        Ok(format!("wss://{rest}"))
    } else if let Some(rest) = http.strip_prefix("http://") {
        Ok(format!("ws://{rest}"))
    } else {
        Err(AiError::bad("the server address needs http:// or https://"))
    }
}

pub fn query_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn host_of(rest: &str) -> Result<&str, AiError> {
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if let Some(inner) = hostport.strip_prefix('[') {
        let end = inner.find(']').ok_or_else(|| AiError::bad("the IPv6 address is missing a closing bracket"))?;
        return Ok(inner.get(..end).unwrap_or(""));
    }
    Ok(match hostport.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => hostport,
    })
}

/// Loopback, RFC1918, link-local, and Tailscale's CGNAT range (100.64.0.0/10), plus `localhost`
/// and `*.local`. Unique-local IPv6 (fc00::/7) covers Tailscale's IPv6 range.
pub fn host_allows_cleartext(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.');
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".local") {
        return true;
    }
    if let Some(v4) = parse_v4(host) {
        return v4[0] == 127
            || v4[0] == 10
            || (v4[0] == 172 && (16..=31).contains(&v4[1]))
            || (v4[0] == 192 && v4[1] == 168)
            || (v4[0] == 169 && v4[1] == 254)
            || (v4[0] == 100 && (64..=127).contains(&v4[1]));
    }
    ipv6_is_local(host)
}

fn parse_v4(host: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut parts = host.split('.');
    for slot in &mut out {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            return None;
        }
        *slot = part.parse().ok()?;
    }
    if parts.next().is_some() { None } else { Some(out) }
}

fn ipv6_is_local(host: &str) -> bool {
    let text = host.to_ascii_lowercase();
    if text == "::1" {
        return true;
    }
    let Some(first) = text.split(':').next() else { return false };
    let Ok(word) = u16::from_str_radix(first, 16) else { return false };
    (0xfc00..=0xfdff).contains(&word) || (0xfe80..=0xfebf).contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleartext_is_limited_to_lan_and_tailscale() {
        assert!(validate_base("http://192.168.1.5:8188").is_ok());
        assert!(validate_base("http://10.0.0.8:8188").is_ok());
        assert!(validate_base("http://100.64.0.4:8188").is_ok());
        assert!(validate_base("http://100.127.255.1").is_ok());
        assert!(validate_base("http://172.16.0.2:8188").is_ok());
        assert!(validate_base("http://127.0.0.1:8188").is_ok());
        assert!(validate_base("http://localhost:8188").is_ok());
        assert!(validate_base("https://example.com").is_ok());
        assert!(validate_base("http://8.8.8.8:8188").is_err());
        assert!(validate_base("http://example.com:8188").is_err());
        assert!(validate_base("http://user:pass@192.168.1.5:8188").is_err());
        assert!(validate_base("ftp://192.168.1.5").is_err());
        assert!(!host_allows_cleartext("100.63.0.1"));
        assert!(!host_allows_cleartext("172.32.0.1"));
    }

    #[test]
    fn a_websocket_url_follows_the_scheme() {
        let ws = ws_url("http://192.168.1.5:8188", "abc").expect("ws");
        assert_eq!(ws, "ws://192.168.1.5:8188/ws?clientId=abc");
        let wss = ws_url("https://comfy.example", "a b").expect("wss");
        assert_eq!(wss, "wss://comfy.example/ws?clientId=a%20b");
    }
}
