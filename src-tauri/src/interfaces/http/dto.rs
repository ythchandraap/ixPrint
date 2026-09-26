use crate::application::state::AppState;
use crate::domain::settings::PrintSettings;
use actix_web::{web, HttpResponse};
use serde::Serialize;
use std::sync::Arc;

pub(super) const MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;
pub(super) const PDF_HEADER_WINDOW: usize = 1024;

pub(super) type State = web::Data<Arc<AppState>>;

#[derive(Serialize)]
pub(super) struct Message<'a> {
    pub(super) message: &'a str,
}

pub(super) fn error(status: actix_web::http::StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(Message { message })
}

/// Logs an internal failure and answers without its details (paths, OS errors).
pub(super) fn internal(e: impl std::fmt::Display) -> HttpResponse {
    eprintln!("[ix.print] internal error: {}", e);
    error(
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        "An internal error occurred in ix.print",
    )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DataPrint {
    /// Preset with a design; defaults to the default preset.
    #[serde(default)]
    pub(super) preset: Option<String>,
    /// Array of objects: one per label, or one per row of a list design.
    pub(super) data: serde_json::Value,
    /// Values shared by every page (header/footer of a list, or common label text).
    #[serde(default)]
    pub(super) header: Option<serde_json::Value>,
    /// Same settings as the `/print` query (printer, copies, offset_x, ...).
    #[serde(default)]
    pub(super) settings: PrintSettings,
    /// Where to POST the outcome once the job is done (see `callback`).
    #[serde(default)]
    pub(super) callback_url: Option<String>,
    /// The client's own reference, echoed in the callback.
    #[serde(default)]
    pub(super) callback_ref: Option<String>,
}

/// Body of the older `POST /print-template`, where the design was a "template".
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyTemplatePrint {
    pub(super) template: String,
    pub(super) data: serde_json::Value,
    #[serde(default)]
    pub(super) header: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) preset: Option<String>,
    #[serde(default)]
    pub(super) settings: PrintSettings,
    #[serde(default)]
    pub(super) callback_url: Option<String>,
    #[serde(default)]
    pub(super) callback_ref: Option<String>,
}

pub(super) const MAX_JSON_BYTES: usize = 5 * 1024 * 1024;
/// Records per JSON print; each becomes at least one rendered label or row.
pub(super) const MAX_RECORDS: usize = crate::domain::template::MAX_RECORDS;

pub(super) struct DataSubmission {
    pub(super) preset: Option<String>,
    pub(super) data: serde_json::Value,
    pub(super) header: Option<serde_json::Value>,
    pub(super) settings: PrintSettings,
    pub(super) history_input: serde_json::Value,
    pub(super) callback: (Option<String>, Option<String>),
}
