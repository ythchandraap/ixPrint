use super::super::blocking;
use super::super::AppStateRef;
use crate::application::state::AppState;
use crate::domain::job::{Document, JobRequest, JobResult};
use crate::domain::settings::PrintSettings;
use std::path::PathBuf;
use std::time::Duration;
/// Sends the desktop form through the same authenticated HTTP endpoint used by
/// external clients. The process-only key never leaves Rust or appears in logs.
#[tauri::command]
pub async fn app_print(
    state: AppStateRef<'_>,
    json_body: Option<String>,
    pdf_path: Option<String>,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<JobResult, String> {
    let state = state.inner().clone();
    if !state.config.get().bridge_enabled {
        return Err("Print service is disabled. Turn on the bridge in the status bar, then try again.".into());
    }
    if json_body.is_some() == pdf_path.is_some() {
        return Err("Choose exactly one source: JSON data or a PDF file.".into());
    }
    blocking(move || submit_to_print_endpoint(&state, json_body, pdf_path, preset, settings)).await?
}

fn submit_to_print_endpoint(
    state: &AppState,
    json_body: Option<String>,
    pdf_path: Option<String>,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<JobResult, String> {
    let port = state.config.get().server_port;
    let endpoint = format!("http://127.0.0.1:{}/print", port);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("ix.print/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let mut response = if let Some(body) = json_body {
        agent
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("X-IxPrint-Desktop", &state.desktop_key)
            .send(body)
    } else if let Some(path) = pdf_path {
        let path = PathBuf::from(path);
        if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
            return Err("Choose a PDF file with the .pdf extension.".into());
        }
        let mut params = Vec::new();
        if let Some(preset) = preset.filter(|p| !p.trim().is_empty()) {
            params.push(("preset", preset));
        }
        let settings_json = serde_json::to_value(&settings).map_err(|_| "Could not read print settings".to_string())?;
        if let Some(values) = settings_json.as_object() {
            for (name, value) in values {
                if value.is_null() { continue; }
                let value = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
                params.push((name.as_str(), value));
            }
        }
        let query = params
            .iter()
            .map(|(name, value)| format!("{}={}", encode_query(name), encode_query(value)))
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() { endpoint } else { format!("{}?{}", endpoint, query) };
        let form = ureq::unversioned::multipart::Form::new()
            .file("file", &path)
            .map_err(|_| format!("Could not read PDF file: {}", path.file_name().unwrap_or_default().to_string_lossy()))?;
        agent
            .post(&url)
            .header("X-IxPrint-Desktop", &state.desktop_key)
            .send(form)
    } else {
        return Err("Choose JSON data or a PDF file before printing.".into());
    }
    .map_err(|_| format!("Could not reach the local print service on port {}. Check that the bridge is running; check History before retrying in case the job was already submitted.", port))?;

    let status = response.status();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    if !status.is_success() {
        let message = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("The print service rejected the request");
        return Err(if status.as_u16() == 401 {
            "The desktop print request was denied. Restart ix.print and try again.".into()
        } else {
            format!("Print request failed (HTTP {}): {}", status, message)
        });
    }
    serde_json::from_value(value)
        .map_err(|_| "The print service returned an unreadable job result. Check History before retrying.".into())
}

fn encode_query(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

/// PNG data URL of the first sheet a PDF file would print as, laid out with
/// `layout::build` — the exact same code path the real print job uses — so
/// the preview matches the printed output 1:1.
#[tauri::command]
pub async fn preview_pdf_sheet(
    state: AppStateRef<'_>,
    pdf_path: String,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<String, String> {
    let state = state.inner().clone();
    blocking(move || {
        crate::application::preset_service::preview_pdf_sheet(&state, &pdf_path, preset.as_deref(), &settings)
    })
    .await?
}

/// Prints `pages` edge-to-edge calibration pages at the sheet size of the preset/printer.
#[tauri::command]
pub async fn test_print(
    state: AppStateRef<'_>,
    printer: Option<String>,
    preset: Option<String>,
    pages: Option<u32>,
) -> Result<JobResult, String> {
    let state = state.inner().clone();
    let request = JobRequest {
        job_id: None,
        document: Document::TestPage {
            pages: pages.unwrap_or(1),
        },
        file_name: "ix.print test page.pdf".into(),
        settings: PrintSettings {
            printer,
            ..Default::default()
        },
        preset: preset.filter(|p| !p.is_empty()),
        source: "app",
        client: None,
        token: None,
        hop: false,
    };
    blocking(move || crate::application::job_service::run(&state, request))
        .await?
        .map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::application::state::test_support::state;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn mock_endpoint() -> (u16, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 8192];
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..split]).to_ascii_lowercase();
                let body = &request[split + 4..];
                let complete = if let Some(len) = headers.lines().find_map(|line| {
                    line.strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                }) {
                    body.len() >= len
                } else {
                    body.windows(5).any(|w| w == b"0\r\n\r\n")
                };
                if complete {
                    break;
                }
            }
            let body = br#"{"job_id":"mock-job","printer":"Mock printer","sheets":3}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            request
        });
        (port, server)
    }

    fn headers_and_body(request: &[u8]) -> (String, String) {
        let split = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (
            String::from_utf8_lossy(&request[..split]).to_string(),
            String::from_utf8_lossy(&request[split + 4..]).to_string(),
        )
    }

    #[test]
    fn desktop_print_posts_json_and_pdf_with_requested_copies() {
        for copies in [1, 4] {
            let (port, server) = mock_endpoint();
            let state = state();
            let mut set_port = |config: &mut crate::domain::settings_config::Config| {
                config.server_port = port;
                Ok(())
            };
            state.config.update(&mut set_port).unwrap();
            let body = format!(
                r#"{{"preset":"label","data":{{"name":"Example"}},"settings":{{"copies":{}}}}}"#,
                copies
            );
            let result = submit_to_print_endpoint(&state, Some(body), None, None, PrintSettings::default()).unwrap();
            let (headers, payload) = headers_and_body(&server.join().unwrap());
            assert_eq!(result.job_id, "mock-job");
            assert_eq!(result.sheets, 3);
            assert!(headers.starts_with("POST /print HTTP/1.1"));
            assert!(headers.to_ascii_lowercase().contains("content-type: application/json"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("x-ixprint-desktop: test-desktop-key"));
            assert!(payload.contains(&format!("\"copies\":{}", copies)));
        }

        let dir = std::env::temp_dir().join(format!("ixprint-pdf-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("source.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\nmock document").unwrap();
        for copies in [1, 4] {
            let (port, server) = mock_endpoint();
            let state = state();
            let mut set_port = |config: &mut crate::domain::settings_config::Config| {
                config.server_port = port;
                Ok(())
            };
            state.config.update(&mut set_port).unwrap();
            let settings = PrintSettings {
                copies: Some(copies),
                printer: Some("Mock Printer / 1".into()),
                ..Default::default()
            };
            let result = submit_to_print_endpoint(
                &state,
                None,
                Some(pdf.to_string_lossy().into_owned()),
                Some("receipt".into()),
                settings,
            )
            .unwrap();
            let (headers, payload) = headers_and_body(&server.join().unwrap());
            assert_eq!(result.job_id, "mock-job");
            assert!(headers.starts_with("POST /print?"));
            assert!(headers.contains("preset=receipt"));
            assert!(headers.contains(&format!("copies={}", copies)));
            assert!(headers.contains("printer=Mock%20Printer%20%2F%201"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("content-type: multipart/form-data; boundary="));
            assert!(headers
                .to_ascii_lowercase()
                .contains("x-ixprint-desktop: test-desktop-key"));
            assert!(payload.contains("filename=\"source.pdf\""));
            assert!(payload.contains("%PDF-1.4"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
