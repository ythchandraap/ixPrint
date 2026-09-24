//! Connections to other ix.print computers.
//!
//! Remote credentials live in `remotes.json`, never in the configuration sent
//! to the webview. Forwarding is deliberately limited to one hop.

use crate::auth;
use crate::config::Preset;
use crate::history::{HistoryEntry, RemoteJob};
use crate::job::{AppState, Document, JobError, JobRequest, JobResult};
use crate::printer;
use crate::settings::{Align, Margins, Orientation, PrintSettings, Scale};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

pub const SEPARATOR: &str = " / ";
pub const HOP_HEADER: &str = "X-IxPrint-Hop";
const MAX_REMOTES: usize = 20;
const MAX_NAME: usize = 32;
const CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub id: String,
    pub name: String,
    pub url: String,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RemoteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub preset: Option<String>,
    pub token_hint: String,
    pub created_at: String,
    pub online: Option<bool>,
    pub last_error: Option<String>,
    pub printers: usize,
}

pub struct RemoteStore {
    path: PathBuf,
    data: RwLock<Vec<Remote>>,
}

pub struct CacheEntry {
    pub fetched: Instant,
    pub printers: Vec<printer::Printer>,
    pub error: Option<String>,
    pub ok_once: bool,
}

pub type RemoteCache = Mutex<HashMap<String, CacheEntry>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Local(String),
    Remote { remote: Remote, printer: String },
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteError {
    BadRequest(String),
    Failed(String),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadRequest(message) | Self::Failed(message) => message,
        })
    }
}

impl RemoteError {
    pub fn into_job_error(self) -> JobError {
        match self {
            Self::BadRequest(message) => JobError::BadRequest(message),
            Self::Failed(message) => JobError::Failed(message),
        }
    }

    pub fn is_bad_request(&self) -> bool {
        matches!(self, Self::BadRequest(_))
    }
}

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

#[derive(Deserialize)]
pub struct RemotePrintResult {
    pub job_id: String,
    pub sheets: u32,
}

impl RemoteStore {
    pub fn load(dir: &Path) -> RemoteStore {
        let path = dir.join("remotes.json");
        let list = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Vec<Remote>>(&text) {
                Ok(list) => match validate_all(&list) {
                    Ok(()) => list,
                    Err(e) => {
                        eprintln!("remotes.json is invalid: {}", e);
                        Vec::new()
                    }
                },
                Err(e) => {
                    eprintln!("remotes.json is corrupted: {}", e);
                    Vec::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                eprintln!("remotes.json could not be read: {}", e);
                Vec::new()
            }
        };
        RemoteStore {
            path,
            data: RwLock::new(list),
        }
    }

    pub fn list(&self) -> Vec<Remote> {
        self.data.read().unwrap().clone()
    }

    pub fn update<F>(&self, change: F) -> Result<Vec<Remote>, String>
    where
        F: FnOnce(&mut Vec<Remote>) -> Result<(), String>,
    {
        let mut guard = self.data.write().unwrap();
        let mut next = guard.clone();
        change(&mut next)?;
        validate_all(&next)?;
        write_atomic(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

fn validate_all(remotes: &[Remote]) -> Result<(), String> {
    if remotes.len() > MAX_REMOTES {
        return Err(format!("At most {} other ix.print computers are allowed", MAX_REMOTES));
    }
    let mut names = HashSet::new();
    for remote in remotes {
        validate_name(&remote.name)?;
        if !names.insert(remote.name.to_ascii_lowercase()) {
            return Err(format!("Remote name '{}' is already used", remote.name));
        }
        if normalize_url(&remote.url)? != remote.url {
            return Err(format!("Remote '{}' address is not normalised", remote.name));
        }
        validate_token(&remote.token)?;
    }
    Ok(())
}

fn write_atomic(path: &Path, remotes: &[Remote]) -> Result<(), String> {
    let dir = path.parent().ok_or("Invalid remotes path")?;
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create the remotes folder: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(remotes).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("Failed to save remotes: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(tmp, path).map_err(|e| format!("Failed to save remotes: {}", e))
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed != name
        || trimmed.is_empty()
        || trimmed.chars().count() > MAX_NAME
        || trimmed.contains('/')
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err("Remote name must be 1–32 characters without '/'".into());
    }
    Ok(())
}

pub fn normalize_url(url: &str) -> Result<String, String> {
    let input = url.trim().trim_end_matches('/');
    let bad = || format!("Remote address '{}' is invalid", url.trim());
    if input.is_empty() || input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let Some((scheme, authority)) = input.split_once("://") else {
        return Err(bad());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" || authority.is_empty() || authority.contains(['/', '?', '#', '@']) {
        return Err(bad());
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(bad)?;
        let host = &authority[..=end];
        let rest = &authority[end + 1..];
        let port = if rest.is_empty() {
            None
        } else {
            Some(
                rest.strip_prefix(':')
                    .ok_or_else(bad)?
                    .parse::<u16>()
                    .map_err(|_| bad())?,
            )
        };
        (host, port)
    } else if authority.matches(':').count() > 1 {
        return Err(bad());
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, Some(port.parse::<u16>().map_err(|_| bad())?))
    } else {
        (authority, None)
    };
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || port == Some(0) {
        return Err(bad());
    }
    let authority = match port {
        Some(port) => format!("{}:{}", host.to_ascii_lowercase(), port),
        None if scheme == "http" => format!("{}:18032", host.to_ascii_lowercase()),
        None => host.to_ascii_lowercase(),
    };
    Ok(format!("{}://{}", scheme, authority))
}

pub fn validate_token(token: &str) -> Result<(), String> {
    if token.trim() != token || !token.starts_with(auth::TOKEN_PREFIX) || token.len() > 200 {
        return Err("Remote token must start with fxp_ and be at most 200 characters".into());
    }
    Ok(())
}

pub fn resolve(local: &[String], remotes: &[Remote], name: &str) -> Target {
    if local.iter().any(|local_name| local_name == name) {
        return Target::Local(name.to_string());
    }
    remotes
        .iter()
        .find_map(|remote| {
            let prefix = format!("{}{}", remote.name, SEPARATOR);
            name.strip_prefix(&prefix)
                .filter(|printer| !printer.is_empty())
                .map(|printer| Target::Remote {
                    remote: remote.clone(),
                    printer: printer.to_string(),
                })
        })
        .unwrap_or(Target::Unknown)
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

fn with_auth<'a>(
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    remote: &'a Remote,
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
        if let Some(entry) = state.remote_cache.lock().unwrap().get(&remote.id) {
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
            let mut cache = state.remote_cache.lock().unwrap();
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
            let mut cache = state.remote_cache.lock().unwrap();
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
    let cache = cache.lock().unwrap();
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

pub fn settings_query(settings: &PrintSettings) -> Vec<(String, String)> {
    let value = serde_json::to_value(settings).unwrap_or(Value::Null);
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(key, value)| match value {
            Value::String(value) => Some((key.clone(), value.clone())),
            Value::Number(value) => Some((key.clone(), value.to_string())),
            Value::Bool(value) => Some((key.clone(), value.to_string())),
            _ => None,
        })
        .collect()
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

pub fn forward(
    state: &AppState,
    job_id: &str,
    preset_name: Option<&str>,
    preset: &Preset,
    merged: &PrintSettings,
    req: &JobRequest,
    remote: &Remote,
    remote_printer: &str,
    record: &mut HistoryEntry,
) -> Result<JobResult, JobError> {
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
