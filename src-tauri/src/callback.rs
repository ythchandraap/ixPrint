//! Print-result callbacks: a client sends `callback_url` with a print and
//! ix.print POSTs the outcome there once the job has left the spooler.
//!
//! Only requests made with an API token may ask for one, and every delivery
//! is signed with that token: `X-IxPrint-Signature: sha256=HMAC(key, "{ts}.{body}")`
//! where the key is the hex SHA-256 of the token and `ts` is `X-IxPrint-Timestamp`.
//! Redirects are never followed, so a callback can't be bounced elsewhere.

use crate::auth::ApiToken;
use crate::history::CallbackStatus;
use crate::job::{AppState, JobResult};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_URL: usize = 2048;
const MAX_REFERENCE: usize = 200;
/// Jobs watched at once; each polls the spooler until its job is done.
const MAX_ACTIVE: usize = 200;
const POLL_EVERY: Duration = Duration::from_secs(3);
/// A job still not done after this is reported as `timeout`.
const GIVE_UP_AFTER: Duration = Duration::from_secs(60 * 60);
/// Pause before each delivery attempt; 2xx ends it.
const RETRY_DELAYS: [Duration; 4] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
];

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Where and how to report one job.
pub struct Callback {
    url: String,
    reference: Option<String>,
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
            return Err(format!("callback_ref maksimal {} karakter", MAX_REFERENCE));
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
        return Err(format!("callback_url maksimal {} karakter", MAX_URL));
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
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_link_local(),
            std::net::IpAddr::V6(ip) => {
                let octets = ip.octets();
                ip.is_loopback() || (octets[0] == 0xfe && octets[1] & 0xc0 == 0x80)
            }
        };
        if blocked {
            return Err("callback_url cannot target a loopback or link-local address".into());
        }
    } else {
        // DNS-rebinding protection is deferred to a later iteration.
    }
    if authority.contains('@') {
        return Err("callback_url must not contain a username or password".into());
    }
    Ok(())
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

/// Follows the job in a background thread and reports its outcome.
/// Refused when too many jobs are being watched already.
pub fn watch(state: Arc<AppState>, job: &JobResult, callback: Callback) -> Result<(), String> {
    let record = {
        let (state, job_id, url, reference) = (
            state.clone(),
            job.job_id.clone(),
            callback.url.clone(),
            callback.reference.clone(),
        );
        move |status: &str, result: Option<&str>, attempts: u32, outcome: Option<&Attempt>| {
            let (http_status, error) = match outcome {
                Some(Attempt::Delivered(code)) => (Some(*code), None),
                Some(Attempt::Failed { status, error }) => (*status, Some(error.clone())),
                None => (None, None),
            };
            let entry = CallbackStatus {
                url: url.clone(),
                reference: reference.clone(),
                state: status.to_string(),
                result: result.map(str::to_string),
                attempts,
                http_status,
                error,
                time: chrono::Local::now().to_rfc3339(),
            };
            if let Err(e) = state.history.append_callback(&job_id, entry) {
                eprintln!("[ix.print] callback status was not recorded in history: {}", e);
            }
        }
    };
    if ACTIVE.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        let message = "too many callbacks are currently waiting".to_string();
        record(
            "refused",
            None,
            0,
            Some(&Attempt::Failed {
                status: None,
                error: message.clone(),
            }),
        );
        return Err(message);
    }
    record("watching", None, 0, None);
    let (job_id, printer, preset, sheets) = (job.job_id.clone(), job.printer.clone(), job.preset.clone(), job.sheets);
    let reference = callback.reference.clone();
    let callback = Arc::new(callback);
    std::thread::spawn(move || {
        let payload = |event, success, state: &str, message: Option<String>| Payload {
            event,
            job_id: job_id.clone(),
            reference: reference.clone(),
            success,
            state: state.to_string(),
            message,
            printer: printer.clone(),
            preset: preset.clone(),
            sheets,
            time: chrono::Local::now().to_rfc3339(),
        };
        let started = Instant::now();
        let mut problem_sent = false;
        let finished = loop {
            let status = crate::server::job_status_json(&state, &job_id).unwrap_or_default();
            let text = |key: &str| status.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let message = text("message").or_else(|| text("printer_message"));
            match text("state").as_deref().unwrap_or("unknown") {
                "completed" => break payload("job.finished", Some(true), "completed", None),
                "failed" => break payload("job.finished", Some(false), "failed", message),
                "gone" => {
                    break payload(
                        "job.finished",
                        Some(false),
                        "gone",
                        Some("Job left the queue without completing (it may have been cancelled)".into()),
                    )
                }
                // Spoolers that give no job reference: handed over is all we can know.
                "unknown" if should_mark_sent(&status) => {
                    break payload(
                        "job.finished",
                        Some(true),
                        "sent",
                        Some("Sent to the printer; the spooler did not report the job status".into()),
                    )
                }
                "error" if !problem_sent => {
                    problem_sent = true;
                    let p = payload("job.problem", None, "error", message);
                    let cb = callback.clone();
                    std::thread::spawn(move || deliver(&cb, &p, |_, _, _| {}));
                }
                _ => {}
            }
            if started.elapsed() > GIVE_UP_AFTER {
                let last = text("state").unwrap_or_else(|| "unknown".into());
                break payload(
                    "job.finished",
                    Some(false),
                    "timeout",
                    Some(format!(
                        "Belum selesai setelah {} menit (status terakhir: {})",
                        GIVE_UP_AFTER.as_secs() / 60,
                        last
                    )),
                );
            }
            std::thread::sleep(POLL_EVERY);
        };
        let result = finished.state.clone();
        deliver(&callback, &finished, |status, attempts, outcome| {
            record(status, Some(&result), attempts, Some(outcome))
        });
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    });
    Ok(())
}

fn should_mark_sent(status: &serde_json::Value) -> bool {
    status.get("remote").is_none()
        && status.get("state").and_then(|v| v.as_str()) == Some("unknown")
        && status.get("spooler_job").is_some_and(|v| v.is_null())
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

/// Sends `payload`, retrying per `RETRY_DELAYS`. `progress(state, attempts, outcome)`
/// is told about each attempt: `retrying`, then `delivered` or `failed`.
fn deliver(callback: &Callback, payload: &Payload, mut progress: impl FnMut(&str, u32, &Attempt)) {
    let Ok(body) = serde_json::to_vec(payload) else { return };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("ix.print/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    for (i, delay) in RETRY_DELAYS.iter().enumerate() {
        std::thread::sleep(*delay);
        let timestamp = chrono::Utc::now().timestamp();
        let result = agent
            .post(&callback.url)
            .header("Content-Type", "application/json")
            .header("X-IxPrint-Event", payload.event)
            .header("X-IxPrint-Timestamp", timestamp.to_string())
            .header("X-IxPrint-Signature", signature(&callback.key, timestamp, &body))
            .send(&body[..]);
        let attempt = match result {
            Ok(resp) if resp.status().is_success() => Attempt::Delivered(resp.status().as_u16()),
            Ok(resp) => Attempt::Failed {
                status: Some(resp.status().as_u16()),
                error: format!("Receiving server returned HTTP {}", resp.status().as_u16()),
            },
            Err(e) => Attempt::Failed {
                status: None,
                error: e.to_string(),
            },
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let dir = std::env::temp_dir().join(format!("ixprint-cb-{}", uuid::Uuid::new_v4()));
        let state = Arc::new(AppState {
            config: crate::config::ConfigStore::load(&dir),
            desktop_key: "test-desktop-key".into(),
            remotes: crate::remote::RemoteStore::load(&dir),
            remote_cache: Default::default(),
            server_error: Default::default(),
            server_handle: Default::default(),
            token_usage: Default::default(),
            failed_auth: Default::default(),
            history: crate::history::History::new(dir.join("history")),
            diagnostics: crate::diagnostics::Diagnostics::new(&dir),
            jobs_dir: dir.join("jobs"),
        });
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
        let history = crate::history::History::new(dir.join("history"));
        watch(state, &job, cb).unwrap();
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
        let _ = std::fs::remove_dir_all(dir);
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
