use super::super::auth_guard::{acquire_in_flight, authorized};
use super::super::dto::{error, internal, DataPrint, DataSubmission, LegacyTemplatePrint, State, MAX_RECORDS};
use super::job_support::{
    callback_from_query, fingerprint, idempotency_key, read_json_payload, save_upload, Submission,
};
use crate::application::ports::Reservation;
use crate::application::state::AppState;
use crate::domain::auth;
use crate::domain::callback::Callback;
use crate::domain::job::{Document, JobError, JobRequest};
use crate::domain::settings::PrintSettings;
use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// Validates a designed preset request without rendering or printing it.
pub(crate) async fn preflight(req: HttpRequest, state: State, mut payload: web::Payload) -> HttpResponse {
    let token = authorized!(req, state);
    let body: DataPrint = match read_json_payload(&mut payload).await {
        Ok(body) => body,
        Err(response) => return response,
    };
    if let Err(message) = Callback::from_request(
        body.callback_url.as_deref(),
        body.callback_ref.as_deref(),
        token.as_ref(),
    ) {
        return error(actix_web::http::StatusCode::BAD_REQUEST, &message);
    }
    let state = state.get_ref().clone();
    match web::block(move || {
        crate::application::preset_service::preflight_data(
            &state,
            body.preset.as_deref(),
            &body.data,
            body.header.as_ref(),
            &body.settings,
        )
    })
    .await
    {
        Ok(Ok(report)) => HttpResponse::Ok().json(report),
        Ok(Err(message)) => error(actix_web::http::StatusCode::BAD_REQUEST, &message),
        Err(e) => internal(e),
    }
}

/// Runs a job on the blocking pool and answers like `/print`.
async fn submit(
    state: Arc<AppState>,
    mut request: JobRequest,
    message: &str,
    cleanup: Option<std::path::PathBuf>,
    callback: Option<Callback>,
    idempotency: Option<String>,
) -> HttpResponse {
    let watcher = state.clone();
    let fingerprint_callback = callback.clone();
    let outcome = web::block(move || {
        let request_fingerprint = if idempotency.is_some() {
            Some(fingerprint(&request, fingerprint_callback.as_ref()).map_err(JobError::Failed)?)
        } else {
            None
        };
        if let (Some(key), Some(fingerprint)) = (idempotency, request_fingerprint) {
            let job_id = uuid::Uuid::new_v4().to_string();
            match state
                .idempotency
                .reserve(&key, &fingerprint, &job_id)
                .map_err(JobError::Failed)?
            {
                Reservation::New => request.job_id = Some(job_id),
                Reservation::Existing(id) => {
                    return match state.history.find(&id) {
                        Some(entry) if entry.success => {
                            let result = crate::domain::job::JobResult {
                                job_id: entry.id,
                                printer: entry.printer,
                                preset: entry.preset,
                                sheets: entry.sheets,
                                spooler_job: entry.spooler_job,
                            };
                            let status = crate::application::printer_service::job_status_json(&state, &result.job_id);
                            Ok(Submission::Replay(result, status))
                        }
                        Some(entry) => Err(JobError::BadRequest(format!(
                            "Original request failed: {}. Use a new Idempotency-Key to retry",
                            entry.error.unwrap_or_default()
                        ))),
                        None => Ok(Submission::Pending(Some(id))),
                    };
                }
                Reservation::Pending => return Ok(Submission::Pending(None)),
                Reservation::Conflict => return Ok(Submission::Conflict),
            }
        }
        let result = crate::application::job_service::run(&state, request);
        // Queue position right after submission, so clients can show it immediately.
        let status = result
            .as_ref()
            .ok()
            .and_then(|r| crate::application::printer_service::job_status_json(&state, &r.job_id));
        result.map(|r| Submission::New(r, status))
    })
    .await;
    if let Some(file) = cleanup {
        let _ = std::fs::remove_file(file);
    }
    match outcome {
        Ok(Ok(submission @ (Submission::New(_, _) | Submission::Replay(_, _)))) => {
            let replayed = matches!(submission, Submission::Replay(_, _));
            let (r, status) = match submission {
                Submission::New(r, status) | Submission::Replay(r, status) => (r, status),
                Submission::Pending(_) | Submission::Conflict => unreachable!(),
            };
            let mut body = serde_json::json!({
                "message": message,
                "job_id": r.job_id,
                "printer": r.printer,
                "preset": r.preset,
                "sheets": r.sheets,
                "spooler_job": r.spooler_job,
                "status": status,
                "replayed": replayed,
            });
            if !replayed {
                if let Some(cb) = callback {
                    // The job is already printing: a refused callback is reported, not an error.
                    body["callback"] = match crate::application::callback_service::watch(watcher, &r, cb) {
                        Ok(()) => serde_json::json!({ "status": "watching" }),
                        Err(e) => serde_json::json!({ "status": "refused", "message": e }),
                    };
                }
            }
            HttpResponse::Ok().json(body)
        }
        Ok(Ok(Submission::Pending(job_id))) => HttpResponse::Accepted().json(serde_json::json!({
            "message": "Original request is still processing or its outcome is not yet known",
            "job_id": job_id,
            "replayed": true,
        })),
        Ok(Ok(Submission::Conflict)) => error(
            actix_web::http::StatusCode::CONFLICT,
            "Idempotency-Key was already used with different request content",
        ),
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
    let idempotency = match idempotency_key(req, token.as_ref()) {
        Ok(value) => value,
        Err(response) => return response,
    };
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
        job_id: None,
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
        token: token.as_ref().map(|t| t.name.clone()),
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    submit(
        state.get_ref().clone(),
        request,
        "Data sent to printer",
        None,
        callback,
        idempotency,
    )
    .await
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
        let body: DataPrint = match read_json_payload(&mut payload).await {
            Ok(body) => body,
            Err(response) => return response,
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
        job_id: None,
        document: Document::File(file.clone()),
        file_name,
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.as_ref().map(|t| t.name.clone()),
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    let idempotency = match idempotency_key(&req, token.as_ref()) {
        Ok(value) => value,
        Err(response) => {
            let _ = std::fs::remove_file(&file);
            return response;
        }
    };
    submit(
        state.get_ref().clone(),
        request,
        "File uploaded and sent to printer",
        Some(file),
        callback,
        idempotency,
    )
    .await
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
        job_id: None,
        document: Document::TestPage { pages: count },
        file_name: "ix.print test page.pdf".into(),
        settings,
        preset,
        source: "http",
        client: req.peer_addr().map(|a| a.ip().to_string()),
        token: token.as_ref().map(|t| t.name.clone()),
        hop: req.headers().contains_key(crate::domain::remote::HOP_HEADER),
    };
    let idempotency = match idempotency_key(&req, token.as_ref()) {
        Ok(value) => value,
        Err(response) => return response,
    };
    submit(
        state.get_ref().clone(),
        request,
        "Test page sent to printer",
        None,
        callback,
        idempotency,
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
