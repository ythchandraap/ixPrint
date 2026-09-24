use super::super::auth_guard::{acquire_in_flight, authorized};
use super::super::dto::{
    error, internal, DataPrint, DataSubmission, LegacyTemplatePrint, State, MAX_JSON_BYTES, MAX_RECORDS,
    MAX_UPLOAD_BYTES, PDF_HEADER_WINDOW,
};
use crate::application::state::AppState;
use crate::domain::auth;
use crate::domain::callback::Callback;
use crate::domain::job::{Document, JobError, JobRequest};
use crate::domain::settings::PrintSettings;
use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

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
        let result = crate::application::job_service::run(&state, request);
        // Queue position right after submission, so clients can show it immediately.
        let status = result
            .as_ref()
            .ok()
            .and_then(|r| crate::application::printer_service::job_status_json(&state, &r.job_id));
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
                body["callback"] = match crate::application::callback_service::watch(watcher, &r, cb) {
                    Ok(()) => serde_json::json!({ "status": "watching" }),
                    Err(e) => serde_json::json!({ "status": "refused", "message": e }),
                };
            }
            HttpResponse::Ok().json(body)
        }
        Ok(Err(JobError::BadRequest(m))) => error(actix_web::http::StatusCode::BAD_REQUEST, &m),
        Ok(Err(JobError::Failed(m))) => error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Print failed: {}", m),
        ),
        Err(e) => internal(e),
    }
}

/// Validates the data part of a JSON print and queues it.
async fn submit_data(
    req: &HttpRequest,
    state: &State,
    token: Option<auth::ApiToken>,
    submission: DataSubmission,
) -> HttpResponse {
    let DataSubmission {
        preset,
        data,
        header,
        settings,
        callback: (callback_url, callback_ref),
    } = submission;
    let bad = |m: String| error(actix_web::http::StatusCode::BAD_REQUEST, &m);
    let callback = match Callback::from_request(callback_url.as_deref(), callback_ref.as_deref(), token.as_ref()) {
        Ok(c) => c,
        Err(e) => return bad(e),
    };
    if let Err(e) = settings.validate() {
        return bad(e);
    }
    let header = match header
        .as_ref()
        .map(crate::domain::template::record_from_json)
        .transpose()
    {
        Ok(h) => h.unwrap_or_default(),
        Err(e) => return bad(format!("header: {}", e)),
    };
    let records = match &data {
        serde_json::Value::Array(items) if items.len() > MAX_RECORDS => {
            return bad(format!("data: at most {} items per request", MAX_RECORDS));
        }
        serde_json::Value::Array(items) => items.iter().map(crate::domain::template::record_from_json).collect(),
        other => crate::domain::template::record_from_json(other).map(|r| vec![r]),
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
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    submit(state.get_ref().clone(), request, "Data sent to printer", None, callback).await
}

/// Older endpoint: `{"template": name, "data": …}` where the template is now a preset with a design.
pub(crate) async fn print_template(req: HttpRequest, state: State, body: web::Bytes) -> HttpResponse {
    let token = authorized!(req, state);
    let _in_flight = match acquire_in_flight(state.get_ref().as_ref(), &req, token.as_ref()) {
        Ok(guard) => guard,
        Err(response) => return response,
    };
    let body: LegacyTemplatePrint = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            return error(
                actix_web::http::StatusCode::BAD_REQUEST,
                &format!("JSON body is invalid: {}", e),
            )
        }
    };
    if body.preset.as_ref().is_some_and(|p| p != &body.template) {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "A template is now a preset with a design: send only \"template\" (or use POST /print with \"preset\")",
        );
    }
    let callback = (body.callback_url, body.callback_ref);
    submit_data(
        &req,
        &state,
        token,
        DataSubmission {
            preset: Some(body.template),
            data: body.data,
            header: body.header,
            settings: body.settings,
            callback,
        },
    )
    .await
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
            .map(sanitize_filename::sanitize)
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
                    return Err(error(
                        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "File terlalu besar",
                    ));
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
        let name = if name.is_empty() {
            "unnamed.pdf".to_string()
        } else {
            name
        };
        return Ok((path, name));
    }
    Err(bad("No file uploaded"))
}

/// `POST /print`: a PDF upload (multipart, settings in the query) or, with a
/// JSON body, data for a preset's design.
pub(crate) async fn print(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
    mut payload: web::Payload,
) -> HttpResponse {
    let token = authorized!(req, state);
    let _in_flight = match acquire_in_flight(state.get_ref().as_ref(), &req, token.as_ref()) {
        Ok(guard) => guard,
        Err(response) => return response,
    };
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
            Err(e) => {
                return error(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    &format!("JSON body is invalid: {}", e),
                )
            }
        };
        let callback = (body.callback_url, body.callback_ref);
        return submit_data(
            &req,
            &state,
            token,
            DataSubmission {
                preset: body.preset,
                data: body.data,
                header: body.header,
                settings: body.settings,
                callback,
            },
        )
        .await;
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
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    submit(
        state.get_ref().clone(),
        request,
        "File uploaded and sent to printer",
        Some(file),
        callback,
    )
    .await
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
pub(crate) async fn test_page(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let token = authorized!(req, state);
    let _in_flight = match acquire_in_flight(state.get_ref().as_ref(), &req, token.as_ref()) {
        Ok(guard) => guard,
        Err(response) => return response,
    };
    let (settings, preset) = match PrintSettings::from_query(&query) {
        Ok(v) => v,
        Err(e) => return error(actix_web::http::StatusCode::BAD_REQUEST, &e),
    };
    let count = match query.get("count").map(|c| c.trim()).filter(|c| !c.is_empty()) {
        None => 1,
        Some(c) => match c.parse::<u32>() {
            Ok(n) if (1..=crate::infrastructure::pdf::test_page::MAX_TEST_PAGES).contains(&n) => n,
            _ => {
                let msg = format!(
                    "count must be between 1 and {}",
                    crate::infrastructure::pdf::test_page::MAX_TEST_PAGES
                );
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
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    submit(
        state.get_ref().clone(),
        request,
        "Test page sent to printer",
        None,
        callback,
    )
    .await
}

/// Status of a job printed through ix.print: failed in ix.print, queued (with
/// position), printing, blocked by a printer error, or completed.
pub(crate) fn job_status_json(state: &AppState, job_id: &str) -> Option<serde_json::Value> {
    crate::application::printer_service::job_status_json(state, job_id)
}

pub(crate) async fn job_status(
    req: HttpRequest,
    state: State,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    authorized!(req, state);
    let Some(job_id) = query.get("job_id").cloned() else {
        return error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "parameter 'job_id' is required",
        );
    };
    let state = state.get_ref().clone();
    match web::block(move || job_status_json(&state, &job_id)).await {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
        Ok(None) => error(actix_web::http::StatusCode::NOT_FOUND, "job not found"),
        Err(e) => internal(e),
    }
}
