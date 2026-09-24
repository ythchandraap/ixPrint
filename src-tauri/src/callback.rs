//! Print-result callbacks: a client sends `callback_url` with a print and
//! ix.print POSTs the outcome there once the job has left the spooler.
//!
//! Only requests made with an API token may ask for one, and every delivery
//! is signed with that token: `X-IxPrint-Signature: sha256=HMAC(key, "{ts}.{body}")`
//! where the key is the hex SHA-256 of the token and `ts` is `X-IxPrint-Timestamp`.
//! Redirects are never followed, so a callback can't be bounced elsewhere.

use crate::auth::ApiToken;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::Duration;

const MAX_URL: usize = 2048;
const MAX_REFERENCE: usize = 200;
/// Pause before each delivery attempt; 2xx ends it.
const RETRY_DELAYS: [Duration; 4] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
];

#[cfg(test)]
static ALLOW_LOOPBACK_FOR_TESTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where and how to report one job.
pub struct Callback {
    pub(crate) url: String,
    pub(crate) reference: Option<String>,
    /// HMAC key: the token's stored hash (the client computes it from its token).
    key: String,
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
            key: token.hash.clone(),
        }))
    }
}

fn validate_url(url: &str) -> Result<(), String> {
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

fn is_blocked_ip(ip: IpAddr) -> bool {
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

fn resolve_checked(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| "callback_url host could not be resolved".to_string())?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("callback_url host could not be resolved".into());
    }
    if addresses.iter().any(|address| is_blocked_ip(address.ip())) {
        return Err("callback_url resolves to a loopback or link-local address".into());
    }
    Ok(addresses)
}

fn callback_host_and_port(url: &str) -> Result<(String, u16), String> {
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| "callback_url is invalid".to_string())?;
    let authority = uri.authority().ok_or_else(|| "callback_url is invalid".to_string())?;
    if authority.port().is_some() && authority.port_u16().is_none() {
        return Err("callback_url is invalid".into());
    }
    let default_port = match uri.scheme_str() {
        Some("http") => 80,
        Some("https") => 443,
        _ => return Err("callback_url is invalid".into()),
    };
    let host = authority.host().trim_matches(['[', ']']).to_string();
    if host.is_empty() {
        return Err("callback_url is invalid".into());
    }
    Ok((host, authority.port_u16().unwrap_or(default_port)))
}

#[derive(Serialize, Debug)]
pub struct Payload {
    /// `job.finished` once per job; `job.problem` when the printer first reports a problem.
    pub event: &'static str,
    pub job_id: String,
    pub reference: Option<String>,
    /// Printed (`true`) or not (`false`); `null` for `job.problem`.
    pub success: Option<bool>,
    /// completed | sent | failed | gone | timeout | error
    pub state: String,
    pub message: Option<String>,
    pub printer: String,
    pub preset: Option<String>,
    pub sheets: u32,
    pub time: String,
}

#[cfg(test)]
pub(crate) use crate::application::callback_service::should_mark_sent;
/// Follows the job in a background thread and reports its outcome.
/// Refused when too many jobs are being watched already.
pub use crate::application::callback_service::watch;

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

#[derive(Debug)]
struct PinnedResolver {
    addresses: Vec<SocketAddr>,
}

impl ureq::unversioned::resolver::Resolver for PinnedResolver {
    fn resolve(
        &self,
        _uri: &ureq::http::Uri,
        _config: &ureq::config::Config,
        _timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut resolved = self.empty();
        for address in self.addresses.iter().copied().take(16) {
            resolved.push(address);
        }
        Ok(resolved)
    }
}

fn callback_agent(addresses: Vec<SocketAddr>) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("ix.print/", env!("CARGO_PKG_VERSION")))
        .build();
    ureq::Agent::with_parts(
        config,
        ureq::unversioned::transport::DefaultConnector::default(),
        PinnedResolver { addresses },
    )
}

/// Sends `payload`, retrying per `RETRY_DELAYS`. `progress(state, attempts, outcome)`
/// is told about each attempt: `retrying`, then `delivered` or `failed`.
fn deliver(callback: &Callback, payload: &Payload, mut progress: impl FnMut(&str, u32, &Attempt)) {
    let Ok(body) = serde_json::to_vec(payload) else { return };
    for (i, delay) in RETRY_DELAYS.iter().enumerate() {
        std::thread::sleep(*delay);
        let timestamp = chrono::Utc::now().timestamp();
        let result = callback_host_and_port(&callback.url).and_then(|(host, port)| {
            let addresses = resolve_checked(&host, port)?;
            callback_agent(addresses)
                .post(&callback.url)
                .header("Content-Type", "application/json")
                .header("X-IxPrint-Event", payload.event)
                .header("X-IxPrint-Timestamp", timestamp.to_string())
                .header("X-IxPrint-Signature", signature(&callback.key, timestamp, &body))
                .send(&body[..])
                .map_err(|e| e.to_string())
        });
        let attempt = match result {
            Ok(resp) if resp.status().is_success() => Attempt::Delivered(resp.status().as_u16()),
            Ok(resp) => Attempt::Failed {
                status: Some(resp.status().as_u16()),
                error: format!("Receiving server returned HTTP {}", resp.status().as_u16()),
            },
            Err(error) => Attempt::Failed { status: None, error },
        };
        let attempts = i as u32 + 1;
        match &attempt {
            Attempt::Delivered(_) => return progress("delivered", attempts, &attempt),
            Attempt::Failed { error, .. } if attempts as usize == RETRY_DELAYS.len() => {
                eprintln!(
                    "[ix.print] callback {} for job {} failed to send to {}: {}",
                    payload.event, payload.job_id, callback.url, error
                );
                return progress("failed", attempts, &attempt);
            }
            Attempt::Failed { .. } => progress("retrying", attempts, &attempt),
        }
    }
}

pub(crate) fn deliver_with(callback: &Callback, payload: &Payload, progress: &mut dyn FnMut(&str, u32, &Attempt)) {
    deliver(callback, payload, |state, attempts, outcome| {
        progress(state, attempts, outcome)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::JobResult;
    use std::sync::Arc;

    static LOOPBACK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct LoopbackTestOverride;

    impl Drop for LoopbackTestOverride {
        fn drop(&mut self) {
            ALLOW_LOOPBACK_FOR_TESTS.store(false, Ordering::SeqCst);
        }
    }

    #[test]
    fn validates_requests() {
        let (_, token) = crate::auth::generate("SIMRS").unwrap();
        let ok = |url: &str| Callback::from_request(Some(url), None, Some(&token));
        assert!(ok("https://simrs.contoh.id/api/print-callback?x=1").unwrap().is_some());
        assert!(ok("http://192.168.1.20:8080/cb").unwrap().is_some());
        for bad in [
            "ftp://a.id/x",
            "https://",
            "javascript:alert(1)",
            "https://u:p@a.id/",
            "https://a.id/ x",
        ] {
            assert!(ok(bad).is_err(), "{}", bad);
        }
        assert!(Callback::from_request(None, None, None).unwrap().is_none());
        assert!(Callback::from_request(Some("  "), None, None).unwrap().is_none());
        // Without a token there is nothing to sign with.
        assert!(Callback::from_request(Some("https://a.id/cb"), None, None).is_err());
        let long = "x".repeat(MAX_REFERENCE + 1);
        assert!(Callback::from_request(Some("https://a.id/cb"), Some(&long), Some(&token)).is_err());
    }

    #[test]
    fn rejects_loopback_and_link_local_ip_literals() {
        let _lock = LOOPBACK_TEST_LOCK.lock().unwrap();
        for url in [
            "http://127.0.0.1:8080/hook",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]:9000/hook",
        ] {
            assert!(
                validate_url(url)
                    .unwrap_err()
                    .contains("cannot target a loopback or link-local address"),
                "{}",
                url
            );
        }
        assert!(validate_url("http://192.168.1.50:9000/hook").is_ok());
        assert!(validate_url("https://simrs.contoh.id/api/print-callback?x=1").is_ok());
    }

    #[test]
    fn blocks_only_unsafe_ip_ranges() {
        let _lock = LOOPBACK_TEST_LOCK.lock().unwrap();
        for address in [
            "127.0.0.1",
            "::1",
            "169.254.169.254",
            "fe80::1",
            "0.0.0.0",
            "::",
            "::ffff:127.0.0.1",
            "224.0.0.1",
        ] {
            assert!(is_blocked_ip(address.parse().unwrap()), "{}", address);
        }
        for address in ["10.1.2.3", "192.168.1.5", "172.16.0.9", "8.8.8.8", "fd00::1"] {
            assert!(!is_blocked_ip(address.parse().unwrap()), "{}", address);
        }
    }

    #[test]
    fn resolve_checked_rejects_loopback_hosts() {
        let _lock = LOOPBACK_TEST_LOCK.lock().unwrap();
        assert!(resolve_checked("localhost", 80).is_err());
        assert!(resolve_checked("127.0.0.1", 80).is_err());
    }

    /// Accepts one HTTP request on `listener`, answers 200, returns (headers, body).
    fn receive(listener: std::net::TcpListener) -> (String, String) {
        use std::io::{BufRead, BufReader, Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            head.push_str(&line.to_ascii_lowercase());
        }
        let len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .map(|v| v.trim().parse().unwrap())
            .unwrap();
        let mut body = vec![0; len];
        reader.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
        (head, String::from_utf8(body).unwrap())
    }

    #[test]
    fn delivers_signed_result_of_failed_job() {
        let _lock = LOOPBACK_TEST_LOCK.lock().unwrap();
        let mut state = Arc::try_unwrap(crate::application::state::test_support::state())
            .ok()
            .unwrap();
        state.callback_sender = Arc::new(crate::infrastructure::net::callback_client::CallbackClient);
        let job = JobResult {
            job_id: "job-1".into(),
            printer: "EPSON".into(),
            preset: Some("struk".into()),
            sheets: 2,
            spooler_job: None,
        };
        state
            .history
            .append(&crate::history::HistoryEntry {
                id: job.job_id.clone(),
                time: chrono::Local::now().to_rfc3339(),
                source: "http".into(),
                client: None,
                token: Some("SIMRS".into()),
                file_name: "x.pdf".into(),
                preset: job.preset.clone(),
                printer: job.printer.clone(),
                spooler_job: None,
                sheets: 2,
                settings: serde_json::Value::Null,
                success: false,
                error: Some("Kertas macet".into()),
                duration_ms: 1,
                callback: None,
                remote: None,
            })
            .unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/cb", listener.local_addr().unwrap());
        let (token, stored) = crate::auth::generate("SIMRS").unwrap();
        // This test exercises delivery/signing with a local receiver. User supplied
        // callback URLs go through validate_url, which intentionally rejects loopback.
        let cb = Callback {
            url,
            reference: Some("ORDER-42".into()),
            key: stored.hash.clone(),
        };
        let history = state.history.clone();
        ALLOW_LOOPBACK_FOR_TESTS.store(true, Ordering::SeqCst);
        let _loopback_override = LoopbackTestOverride;
        watch(Arc::new(state), &job, cb).unwrap();
        let (head, body) = receive(listener);

        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["event"], "job.finished");
        assert_eq!(json["job_id"], "job-1");
        assert_eq!(json["reference"], "ORDER-42");
        assert_eq!(json["success"], false);
        assert_eq!(json["state"], "failed");
        assert_eq!(json["message"], "Kertas macet");
        // The receiver verifies with its own token: key = hex SHA-256 of the token.
        let header = |name: &str| {
            head.lines()
                .find_map(|l| l.strip_prefix(name))
                .map(|v| v.trim().to_string())
                .unwrap()
        };
        let ts: i64 = header("x-ixprint-timestamp:").parse().unwrap();
        assert_eq!(
            header("x-ixprint-signature:"),
            signature(&crate::auth::hash(&token), ts, body.as_bytes())
        );

        // The History tab sees the delivery.
        let delivered = (0..50).find_map(|_| {
            let cb = history
                .find("job-1")
                .and_then(|e| e.callback)
                .filter(|c| c.state == "delivered");
            if cb.is_none() {
                std::thread::sleep(Duration::from_millis(20));
            }
            cb
        });
        let cb = delivered.expect("callback status recorded");
        assert_eq!(
            (cb.result.as_deref(), cb.attempts, cb.http_status),
            (Some("failed"), 1, Some(200))
        );
        assert_eq!(cb.reference.as_deref(), Some("ORDER-42"));
    }

    #[test]
    fn signature_matches_reference_hmac() {
        // Python: hmac.new(b"k", b'1700000000.{"a":1}', hashlib.sha256).hexdigest()
        assert_eq!(
            signature("k", 1700000000, br#"{"a":1}"#),
            "sha256=1b6ad1bc9bf48c52ce01939866d5877c6cc4e87ddad27e16673cb8c4964394a3"
        );
        assert_ne!(
            signature("k", 1700000000, br#"{"a":1}"#),
            signature("k", 1700000001, br#"{"a":1}"#)
        );
    }

    #[test]
    fn remote_unknown_status_is_not_marked_sent() {
        assert!(!should_mark_sent(
            &serde_json::json!({"state":"unknown","spooler_job":null,"remote":"PC2"})
        ));
        assert!(should_mark_sent(
            &serde_json::json!({"state":"unknown","spooler_job":null})
        ));
    }
}
