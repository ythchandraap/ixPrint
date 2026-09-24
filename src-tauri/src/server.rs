//! HTTP API for web apps on the network.
//!
//! Read-only apart from `/print`: presets and the default printer can only be
//! changed from the ix.print window, because this port is open to the network.

use crate::auth;
use crate::callback::{self, Callback};
use crate::config::PresetBundle;
use crate::job::{self, AppState, Document, JobError, JobRequest};
use crate::printer;
use crate::settings::PrintSettings;
use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::middleware::DefaultHeaders;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

pub(crate) const MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;
/// PDF readers accept the `%PDF-` header anywhere in the first 1024 bytes.
const PDF_HEADER_WINDOW: usize = 1024;

type State = web::Data<Arc<AppState>>;

#[derive(Serialize)]
struct Message<'a> {
    message: &'a str,
}

fn error(status: actix_web::http::StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(Message { message })
}

/// Logs an internal failure and answers without its details (paths, OS errors).
fn internal(e: impl std::fmt::Display) -> HttpResponse {
    eprintln!("[ix.print] internal error: {}", e);
	error(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "An internal error occurred in ix.print")
}

fn unauthorized(message: &str) -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Bearer"))
        .json(Message { message })
}

/// Checks the API token (`Authorization: Bearer`, `X-Api-Token` or `?token=`).
/// Returns the token when one was sent; `None` when tokens are optional
/// and none was sent. A wrong token is always rejected, to surface mistakes.
fn authorize(req: &HttpRequest, state: &AppState) -> Result<Option<auth::ApiToken>, HttpResponse> {
    let ip = req.peer_addr().map(|a| a.ip());
    let desktop_key = req.headers().get("x-ixprint-desktop").and_then(|v| v.to_str().ok());
    if req.method() == actix_web::http::Method::POST
        && req.path() == "/print"
        && ip.is_some_and(|ip| ip.is_loopback())
        && desktop_key.is_some_and(|k| crate::auth::ct_eq(k.as_bytes(), state.desktop_key.as_bytes()))
    {
        return Ok(None);
    }
    if let Some(wait) = ip.and_then(|ip| state.failed_auth.locked(ip)) {
        return Err(HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", wait.as_secs().max(1).to_string()))
			.json(Message { message: "Too many incorrect tokens from this address; try again later" }));
    }
    let config = state.config.get();
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
    let query = web::Query::<HashMap<String, String>>::from_query(req.query_string()).ok();
    let provided = auth::extract(
        header("authorization"),
        header("x-api-token"),
        query.as_ref().and_then(|q| q.get("token")).map(String::as_str),
    );
    match provided {
        Some(token) => match auth::find(&config.tokens, &token) {
            Some(t) => {
                if let Some(ip) = ip {
                    state.failed_auth.succeed(ip);
                }
                state.token_usage.record(&t.id, req.peer_addr().map(|a| a.ip().to_string()));
                Ok(Some(t.clone()))
            }
            None => {
                if let Some(ip) = ip {
                    state.failed_auth.fail(ip);
                }
				Err(unauthorized("Token is invalid or has been revoked"))
            }
        },
        None if config.require_token && config.tokens.is_empty() => Err(unauthorized(
			"A token is required, but none exists: create one in ix.print → Settings → API",
        )),
        None if config.require_token => Err(unauthorized(
			"A token is required: send the 'Authorization: Bearer <token>' header (create one in ix.print → API)",
        )),
        None => Ok(None),
    }
}

macro_rules! authorized {
    ($req:expr, $state:expr) => {
        match authorize(&$req, &$state) {
            Ok(name) => name,
            Err(resp) => return resp,
        }
    };
}

/// A printer as reported to clients; `is_default` is the printer ix.print uses
/// when nothing else is specified.
#[derive(Serialize)]
pub struct PrinterView {
    #[serde(flatten)]
    pub printer: printer::Printer,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

pub fn printer_views(state: &AppState, include_remote: bool) -> Vec<PrinterView> {
    let default = state.default_printer();
    let mut views: Vec<PrinterView> = printer::list_printers()
        .into_iter()
        .map(|p| PrinterView {
            is_default: default.as_deref() == Some(p.name.as_str()),
            printer: p,
            remote: None,
        })
        .collect();
    if include_remote {
        let remotes = state.remotes.list();
        let remote_views = std::thread::scope(|scope| {
            remotes
                .iter()
                .map(|remote| scope.spawn(|| crate::remote::printers(state, remote).0))
                .collect::<Vec<_>>()
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .map(|printer| PrinterView {
                    is_default: default.as_deref() == Some(printer.name.as_str()),
                    remote: remotes.iter().find_map(|r| printer.name.strip_prefix(&format!("{}{}", r.name, crate::remote::SEPARATOR)).map(|_| r.name.clone())),
                    printer,
                })
                .collect::<Vec<_>>()
        });
        views.extend(remote_views);
    }
    views
}

async fn hello() -> impl Responder {
    "Hello, world!"
}

const DOCS_HTML: &str = include_str!("docs.html");

/// URL other computers on the network use to reach this server.
pub fn lan_url(port: u16) -> Option<String> {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| format!("http://{}:{}", ip, port))
}

/// Interactive API documentation; its "try it" forms call this same server.
async fn docs(state: State) -> HttpResponse {
    let port = state.config.get().server_port;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header((
            "Content-Security-Policy",
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
             connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; \
             frame-ancestors 'none'",
        ))
        .body(DOCS_HTML.replace("{{LAN_URL}}", &lan_url(port).unwrap_or_default()))
}

async fn list_printers(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    let include_remote = !req.headers().contains_key(crate::remote::HOP_HEADER);
    let state = state.get_ref().clone();
    match web::block(move || printer_views(&state, include_remote)).await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(e) => internal(e),
    }
}

enum LookupError {
    NotFound(String),
    Remote(crate::remote::RemoteError),
}

fn remote_error_response(remote_error: crate::remote::RemoteError) -> HttpResponse {
    let status = if remote_error.is_bad_request() {
        actix_web::http::StatusCode::BAD_REQUEST
    } else {
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    };
    error(status, &remote_error.to_string())
}

async fn capabilities(req: HttpRequest, state: State, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    authorized!(req, state);
    let hop = req.headers().contains_key(crate::remote::HOP_HEADER);
    let state = state.get_ref().clone();
    let requested = query.get("printer").filter(|p| !p.trim().is_empty()).cloned();
    let Some(name) = requested.or_else(|| state.default_printer()) else {
		return error(actix_web::http::StatusCode::BAD_REQUEST, "parameter 'printer' is required");
    };
    match web::block(move || {
        let local_names = printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>();
        match crate::remote::resolve(&local_names, &state.remotes.list(), &name) {
            crate::remote::Target::Local(name) => printer::capabilities(&name).map_err(LookupError::NotFound),
            crate::remote::Target::Unknown => Err(LookupError::NotFound(format!("Printer '{}' was not found on this computer", name))),
            crate::remote::Target::Remote { .. } if hop => Err(LookupError::Remote(crate::remote::RemoteError::BadRequest("Forwarding between ix.print computers is limited to one hop".into()))),
            crate::remote::Target::Remote { remote, printer } => crate::remote::capabilities(&remote, &printer).map_err(LookupError::Remote),
        }
    }).await {
        Ok(Ok(caps)) => HttpResponse::Ok().json(caps),
        Ok(Err(LookupError::NotFound(e))) => error(actix_web::http::StatusCode::NOT_FOUND, &e),
        Ok(Err(LookupError::Remote(e))) => remote_error_response(e),
        Err(e) => internal(e),
    }
}

async fn queue(req: HttpRequest, state: State, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    authorized!(req, state);
    let hop = req.headers().contains_key(crate::remote::HOP_HEADER);
    let state = state.get_ref().clone();
    let requested = query.get("printer").filter(|p| !p.trim().is_empty()).cloned();
    let Some(name) = requested.or_else(|| state.default_printer()) else {
		return error(actix_web::http::StatusCode::BAD_REQUEST, "parameter 'printer' is required");
    };
    match web::block(move || {
        let local_names = printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>();
        match crate::remote::resolve(&local_names, &state.remotes.list(), &name) {
            crate::remote::Target::Local(name) => printer::queue(&name).map_err(LookupError::NotFound),
            crate::remote::Target::Unknown => Err(LookupError::NotFound(format!("Printer '{}' was not found on this computer", name))),
            crate::remote::Target::Remote { .. } if hop => Err(LookupError::Remote(crate::remote::RemoteError::BadRequest("Forwarding between ix.print computers is limited to one hop".into()))),
            crate::remote::Target::Remote { remote, printer } => {
                let mut queue = crate::remote::queue(&remote, &printer).map_err(LookupError::Remote)?;
                queue.printer = name.clone();
                Ok(queue)
            }
        }
    }).await {
        Ok(Ok(q)) => HttpResponse::Ok().json(q),
        Ok(Err(LookupError::NotFound(e))) => error(actix_web::http::StatusCode::NOT_FOUND, &e),
        Ok(Err(LookupError::Remote(e))) => remote_error_response(e),
        Err(e) => internal(e),
    }
}

/// Status of a job printed through ix.print: failed in ix.print, queued (with
/// position), printing, blocked by a printer error, or completed.
pub fn job_status_json(state: &AppState, job_id: &str) -> Option<serde_json::Value> {
    let entry = state.history.find(job_id)?;
    let base = serde_json::json!({
        "job_id": entry.id,
        "printer": entry.printer,
        "spooler_job": entry.spooler_job,
    });
    let status = if !entry.success {
        serde_json::json!({ "state": "failed", "message": entry.error })
    } else if let Some(remote_job) = &entry.remote {
        let mut status = match state.remotes.list().into_iter().find(|r| r.id == remote_job.remote_id) {
            Some(remote) => match crate::remote::job_status(&remote, &remote_job.job_id) {
                Ok(status) => status,
                Err(error) => serde_json::json!({ "state": "unknown", "message": error.to_string(), "remote_unreachable": true }),
            },
            None => serde_json::json!({ "state": "unknown", "message": format!("Remote '{}' was removed", remote_job.remote_name) }),
        };
        if let Some(object) = status.as_object_mut() {
            object.insert("job_id".into(), serde_json::json!(entry.id));
            object.insert("printer".into(), serde_json::json!(entry.printer));
            object.insert("remote".into(), serde_json::json!(remote_job.remote_name));
        }
        status
    } else {
        match entry.spooler_job.as_deref().map(|id| printer::job_status(&entry.printer, id)) {
            Some(Ok(s)) => serde_json::to_value(s).unwrap_or_default(),
            Some(Err(e)) => serde_json::json!({ "state": "unknown", "message": e }),
            None => serde_json::json!({ "state": "unknown" }),
        }
    };
    let mut merged = base;
    if let (Some(m), Some(s)) = (merged.as_object_mut(), status.as_object()) {
        m.extend(s.clone());
    }
    Some(merged)
}

async fn job_status(req: HttpRequest, state: State, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    authorized!(req, state);
    let Some(job_id) = query.get("job_id").cloned() else {
		return error(actix_web::http::StatusCode::BAD_REQUEST, "parameter 'job_id' is required");
    };
    let state = state.get_ref().clone();
    match web::block(move || job_status_json(&state, &job_id)).await {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
		Ok(None) => error(actix_web::http::StatusCode::NOT_FOUND, "job not found"),
        Err(e) => internal(e),
    }
}

/// Presets that have a design, with the fields each one expects.
pub fn templates_json(state: &AppState) -> serde_json::Value {
    let config = state.config.get();
    serde_json::Value::Array(
        config
            .presets
            .iter()
            .filter_map(|(name, p)| {
                let d = p.design.as_ref()?;
                Some(serde_json::json!({
                    "name": name,
                    "preset": name,
                    "width_mm": d.width_mm,
                    "height_mm": d.height_mm,
                    "fields": d.fields(),
                    "auto_fields": d.auto_fields(),
                    "sample": d.sample,
                }))
            })
            .collect(),
    )
}

/// `GET /templates`: presets that have a design (kept for older clients).
async fn templates(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    HttpResponse::Ok().json(templates_json(&state))
}

/// JSON body of `POST /print`: fill a preset's design with data.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DataPrint {
    /// Preset with a design; defaults to the default preset.
    #[serde(default)]
    preset: Option<String>,
    /// Array of objects: one per label, or one per row of a list design.
    data: serde_json::Value,
    /// Values shared by every page (header/footer of a list, or common label text).
    #[serde(default)]
    header: Option<serde_json::Value>,
    /// Same settings as the `/print` query (printer, copies, offset_x, ...).
    #[serde(default)]
    settings: PrintSettings,
    /// Where to POST the outcome once the job is done (see `callback`).
    #[serde(default)]
    callback_url: Option<String>,
    /// The client's own reference, echoed in the callback.
    #[serde(default)]
    callback_ref: Option<String>,
}

/// Body of the older `POST /print-template`, where the design was a "template".
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTemplatePrint {
    template: String,
    data: serde_json::Value,
    #[serde(default)]
    header: Option<serde_json::Value>,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    settings: PrintSettings,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    callback_ref: Option<String>,
}

const MAX_JSON_BYTES: usize = 5 * 1024 * 1024;
/// Records per JSON print; each becomes at least one rendered label or row.
const MAX_RECORDS: usize = 10_000;

/// Runs a job on the blocking pool and answers like `/print`.
async fn submit(
    state: Arc<AppState>,
    request: JobRequest,
    message: &str,
    cleanup: Option<std::path::PathBuf>,
    callback: Option<Callback>,
) -> HttpResponse {
    let watcher = state.clone();
    let outcome = web::block(move || {
        let result = job::run(&state, request);
        // Queue position right after submission, so clients can show it immediately.
        let status = result.as_ref().ok().and_then(|r| job_status_json(&state, &r.job_id));
        result.map(|r| (r, status))
    })
    .await;
    if let Some(file) = cleanup {
        let _ = std::fs::remove_file(file);
    }
    match outcome {
        Ok(Ok((r, status))) => {
            let mut body = serde_json::json!({
                "message": message,
                "job_id": r.job_id,
                "printer": r.printer,
                "preset": r.preset,
                "sheets": r.sheets,
                "spooler_job": r.spooler_job,
                "status": status,
            });
            if let Some(cb) = callback {
                // The job is already printing: a refused callback is reported, not an error.
                body["callback"] = match callback::watch(watcher, &r, cb) {
                    Ok(()) => serde_json::json!({ "status": "watching" }),
                    Err(e) => serde_json::json!({ "status": "refused", "message": e }),
                };
            }
            HttpResponse::Ok().json(body)
        }
        Ok(Err(JobError::BadRequest(m))) => error(actix_web::http::StatusCode::BAD_REQUEST, &m),
        Ok(Err(JobError::Failed(m))) => {
            error(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, &format!("Print failed: {}", m))
        }
        Err(e) => internal(e),
    }
}

/// Validates the data part of a JSON print and queues it.
async fn submit_data(
    req: &HttpRequest,
    state: &State,
    token: Option<auth::ApiToken>,
    preset: Option<String>,
    data: serde_json::Value,
    header: Option<serde_json::Value>,
    settings: PrintSettings,
    (callback_url, callback_ref): (Option<String>, Option<String>),
) -> HttpResponse {
    let bad = |m: String| error(actix_web::http::StatusCode::BAD_REQUEST, &m);
    let callback = match Callback::from_request(callback_url.as_deref(), callback_ref.as_deref(), token.as_ref()) {
        Ok(c) => c,
        Err(e) => return bad(e),
    };
    if let Err(e) = settings.validate() {
        return bad(e);
    }
    let header = match header.as_ref().map(crate::template::record_from_json).transpose() {
        Ok(h) => h.unwrap_or_default(),
        Err(e) => return bad(format!("header: {}", e)),
    };
    let records = match &data {
        serde_json::Value::Array(items) if items.len() > MAX_RECORDS => {
            return bad(format!("data: maksimal {} data per request", MAX_RECORDS));
        }
        serde_json::Value::Array(items) => items.iter().map(crate::template::record_from_json).collect(),
        other => crate::template::record_from_json(other).map(|r| vec![r]),
    };
    let records: Vec<_> = match records {
        Ok(r) => r,
        Err(e) => return bad(format!("data: {}", e)),
    };
    let request = JobRequest {
        file_name: format!(
			"design {} ({} item(s))",
            preset.as_deref().unwrap_or("default preset"),
            records.len()
        ),
        document: Document::Data { records, header },
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.map(|t| t.name),
        hop: req.headers().contains_key(crate::remote::HOP_HEADER),
    };
    submit(state.get_ref().clone(), request, "Data sent to printer", None, callback).await
}

/// Older endpoint: `{"template": name, "data": …}` where the template is now a preset with a design.
async fn print_template(req: HttpRequest, state: State, body: web::Bytes) -> HttpResponse {
    let token = authorized!(req, state);
    let body: LegacyTemplatePrint = match serde_json::from_slice(&body) {
        Ok(b) => b,
		Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &format!("JSON body is invalid: {}", e)),
    };
    if body.preset.as_ref().is_some_and(|p| p != &body.template) {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
			"A template is now a preset with a design: send only \"template\" (or use POST /print with \"preset\")",
        );
    }
    let callback = (body.callback_url, body.callback_ref);
    submit_data(&req, &state, token, Some(body.template), body.data, body.header, body.settings, callback).await
}

async fn presets(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    let c = state.config.get();
    HttpResponse::Ok().json(serde_json::json!({
        "default_printer": c.default_printer,
        "default_preset": c.default_preset,
        "presets": c.presets,
    }))
}

async fn export_presets(req: HttpRequest, state: State) -> HttpResponse {
    authorized!(req, state);
    let bundle = PresetBundle::from_config(&state.config.get(), None, true);
    HttpResponse::Ok()
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"ix.print-presets.json\"",
        ))
        .json(bundle)
}

/// Saves the first uploaded file of the multipart body; returns (path, original name).
async fn save_upload(
    multipart: &mut Multipart,
    dir: &std::path::Path,
) -> Result<(std::path::PathBuf, String), HttpResponse> {
    let bad = |m: &str| error(actix_web::http::StatusCode::BAD_REQUEST, m);
    while let Some(item) = multipart.next().await {
		let mut field = item.map_err(|e| bad(&format!("Multipart data is invalid: {}", e)))?;
        let Some(name) = field
            .content_disposition()
            .and_then(|cd| cd.get_filename())
            .map(|n| sanitize_filename::sanitize(n))
        else {
            continue; // text field
        };
        std::fs::create_dir_all(dir).map_err(internal)?;
        let path = dir.join(format!("{}.pdf", uuid::Uuid::new_v4()));
        let mut file = std::fs::File::create(&path).map_err(internal)?;
        let mut size = 0usize;
        let mut head = Vec::with_capacity(PDF_HEADER_WINDOW);
        let result = async {
            while let Some(chunk) = field.next().await {
                let data = chunk.map_err(|e| bad(&format!("Upload terputus: {}", e)))?;
                size += data.len();
                if size > MAX_UPLOAD_BYTES {
                    return Err(error(actix_web::http::StatusCode::PAYLOAD_TOO_LARGE, "File terlalu besar"));
                }
                let room = PDF_HEADER_WINDOW - head.len();
                head.extend_from_slice(&data[..room.min(data.len())]);
                file.write_all(&data).map_err(internal)?;
            }
            // Only PDFs go on to PDFium; anything else is refused before parsing.
            if !head.windows(5).any(|w| w == b"%PDF-") {
				return Err(bad("File is not a PDF"));
            }
            Ok(())
        }
        .await;
        drop(file);
        if let Err(resp) = result {
            let _ = std::fs::remove_file(&path);
            return Err(resp);
        }
        let name = if name.is_empty() { "unnamed.pdf".to_string() } else { name };
        return Ok((path, name));
    }
    Err(bad("No file uploaded"))
}

/// `POST /print`: a PDF upload (multipart, settings in the query) or, with a
/// JSON body, data for a preset's design.
async fn print(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
    mut payload: web::Payload,
) -> HttpResponse {
    let token = authorized!(req, state);
    let is_json = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("application/json"));

    if is_json {
        let mut body = web::BytesMut::new();
        while let Some(chunk) = payload.next().await {
            match chunk {
                Ok(c) if body.len() + c.len() <= MAX_JSON_BYTES => body.extend_from_slice(&c),
				Ok(_) => return error(actix_web::http::StatusCode::PAYLOAD_TOO_LARGE, "JSON body is too large"),
                Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        let body: DataPrint = match serde_json::from_slice(&body) {
            Ok(b) => b,
			Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &format!("JSON body is invalid: {}", e)),
        };
        let callback = (body.callback_url, body.callback_ref);
        return submit_data(&req, &state, token, body.preset, body.data, body.header, body.settings, callback).await;
    }

    let (settings, preset) = match PrintSettings::from_query(&query) {
        Ok(v) => v,
        Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &e),
    };
    let callback = match callback_from_query(&query, token.as_ref()) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let mut multipart = Multipart::new(req.headers(), payload);
    let (file, file_name) = match save_upload(&mut multipart, &state.jobs_dir).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let request = JobRequest {
        document: Document::File(file.clone()),
        file_name,
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.map(|t| t.name),
        hop: req.headers().contains_key(crate::remote::HOP_HEADER),
    };
    submit(state.get_ref().clone(), request, "File uploaded and sent to printer", Some(file), callback).await
}

/// `callback_url` / `callback_ref` from the query of an upload or test page.
fn callback_from_query(
    query: &HashMap<String, String>,
    token: Option<&auth::ApiToken>,
) -> Result<Option<Callback>, HttpResponse> {
    let get = |k: &str| query.get(k).map(String::as_str);
    Callback::from_request(get("callback_url"), get("callback_ref"), token)
        .map_err(|e| error(actix_web::http::StatusCode::BAD_REQUEST, &e))
}

/// Prints the calibration page (`count` pages, default 1) with the same
/// settings `/print` would use.
async fn test_page(req: HttpRequest, state: State, query: web::Query<HashMap<String, String>>) -> HttpResponse {
    let token = authorized!(req, state);
    let (settings, preset) = match PrintSettings::from_query(&query) {
        Ok(v) => v,
        Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &e),
    };
    let count = match query.get("count").map(|c| c.trim()).filter(|c| !c.is_empty()) {
        None => 1,
        Some(c) => match c.parse::<u32>() {
            Ok(n) if (1..=crate::testpage::MAX_TEST_PAGES).contains(&n) => n,
            _ => {
                let msg = format!("count harus 1–{}", crate::testpage::MAX_TEST_PAGES);
                return error(actix_web::http::StatusCode::BAD_REQUEST, &msg);
            }
        },
    };
    let callback = match callback_from_query(&query, token.as_ref()) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let request = JobRequest {
        document: Document::TestPage { pages: count },
        file_name: "ix.print test page.pdf".into(),
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.map(|t| t.name),
        hop: req.headers().contains_key(crate::remote::HOP_HEADER),
    };
    submit(state.get_ref().clone(), request, "Test page sent to printer", None, callback).await
}

/// Registers every route; shared by the server and the tests.
pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(hello))
        .route("/docs", web::get().to(docs))
        .route("/list-printer", web::get().to(list_printers))
        .route("/printer-capabilities", web::get().to(capabilities))
        .route("/queue", web::get().to(queue))
        .route("/job-status", web::get().to(job_status))
        .route("/presets", web::get().to(presets))
        .route("/presets/export", web::get().to(export_presets))
        .route("/print", web::post().to(print))
        .route("/test-page", web::post().to(test_page))
        .route("/templates", web::get().to(templates))
        .route("/print-template", web::post().to(print_template));
}

/// Browsers may call the API only from the allowed websites, or from pages
/// this server serves itself (`/docs`). Read on every request, so changes in
/// the window apply at once. A refused origin gets an error before any handler
/// runs, so a foreign page can't even trigger a print "blind".
fn cors(state: Arc<AppState>) -> Cors {
    Cors::default()
        .allowed_origin_fn(move |origin, head| {
            let Ok(origin) = origin.to_str() else { return false };
            let host = head.headers().get("host").and_then(|h| h.to_str().ok());
            if host.is_some_and(|h| origin.eq_ignore_ascii_case(&format!("http://{}", h))) {
                return true;
            }
            let origin = origin.to_ascii_lowercase();
            state
                .config
                .get()
                .allowed_origins
                .iter()
                .any(|o| o == crate::config::ANY_ORIGIN || *o == origin)
        })
        .allowed_methods(["GET", "POST"])
        .allow_any_header()
        .max_age(600)
        .block_on_origin_mismatch(true)
}

/// Headers for every response (a handler's own value wins, e.g. `/docs`' CSP).
fn security_headers() -> DefaultHeaders {
    DefaultHeaders::new()
        .add(("X-Content-Type-Options", "nosniff"))
        .add(("X-Frame-Options", "DENY"))
        .add(("Referrer-Policy", "no-referrer"))
        .add(("Cache-Control", "no-store"))
        .add(("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'"))
}

pub async fn run(state: Arc<AppState>) -> std::io::Result<()> {
    if !state.config.get().bridge_enabled {
        return Ok(());
    }
    let port = state.config.get().server_port;
    let data = web::Data::new(state.clone());
    let cors_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .wrap(security_headers())
            .wrap(cors(cors_state.clone()))
            .app_data(data.clone())
            .configure(routes)
    })
    // Slow or idle clients can't hold connections open forever.
    .client_request_timeout(std::time::Duration::from_secs(30))
    .client_disconnect_timeout(std::time::Duration::from_secs(5))
    .keep_alive(std::time::Duration::from_secs(30))
    .max_connections(256)
    .bind(("0.0.0.0", port))?
    .run();
    *state.server_error.lock().unwrap() = None;
    *state.server_handle.lock().unwrap() = Some(server.handle());
    let result = server.await;
    state.server_handle.lock().unwrap().take();
    result
}

/// Starts the bridge on its own Actix runtime so the Tauri window stays responsive.
pub fn spawn(state: Arc<AppState>) {
    std::thread::spawn(move || {
        if let Err(e) = actix_web::rt::System::new().block_on(run(state.clone())) {
			let msg = format!("Server port {} failed: {}", state.config.get().server_port, e);
            eprintln!("{}", msg);
            *state.server_error.lock().unwrap() = Some(msg);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigStore;
    use crate::history::History;
    use actix_web::test;

    fn state() -> Arc<AppState> {
        let dir = std::env::temp_dir().join(format!("ixprint-srv-{}", uuid::Uuid::new_v4()));
        let config = ConfigStore::load(&dir);
        // Most tests exercise the endpoints themselves; token_protection turns tokens back on.
        config.update(|c| { c.require_token = false; Ok(()) }).unwrap();
        Arc::new(AppState {
            config,
            desktop_key: "test-desktop-key".into(),
            remotes: crate::remote::RemoteStore::load(&dir),
            remote_cache: Default::default(),
            server_error: Default::default(),
            server_handle: Default::default(),
            token_usage: Default::default(),
            failed_auth: Default::default(),
            history: History::new(dir.join("history")),
            diagnostics: crate::diagnostics::Diagnostics::new(&dir),
            jobs_dir: dir.join("jobs"),
        })
    }

    #[actix_web::test]
    async fn contract_all_routes_registered() {
        let app = test::init_service(
            App::new().app_data(web::Data::new(state())).configure(routes),
        )
        .await;

        let routes = [
            ("GET", "/"),
            ("GET", "/docs"),
            ("GET", "/list-printer"),
            ("GET", "/printer-capabilities"),
            ("GET", "/queue"),
            ("GET", "/job-status"),
            ("GET", "/presets"),
            ("GET", "/presets/export"),
            ("POST", "/print"),
            ("POST", "/test-page"),
            ("GET", "/templates"),
            ("POST", "/print-template"),
        ];

        for (method, path) in routes {
            let request = match method {
                "GET" => test::TestRequest::get().uri(path).to_request(),
                "POST" => test::TestRequest::post().uri(path).to_request(),
                _ => unreachable!(),
            };
            let response = test::call_service(&app, request).await;
            assert_ne!(response.status(), actix_web::http::StatusCode::NOT_FOUND, "{method} {path}");
        }
    }

    #[actix_web::test]
    async fn json_print_validation() {
        let state = state();
        let app = test::init_service(
            App::new().app_data(web::Data::new(state.clone())).configure(routes),
        )
        .await;
        let call = |uri: &'static str, body: &'static str| {
            let app = &app;
            async move {
                let req = test::TestRequest::post()
                    .uri(uri)
                    .insert_header(("Content-Type", "application/json"))
                    .set_payload(body.to_string())
                    .to_request();
                let resp = test::call_service(app, req).await;
                let code = resp.status().as_u16();
                let json: serde_json::Value = test::read_body_json(resp).await;
                (code, json["message"].as_str().unwrap_or_default().to_string())
            }
        };

        // A preset with a design, and one without.
        let design: crate::template::Template = serde_json::from_value(serde_json::json!({
            "width_mm": 60, "height_mm": 40,
            "elements": [{ "type": "text", "x": 1, "y": 1, "w": 50, "h": 8, "text": "{{nama}} {{rm}}" }]
        }))
        .unwrap();
        state
            .config
            .update(|c| {
                c.presets.insert("label".into(), crate::config::Preset { settings: Default::default(), design: Some(design) });
                c.presets.insert("a4".into(), Default::default());
                Ok(())
            })
            .unwrap();

        for (uri, body, expect) in [
            ("/print", "{not json", "JSON"),
            ("/print", r#"{"preset":"label","data":{},"nama":"extra"}"#, "unknown field"),
            ("/print", r#"{"preset":"label","data":{"a":[1]}}"#, "data"),
            ("/print", r#"{"preset":"missing","data":{"name":"A"}}"#, "was not found"),
            ("/print", r#"{"preset":"a4","data":{"name":"A"}}"#, "No printer"),
            ("/print", r#"{"data":{"name":"A"}}"#, "No printer"),
            ("/print", r#"{"preset":"label","data":{"nama":"A"},"callback_url":"https://a.id/cb"}"#, "token"),
            ("/print-template", r#"{"template":"missing","data":{"name":"A"}}"#, "was not found"),
            ("/print-template", r#"{"template":"label","preset":"a4","data":{}}"#, "preset"),
        ] {
            let (code, msg) = call(uri, body).await;
            assert_eq!(code, 400, "{} {} → {}", uri, body, msg);
            // This validation can reach preset/design checks first when the
            // test host has an OS default printer, which varies by machine.
            let expected = expect == "No printer" && msg.contains("design") || msg.contains(expect);
            assert!(expected, "{} {} → {}", uri, body, msg);
        }

        // A PDF upload without a file is still rejected as before.
        let resp = test::call_service(&app, test::TestRequest::post().uri("/print").to_request()).await;
        assert_eq!(resp.status().as_u16(), 400);

        // An upload that is not a PDF never reaches PDFium.
        let body = "--XX\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.pdf\"\r\n\r\nMZ not a pdf\r\n--XX--\r\n";
        let req = test::TestRequest::post()
            .uri("/print")
            .insert_header(("Content-Type", "multipart/form-data; boundary=XX"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400);
        let json: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(json["message"], "File is not a PDF");
        assert_eq!(std::fs::read_dir(&state.jobs_dir).map(|d| d.count()).unwrap_or(0), 0);

        // Unknown printers are refused before any spooler tool runs.
        let resp = test::call_service(&app, test::TestRequest::get().uri("/queue?printer=-h").to_request()).await;
        assert_eq!(resp.status().as_u16(), 404);

        // GET /templates lists presets with a design and their fields.
        let resp = test::call_service(&app, test::TestRequest::get().uri("/templates").to_request()).await;
        let list: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "label");
        assert_eq!(list[0]["fields"], serde_json::json!(["nama", "rm"]));
    }

    #[actix_web::test]
    async fn test_page_rejects_oversized_custom_paper() {
        let state = state();
        let app = test::init_service(
            App::new().app_data(web::Data::new(state)).configure(routes),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/test-page?paper=50000x50000mm")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400);
        let json: serde_json::Value = test::read_body_json(resp).await;
        assert!(json["message"].as_str().unwrap_or_default().contains("must be between"));
    }

    #[actix_web::test]
    async fn desktop_key_only_authorizes_loopback_requests() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let state = state();
        state.config.update(|c| { c.require_token = true; Ok(()) }).unwrap();
        let app = test::init_service(
            App::new().app_data(web::Data::new(state.clone())).configure(routes),
        )
        .await;

        let local = test::TestRequest::post()
            .uri("/print")
            .peer_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000))
            .insert_header(("X-IxPrint-Desktop", state.desktop_key.as_str()))
            .to_request();
        let local_response = test::call_service(&app, local).await;
        assert_eq!(local_response.status().as_u16(), 400); // Authorized; empty upload is rejected by /print.

        let remote = test::TestRequest::post()
            .uri("/print")
            .peer_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 4000))
            .insert_header(("X-IxPrint-Desktop", state.desktop_key.as_str()))
            .to_request();
        let remote_response = test::call_service(&app, remote).await;
        assert_eq!(remote_response.status().as_u16(), 401);
    }

    #[actix_web::test]
    async fn json_print_reports_unknown_printer_before_spooling() {
        let state = state();
        let design: crate::template::Template = serde_json::from_value(serde_json::json!({
            "width_mm": 60,
            "height_mm": 40,
            "elements": [{ "type": "text", "x": 1, "y": 1, "w": 50, "h": 8, "text": "{{name}}" }]
        }))
        .unwrap();
        state
            .config
            .update(|c| {
                c.presets.insert("label".into(), crate::config::Preset { settings: Default::default(), design: Some(design) });
                Ok(())
            })
            .unwrap();
        let app = test::init_service(
            App::new().app_data(web::Data::new(state)).configure(routes),
        )
        .await;
        let missing_printer = format!("__ixprint_missing_{}__", uuid::Uuid::new_v4());
        let payload = serde_json::json!({
            "preset": "label",
            "data": { "name": "Example" },
            "settings": { "printer": missing_printer }
        });
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/print")
                .insert_header(("Content-Type", "application/json"))
                .set_payload(payload.to_string())
                .to_request(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 400);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert!(body["message"].as_str().unwrap().contains("was not found"));
    }

    #[actix_web::test]
    async fn browser_origins() {
        let state = state();
        let app = test::init_service(
            App::new().wrap(cors(state.clone())).app_data(web::Data::new(state.clone())).configure(routes),
        )
        .await;
        let status = |origin: &'static str| {
            let app = &app;
            async move {
                let req = test::TestRequest::get()
                    .uri("/presets")
                    .insert_header(("Host", "192.168.1.6:18032"))
                    .insert_header(("Origin", origin))
                    .to_request();
                test::call_service(app, req).await.status().as_u16()
            }
        };

        // New installs: no website allowed, except pages served here (/docs).
        assert_ne!(status("https://evil.example").await, 200);
        assert_eq!(status("http://192.168.1.6:18032").await, 200);
        // Requests without Origin (servers, curl) are not a CORS matter.
        assert_eq!(test::call_service(&app, test::TestRequest::get().uri("/presets").to_request()).await.status().as_u16(), 200);

        state.config.update(|c| { c.allowed_origins = vec!["https://simrs.example".into()]; Ok(()) }).unwrap();
        assert_eq!(status("https://simrs.example").await, 200);
        assert_eq!(status("https://SIMRS.example").await, 200);
        assert_ne!(status("https://simrs.example.evil.test").await, 200);
        assert_ne!(status("null").await, 200);

        state.config.update(|c| { c.allowed_origins = vec!["*".into()]; Ok(()) }).unwrap();
        assert_eq!(status("https://anything.example").await, 200);
    }

    #[actix_web::test]
    async fn token_protection() {
        let state = state();
        let app = test::init_service(
            App::new().app_data(web::Data::new(state.clone())).configure(routes),
        )
        .await;
        let status = |req: test::TestRequest| {
            let app = &app;
            async move { test::call_service(app, req.to_request()).await.status().as_u16() }
        };

        // Tokens optional: open, but a wrong token is still rejected.
        assert_eq!(status(test::TestRequest::get().uri("/presets")).await, 200);
        assert_eq!(
            status(test::TestRequest::get().uri("/presets").insert_header(("Authorization", "Bearer fxp_nope"))).await,
            401
        );

        // Required with no token yet: everything is refused, with a hint to create one.
        state.config.update(|c| { c.require_token = true; Ok(()) }).unwrap();
        assert_eq!(status(test::TestRequest::get().uri("/presets")).await, 401);
        let (token, stored) = crate::auth::generate("SIMRS").unwrap();
        let id = stored.id.clone();
        state.config.update(|c| { c.tokens.push(stored); c.require_token = true; Ok(()) }).unwrap();

        // Required: every API endpoint needs it, in any of the three places.
        for uri in ["/presets", "/presets/export", "/list-printer", "/queue?printer=x", "/job-status?job_id=x"] {
            assert_eq!(status(test::TestRequest::get().uri(uri)).await, 401, "{}", uri);
        }
        assert_eq!(status(test::TestRequest::post().uri("/print")).await, 401);
        assert_eq!(status(test::TestRequest::post().uri("/test-page")).await, 401);
        assert_eq!(status(test::TestRequest::get().uri("/templates")).await, 401);
        assert_eq!(status(test::TestRequest::post().uri("/print-template")).await, 401);
        let bearer = format!("Bearer {}", token);
        assert_eq!(status(test::TestRequest::get().uri("/presets").insert_header(("Authorization", bearer.as_str()))).await, 200);
        assert_eq!(status(test::TestRequest::get().uri("/presets").insert_header(("X-Api-Token", token.as_str()))).await, 200);
        assert_eq!(status(test::TestRequest::get().uri(&format!("/presets?token={}", token))).await, 200);
        assert!(state.token_usage.snapshot().contains_key(&id));

        // Health check and docs stay public.
        assert_eq!(status(test::TestRequest::get().uri("/")).await, 200);
        assert_eq!(status(test::TestRequest::get().uri("/docs")).await, 200);

        // Repeated wrong tokens lock the client out, even with a valid token afterwards.
        let attacker: std::net::SocketAddr = "10.0.0.66:5000".parse().unwrap();
        for _ in 0..10 {
            let req = test::TestRequest::get().uri("/presets").peer_addr(attacker).insert_header(("X-Api-Token", "fxp_guess"));
            assert_eq!(status(req).await, 401);
        }
        let req = test::TestRequest::get().uri("/presets").peer_addr(attacker).insert_header(("Authorization", bearer.as_str()));
        assert_eq!(status(req).await, 429);
        let other: std::net::SocketAddr = "10.0.0.67:5000".parse().unwrap();
        let req = test::TestRequest::get().uri("/presets").peer_addr(other).insert_header(("Authorization", bearer.as_str()));
        assert_eq!(status(req).await, 200);

        // Revoked token stops working.
        state.config.update(|c| { c.tokens.clear(); Ok(()) }).unwrap();
        assert_eq!(status(test::TestRequest::get().uri("/presets").insert_header(("Authorization", bearer.as_str()))).await, 401);
    }
}
