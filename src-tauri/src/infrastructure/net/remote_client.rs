use crate::application::ports::RemoteGateway;
use crate::domain::printer;
use crate::domain::remote::{
    settings_query, Remote, RemoteError, RemotePrintResult, SharedPreset, SharedPresetListing, HOP_HEADER,
};
use crate::domain::settings::PrintSettings;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;

const MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;
const MAX_SHARED_BUNDLE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_SHARED_LIST_BYTES: u64 = 1024 * 1024;

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

fn read_limited_body(
    remote: &Remote,
    response: &mut ureq::http::Response<ureq::Body>,
    limit: u64,
) -> Result<Vec<u8>, RemoteError> {
    if response.body().content_length().is_some_and(|length| length > limit) {
        return Err(RemoteError::Failed(format!(
            "{} returned a response larger than allowed",
            remote.name
        )));
    }
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => {
                RemoteError::Failed(format!("{} returned a response larger than allowed", remote.name))
            }
            other => request_error(remote, other),
        })
}

fn shared_error(remote: &Remote, status: u16) -> RemoteError {
    match status {
        400 => RemoteError::BadRequest(format!("{} rejected the preset request", remote.name)),
        401 => RemoteError::Failed(format!(
            "{}: API token rejected — create a new token on that computer",
            remote.name
        )),
        404 => RemoteError::BadRequest(format!("{}: preset is no longer available", remote.name)),
        429 => RemoteError::Failed(format!("{}: too many incorrect tokens, try again later", remote.name)),
        _ => RemoteError::Failed(format!(
            "{}: shared preset request failed (HTTP {})",
            remote.name, status
        )),
    }
}

fn shared_request_error(remote: &Remote) -> RemoteError {
    RemoteError::Failed(format!(
        "{} cannot be reached at {}. Check its address and LAN connection.",
        remote.name, remote.url
    ))
}

#[derive(Deserialize)]
struct SharedPresetsBody {
    presets: Vec<SharedPreset>,
}

fn shared_presets(remote: &Remote) -> Result<SharedPresetListing, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(10)).get(format!("{}/presets/shared", remote.url)),
        remote,
    )
    .call()
    .map_err(|_| shared_request_error(remote))?;
    let status = response.status().as_u16();
    if status == 403 {
        return Ok(SharedPresetListing {
            listing_enabled: false,
            presets: Vec::new(),
        });
    }
    if !(200..300).contains(&status) {
        return Err(shared_error(remote, status));
    }
    let mut response = response;
    let bytes = read_limited_body(remote, &mut response, MAX_SHARED_LIST_BYTES)?;
    let body: SharedPresetsBody = serde_json::from_slice(&bytes)
        .map_err(|_| RemoteError::Failed(format!("{} returned an invalid shared preset list", remote.name)))?;
    Ok(SharedPresetListing {
        listing_enabled: true,
        presets: body.presets,
    })
}

fn fetch_shared_preset(remote: &Remote, name: &str) -> Result<Vec<u8>, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(30)).get(format!("{}/presets/shared/export", remote.url)),
        remote,
    )
    .query("name", name)
    .call()
    .map_err(|_| shared_request_error(remote))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(shared_error(remote, status));
    }
    let mut response = response;
    read_limited_body(remote, &mut response, MAX_SHARED_BUNDLE_BYTES)
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

fn fetch_printers(remote: &Remote) -> Result<Vec<printer::Printer>, RemoteError> {
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

fn capabilities(remote: &Remote, name: &str) -> Result<printer::Capabilities, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/printer-capabilities", remote.url)),
        remote,
    )
    .query("printer", name)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

fn queue(remote: &Remote, name: &str) -> Result<printer::PrinterQueue, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/queue", remote.url)),
        remote,
    )
    .query("printer", name)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

fn job_status(remote: &Remote, job_id: &str) -> Result<Value, RemoteError> {
    let response = with_auth_no_body(
        agent(Duration::from_secs(5)).get(format!("{}/job-status", remote.url)),
        remote,
    )
    .query("job_id", job_id)
    .call()
    .map_err(|e| request_error(remote, e))?;
    read_response(remote, response)
}

fn print_pdf(
    remote: &Remote,
    path: &Path,
    file_name: &str,
    settings: &PrintSettings,
) -> Result<RemotePrintResult, RemoteError> {
    let size = fs::metadata(path).map_err(|e| request_error(remote, e))?.len();
    if size > MAX_UPLOAD_BYTES as u64 {
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

fn test_page(remote: &Remote, pages: u32, settings: &PrintSettings) -> Result<RemotePrintResult, RemoteError> {
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

#[derive(Default)]
pub struct RemoteClient;

impl RemoteGateway for RemoteClient {
    fn shared_presets(&self, remote: &Remote) -> Result<SharedPresetListing, RemoteError> {
        self::shared_presets(remote)
    }
    fn fetch_shared_preset(&self, remote: &Remote, name: &str) -> Result<Vec<u8>, RemoteError> {
        self::fetch_shared_preset(remote, name)
    }
    fn fetch_printers(&self, remote: &Remote) -> Result<Vec<printer::Printer>, RemoteError> {
        self::fetch_printers(remote)
    }
    fn capabilities(&self, remote: &Remote, name: &str) -> Result<printer::Capabilities, RemoteError> {
        self::capabilities(remote, name)
    }
    fn queue(&self, remote: &Remote, name: &str) -> Result<printer::PrinterQueue, RemoteError> {
        self::queue(remote, name)
    }
    fn job_status(&self, remote: &Remote, job_id: &str) -> Result<Value, RemoteError> {
        self::job_status(remote, job_id)
    }
    fn print_pdf(
        &self,
        remote: &Remote,
        path: &Path,
        file_name: &str,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError> {
        self::print_pdf(remote, path, file_name, settings)
    }
    fn test_page(
        &self,
        remote: &Remote,
        pages: u32,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError> {
        self::test_page(remote, pages, settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn remote(url: String) -> Remote {
        Remote {
            id: "remote-id".into(),
            name: "Source".into(),
            url,
            token: "fxp_never-log-this-token".into(),
            preset: None,
            created_at: String::new(),
        }
    }

    fn serve_once(response: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{}", address)
    }

    #[test]
    fn shared_preset_client_does_not_follow_redirects_and_caps_response_size() {
        let redirect = serve_once(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );
        let error = RemoteClient
            .fetch_shared_preset(&remote(redirect), "Label")
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 302"));

        let oversized = serve_once(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_SHARED_BUNDLE_BYTES + 1
        ));
        let error = RemoteClient
            .fetch_shared_preset(&remote(oversized), "Label")
            .unwrap_err();
        assert!(error.to_string().contains("larger than allowed"));
    }

    #[test]
    fn shared_preset_client_never_displays_token_from_remote_error_body() {
        let body = r#"{"message":"fxp_never-log-this-token"}"#;
        let base = serve_once(format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));
        let error = RemoteClient.shared_presets(&remote(base)).unwrap_err().to_string();
        assert!(error.contains("token rejected"));
        assert!(!error.contains("fxp_never-log-this-token"));

        let unavailable = RemoteClient
            .shared_presets(&remote("http://127.0.0.1:1".into()))
            .unwrap_err()
            .to_string();
        assert!(unavailable.contains("cannot be reached"));
        assert!(!unavailable.contains("fxp_never-log-this-token"));
    }
}
