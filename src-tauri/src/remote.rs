//! Remote bridge operations retained here until their use cases and network client move in L4.8.

use crate::domain::printer;
pub use crate::domain::remote::*;
use crate::domain::settings::PrintSettings;
pub use crate::infrastructure::persistence::remote_store::RemoteStore;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Deserialize)]
struct RemotePrinterView {
    name: String,
    status: String,
    message: Option<String>,
    shared: String,
    os_default: bool,
    #[serde(default)]
    remote: Option<String>,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("ix.print/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn request_error(remote: &Remote, error: impl std::fmt::Display) -> RemoteError {
    RemoteError::Failed(format!("{} cannot be reached ({}): {}", remote.name, remote.url, error))
}

fn response_message(body: &str, status: u16) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| format!("HTTP {}", status))
}

fn read_response<T: DeserializeOwned>(
    remote: &Remote,
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T, RemoteError> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| request_error(remote, e))?;
    if (200..300).contains(&status) {
        serde_json::from_str(&body)
            .map_err(|e| RemoteError::Failed(format!("{} returned invalid JSON: {}", remote.name, e)))
    } else {
        let message = response_message(&body, status);
        match status {
            400 | 404 => Err(RemoteError::BadRequest(format!("{}: {}", remote.name, message))),
            401 => Err(RemoteError::Failed(format!(
                "{}: token rejected — create a new token on that computer",
                remote.name
            ))),
            429 => Err(RemoteError::Failed(format!(
                "{}: too many wrong tokens, try again later",
                remote.name
            ))),
            _ => Err(RemoteError::Failed(format!("{}: {}", remote.name, message))),
        }
    }
}

fn with_auth(
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    remote: &Remote,
) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
    request
        .header("Authorization", format!("Bearer {}", remote.token))
        .header(HOP_HEADER, "1")
}

fn with_auth_no_body(
    request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    remote: &Remote,
) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    request
        .header("Authorization", format!("Bearer {}", remote.token))
        .header(HOP_HEADER, "1")
}

pub fn fetch_printers(remote: &Remote) -> Result<Vec<printer::Printer>, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(3)).get(format!("{}/list-printer", remote.url)),
        remote,
    )
    .call()
    .map_err(|e| request_error(remote, e))?;
    let views: Vec<RemotePrinterView> = read_response(remote, response)?;
    Ok(views
        .into_iter()
        .filter(|p| p.remote.is_none())
        .map(|p| printer::Printer {
            name: p.name,
            status: p.status,
            message: p.message,
            shared: p.shared,
            os_default: p.os_default,
        })
        .collect())
}

pub fn capabilities(remote: &Remote, name: &str) -> Result<printer::Capabilities, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/printer-capabilities", remote.url)),
        remote,
    )
    .query("printer", name)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

pub fn queue(remote: &Remote, name: &str) -> Result<printer::PrinterQueue, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/queue", remote.url)),
        remote,
    )
    .query("printer", name)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

pub fn job_status(remote: &Remote, job_id: &str) -> Result<Value, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/job-status", remote.url)),
        remote,
    )
    .query("job_id", job_id)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

pub fn print_pdf(
    remote: &Remote,
    path: &Path,
    file_name: &str,
    settings: &PrintSettings,
) -> Result<RemotePrintResult, RemoteError> {
    let size = fs::metadata(path).map_err(|e| request_error(remote, e))?.len();
    if size > crate::server::MAX_UPLOAD_BYTES as u64 {
        return Err(RemoteError::BadRequest("PDF file is too large".into()));
    }
    let bytes = fs::read(path).map_err(|e| request_error(remote, e))?;
    let boundary = format!("ixprint-{}", uuid::Uuid::new_v4().simple());
    let safe_name = sanitize_filename::sanitize(file_name);
    let mut body = Vec::with_capacity(bytes.len() + safe_name.len() + 160);
    body.extend_from_slice(format!("--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/pdf\r\n\r\n", boundary, safe_name).as_bytes());
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    let mut request = with_auth(
        agent(Duration::from_secs(120)).post(format!("{}/print", remote.url)),
        remote,
    )
    .header("Content-Type", format!("multipart/form-data; boundary={}", boundary));
    if let Some(preset) = &remote.preset {
        request = request.query("preset", preset);
    }
    for (key, value) in settings_query(settings) {
        request = request.query(key, value);
    }
    let response = request.send(body).map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

pub fn test_page(remote: &Remote, pages: u32, settings: &PrintSettings) -> Result<RemotePrintResult, RemoteError> {
    let mut request = with_auth(
        agent(Duration::from_secs(60)).post(format!("{}/test-page", remote.url)),
        remote,
    )
    .query("count", pages.to_string());
    if let Some(preset) = &remote.preset {
        request = request.query("preset", preset);
    }
    for (key, value) in settings_query(settings) {
        request = request.query(key, value);
    }
    let response = request.send_empty().map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Margins, Paper, Scale};

    fn remote(name: &str) -> Remote {
        Remote {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            url: "http://pc2.local:18032".into(),
            token: "fxp_test-token".into(),
            preset: None,
            created_at: chrono::Local::now().to_rfc3339(),
        }
    }

    #[test]
    fn normalizes_urls_and_rejects_paths() {
        assert_eq!(normalize_url("http://192.168.1.7").unwrap(), "http://192.168.1.7:18032");
        assert_eq!(
            normalize_url("HTTP://PC2.local:18032/").unwrap(),
            "http://pc2.local:18032"
        );
        assert_eq!(
            normalize_url("https://print.contoh.id").unwrap(),
            "https://print.contoh.id"
        );
        for value in ["192.168.1.7", "http://a/b", "http://u:p@a", "ftp://a"] {
            assert!(normalize_url(value).is_err(), "{}", value);
        }
    }

    #[test]
    fn validates_and_resolves_names() {
        assert!(validate_name("PC2").is_ok());
        assert!(validate_name("Gudang Lt 2").is_ok());
        for value in ["", " PC2", "PC2 ", "PC/2", &"x".repeat(33)] {
            assert!(validate_name(value).is_err(), "{}", value);
        }
        let r = remote("PC2");
        assert_eq!(
            resolve(&["PC2 / EPSON".into()], std::slice::from_ref(&r), "PC2 / EPSON"),
            Target::Local("PC2 / EPSON".into())
        );
        assert!(
            matches!(resolve(&[], std::slice::from_ref(&r), "PC2 / EPSON"), Target::Remote { printer, .. } if printer == "EPSON")
        );
        assert_eq!(resolve(&[], std::slice::from_ref(&r), "PC2 / "), Target::Unknown);
        assert_eq!(resolve(&[], std::slice::from_ref(&r), "pc2 / EPSON"), Target::Unknown);
    }

    #[test]
    fn settings_round_trip_to_query() {
        let settings = PrintSettings {
            copies: Some(2),
            paper: Some(Paper::Roll { width_mm: 80.0 }),
            offset_x: Some(1.5),
            margins: Some(Margins {
                top: 5.0,
                right: 3.0,
                bottom: 5.0,
                left: 3.0,
            }),
            collate: Some(false),
            scale: Some(Scale::Percent(90.0)),
            ..Default::default()
        };
        let params = settings_query(&settings).into_iter().collect();
        let (parsed, preset) = PrintSettings::from_query(&params).unwrap();
        assert_eq!(parsed, settings);
        assert!(preset.is_none());
    }

    #[test]
    fn store_survives_reload_and_rejects_duplicates() {
        let dir = std::env::temp_dir().join(format!("ixprint-remote-{}", uuid::Uuid::new_v4()));
        let store = RemoteStore::load(&dir);
        store
            .update(|list| {
                list.push(remote("PC2"));
                Ok(())
            })
            .unwrap();
        assert_eq!(RemoteStore::load(&dir).list().len(), 1);
        assert!(store
            .update(|list| {
                list.push(remote("pc2"));
                Ok(())
            })
            .is_err());
        fs::write(dir.join("remotes.json"), "broken").unwrap();
        assert!(RemoteStore::load(&dir).list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
