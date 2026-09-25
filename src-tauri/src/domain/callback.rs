//! Signed print callbacks and their URL validation.

use crate::domain::auth::ApiToken;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::net::IpAddr;
#[cfg(test)]
use std::sync::atomic::Ordering;

pub(crate) const MAX_REFERENCE: usize = 200;
pub(crate) const MAX_URL: usize = 2048;

#[cfg(test)]
pub(crate) static ALLOW_LOOPBACK_FOR_TESTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where and how to report one job.
#[derive(Clone)]
pub struct Callback {
    pub(crate) url: String,
    pub(crate) reference: Option<String>,
    pub(crate) token_id: String,
    /// HMAC key: the token's stored hash (the client computes it from its token).
    pub(crate) key: String,
}

impl Callback {
    /// `None` when no `callback_url` was sent.
    pub fn from_request(
        url: Option<&str>,
        reference: Option<&str>,
        token: Option<&ApiToken>,
    ) -> Result<Option<Callback>, String> {
        let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
            return Ok(None);
        };
        let token =
            token.ok_or("callback_url can only be used with an API token (the callback is signed with that token)")?;
        validate_url(url)?;
        let reference = reference.map(str::trim).filter(|r| !r.is_empty());
        if reference.is_some_and(|r| r.chars().count() > MAX_REFERENCE) {
            return Err(format!("callback_ref must be at most {} characters", MAX_REFERENCE));
        }
        Ok(Some(Callback {
            url: url.to_string(),
            reference: reference.map(str::to_string),
            token_id: token.id.clone(),
            key: token.hash.clone(),
        }))
    }
}

pub(crate) fn validate_url(url: &str) -> Result<(), String> {
    let bad = || format!("callback_url is invalid: '{}' (must use http:// or https://)", url);
    if url.len() > MAX_URL {
        return Err(format!("callback_url must be at most {} characters", MAX_URL));
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split_once(']').map(|(host, _)| host).unwrap_or_default()
    } else {
        authority.rsplit_once(':').map(|(host, _)| host).unwrap_or(authority)
    };
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err("callback_url cannot target a loopback or link-local address".into());
        }
    }
    if authority.contains('@') {
        return Err("callback_url must not contain a username or password".into());
    }
    Ok(())
}

pub(crate) fn is_blocked_ip(ip: IpAddr) -> bool {
    #[cfg(test)]
    if ALLOW_LOOPBACK_FOR_TESTS.load(Ordering::SeqCst) && ip.is_loopback() {
        return false;
    }

    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(ipv4));
            }
            let first = ip.segments()[0];
            ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || first & 0xffc0 == 0xfe80
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Payload {
    /// `job.finished` once per job; `job.problem` when the printer first reports a problem.
    pub event: &'static str,
    pub job_id: String,
    pub reference: Option<String>,
    /// Spooler completion (`true`), known failure (`false`), or unknown outcome (`null`).
    pub success: Option<bool>,
    /// completed | sent | failed | gone | timeout | error
    pub state: String,
    pub message: Option<String>,
    pub printer: String,
    pub preset: Option<String>,
    pub sheets: u32,
    pub time: String,
}

/// `sha256=<hex>` over `"{timestamp}.{body}"`.
pub fn signature(key: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC takes any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("sha256={}", hex)
}

pub enum Attempt {
    Delivered(u16),
    Failed { status: Option<u16>, error: String },
}
