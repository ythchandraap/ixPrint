//! Remote bridge operations retained here until their use cases and network client move in L4.8.

use crate::config::Preset;
pub use crate::domain::remote::*;
use crate::domain::settings::{Align, Margins, Orientation, PrintSettings, Scale};
use crate::history::{HistoryEntry, RemoteJob};
pub use crate::infrastructure::persistence::remote_store::{RemoteCache, RemoteStore};
use crate::job::{AppState, Document, JobError, JobRequest, JobResult};
use crate::printer;
use crate::shared::sync::LockExt;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(5);

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

fn map_printers(remote: &Remote, printers: &[printer::Printer], offline: Option<&str>) -> Vec<printer::Printer> {
    printers
        .iter()
        .cloned()
        .map(|mut p| {
            p.name = format!("{}{}{}", remote.name, SEPARATOR, p.name);
            p.os_default = false;
            if let Some(error) = offline {
                p.status = "Offline".into();
                p.message = Some(error.to_string());
            }
            p
        })
        .collect()
}

pub fn printers(state: &AppState, remote: &Remote) -> (Vec<printer::Printer>, Option<String>) {
    refresh_printers(state, remote, false)
}

pub fn refresh_printers(state: &AppState, remote: &Remote, force: bool) -> (Vec<printer::Printer>, Option<String>) {
    if !force {
        if let Some(entry) = state.remote_cache.lock_safe().get(&remote.id) {
            if entry.fetched.elapsed() < CACHE_TTL {
                return (
                    map_printers(
                        remote,
                        &entry.printers,
                        (!entry.ok_once).then_some(entry.error.as_deref().unwrap_or("Remote has not been reached")),
                    ),
                    entry.error.clone(),
                );
            }
        }
    }
    match fetch_printers(remote) {
        Ok(list) => {
            let mut cache = state.remote_cache.lock_safe();
            cache.insert(
                remote.id.clone(),
                CacheEntry {
                    fetched: Instant::now(),
                    printers: list.clone(),
                    error: None,
                    ok_once: true,
                },
            );
            (map_printers(remote, &list, None), None)
        }
        Err(error) => {
            let message = error.to_string();
            let mut cache = state.remote_cache.lock_safe();
            let entry = cache.entry(remote.id.clone()).or_insert_with(|| CacheEntry {
                fetched: Instant::now(),
                printers: Vec::new(),
                error: None,
                ok_once: false,
            });
            entry.fetched = Instant::now();
            entry.error = Some(message.clone());
            (map_printers(remote, &entry.printers, Some(&message)), Some(message))
        }
    }
}

pub fn view(remote: &Remote, cache: &RemoteCache) -> RemoteView {
    let cache = cache.lock_safe();
    let entry = cache.get(&remote.id);
    RemoteView {
        id: remote.id.clone(),
        name: remote.name.clone(),
        url: remote.url.clone(),
        preset: remote.preset.clone(),
        token_hint: format!("{}…", &remote.token[..remote.token.len().min(10)]),
        created_at: remote.created_at.clone(),
        online: entry.map(|e| e.ok_once && e.error.is_none()),
        last_error: entry.and_then(|e| e.error.clone()),
        printers: entry.map(|e| e.printers.len()).unwrap_or(0),
    }
}

pub fn views(store: &RemoteStore, cache: &RemoteCache) -> Vec<RemoteView> {
    store.list().iter().map(|r| view(r, cache)).collect()
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

pub(crate) struct ForwardRequest<'a> {
    pub(crate) job_id: &'a str,
    pub(crate) preset_name: Option<&'a str>,
    pub(crate) preset: &'a Preset,
    pub(crate) merged: &'a PrintSettings,
    pub(crate) req: &'a JobRequest,
    pub(crate) remote: &'a Remote,
    pub(crate) remote_printer: &'a str,
    pub(crate) record: &'a mut HistoryEntry,
}

pub(crate) fn forward(state: &AppState, request: ForwardRequest<'_>) -> Result<JobResult, JobError> {
    let ForwardRequest {
        job_id,
        preset_name,
        preset,
        merged,
        req,
        remote,
        remote_printer,
        record,
    } = request;
    let mut settings = merged.clone();
    settings.printer = Some(remote_printer.to_string());
    let generated = state.jobs_dir.join(format!("{}-remote.pdf", job_id));
    let result = (|| -> Result<RemotePrintResult, RemoteError> {
        match &req.document {
            Document::File(path) => print_pdf(remote, path, &req.file_name, &settings),
            Document::Data { records, header } => {
                let design = preset.design.as_ref().ok_or_else(|| {
                    RemoteError::BadRequest(match preset_name {
                        Some(name) => format!("Preset '{}' has no design; send a PDF instead of JSON data", name),
                        None => "Specify a preset with a design in the \"preset\" field".into(),
                    })
                })?;
                if records.len() > crate::template::MAX_RECORDS {
                    return Err(RemoteError::BadRequest(format!(
                        "at most {} records per job",
                        crate::template::MAX_RECORDS
                    )));
                }
                crate::template::render(&generated, design, records, header).map_err(RemoteError::BadRequest)?;
                design_settings(&mut settings, design, false);
                print_pdf(remote, &generated, &req.file_name, &settings)
            }
            Document::TestPage { pages } if preset.design.is_some() => {
                let design = preset.design.as_ref().unwrap();
                let records = vec![design.sample.clone(); (*pages).clamp(1, crate::testpage::MAX_TEST_PAGES) as usize];
                crate::template::render(&generated, design, &records, &design.sample)
                    .map_err(RemoteError::BadRequest)?;
                design_settings(&mut settings, design, true);
                print_pdf(remote, &generated, &req.file_name, &settings)
            }
            Document::TestPage { pages } => test_page(remote, *pages, &settings),
        }
    })();
    let _ = fs::remove_file(&generated);
    let result = result.map_err(RemoteError::into_job_error)?;
    record.settings = serde_json::to_value(&settings).unwrap_or_default();
    record.sheets = result.sheets;
    record.remote = Some(RemoteJob {
        remote_id: remote.id.clone(),
        remote_name: remote.name.clone(),
        job_id: result.job_id.clone(),
    });
    Ok(JobResult {
        job_id: job_id.to_string(),
        printer: format!("{}{}{}", remote.name, SEPARATOR, remote_printer),
        preset: preset_name.map(str::to_string),
        sheets: result.sheets,
        spooler_job: None,
    })
}

fn design_settings(settings: &mut PrintSettings, design: &crate::template::Template, test: bool) {
    settings.paper = Some(crate::job::design_paper(design));
    settings.scale = Some(Scale::None);
    settings.margins = Some(Margins::default());
    settings.align = Some(Align::Center);
    settings.orientation = Some(Orientation::Portrait);
    settings.pages = None;
    if test {
        settings.copies = Some(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Margins, Paper};

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
