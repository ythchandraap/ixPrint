use crate::application::ports::IncomingDocumentRepository;
use crate::application::state::AppState;
use crate::domain::incoming_job::{IncomingJob, IncomingSource, IncomingState};
use base64::Engine;
use serde::Serialize;
use std::path::Path;

const MAX_DOCUMENT_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Serialize)]
pub struct IncomingTargetCheck {
    pub compatible: bool,
    pub message: Option<String>,
    pub supported_media: Vec<String>,
    pub remote: Option<String>,
}

#[derive(Serialize)]
pub struct IncomingPrintResult {
    pub job_id: String,
    pub printer: String,
    pub sheets: u32,
    pub copies: u32,
    pub spooler_job: Option<String>,
    pub remote: Option<String>,
}

pub fn stage_pdf(
    state: &AppState,
    source_path: &str,
    source: IncomingSource,
    os_job_id: Option<String>,
    copies: u32,
) -> Result<IncomingJob, String> {
    if !(1..=999).contains(&copies) {
        return Err("Copies must be between 1 and 999".into());
    }
    let path = state
        .incoming_documents
        .canonical_file(Path::new(source_path), MAX_DOCUMENT_BYTES)?;
    if !path.is_file() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
        return Err("Choose a PDF document".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let staged_source = state.incoming_documents.copy_to_staging(&id, &path)?;
    let result = commit_staged_pdf(state, id.clone(), &staged_source, &path, source, os_job_id, copies);
    let _ = state.incoming_documents.discard_staging(&id);
    result
}

pub fn stage_file(
    state: &AppState,
    source_path: &str,
    source: IncomingSource,
    copies: u32,
) -> Result<IncomingJob, String> {
    if !(1..=999).contains(&copies) {
        return Err("Copies must be between 1 and 999".into());
    }
    let path = state
        .incoming_documents
        .canonical_file(Path::new(source_path), MAX_DOCUMENT_BYTES)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "pdf" {
        return stage_pdf(state, path.to_str().unwrap_or_default(), source, None, copies);
    }
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
        return Err("Choose one PDF, PNG, JPG, or JPEG file".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let generated = state.incoming_documents.staging_path(&id)?;
    let result = state
        .pdf_engine
        .image_to_pdf(&path, &generated)
        .and_then(|_| commit_staged_pdf(state, id.clone(), &generated, &path, source, None, copies));
    let _ = state.incoming_documents.discard_staging(&id);
    result
}

fn commit_staged_pdf(
    state: &AppState,
    id: String,
    staged_pdf: &Path,
    original: &Path,
    source: IncomingSource,
    os_job_id: Option<String>,
    copies: u32,
) -> Result<IncomingJob, String> {
    let pages = state.pdf_engine.page_sizes(staged_pdf)?;
    let document_name = original
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_filename::sanitize)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "document.pdf".into());
    let job = IncomingJob {
        id,
        source,
        document_name,
        pages,
        copies,
        os_job_id,
        state: IncomingState::AwaitingDestination,
        received_at: state.clock.now_rfc3339(),
        printer: None,
        spooler_job: None,
        remote_job_id: None,
        remote_name: None,
        submission_started_at: None,
        message: None,
    };
    job.validate()?;
    state.incoming_documents.create(job, staged_pdf).map(|(job, _)| job)
}

pub fn list(state: &AppState) -> Result<Vec<IncomingJob>, String> {
    state.incoming_documents.list()
}

mod submission;
pub use submission::{check_target, receive_ready_pdf, submit};

pub fn preview_page(state: &AppState, job_id: &str, page: usize) -> Result<String, String> {
    let job = state
        .incoming_documents
        .get(job_id)?
        .ok_or_else(|| "Incoming document was not found".to_string())?;
    if job.state != IncomingState::AwaitingDestination {
        return Err("This incoming document is no longer awaiting a destination".into());
    }
    if page == 0 || page > job.pages.len() {
        return Err("Page is outside this document".into());
    }
    let path = state.incoming_documents.document_path(job_id)?;
    let png = state.pdf_engine.render_page_png(&path, page - 1, 3.0)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

pub fn cancel(state: &AppState, job_id: &str) -> Result<IncomingJob, String> {
    let mut job = state
        .incoming_documents
        .get(job_id)?
        .ok_or_else(|| "Incoming document was not found".to_string())?;
    if job.state != IncomingState::AwaitingDestination {
        return Err("Only a document awaiting a destination can be cancelled".into());
    }
    job.state = IncomingState::Cancelled;
    job.message = Some("Cancelled before printing".into());
    state.incoming_documents.save(&job)?;
    submission::record_cancelled(state, &job);
    state.incoming_documents.remove_document(job_id)?;
    Ok(job)
}

pub fn prune(repository: &dyn IncomingDocumentRepository, retention_days: u32) -> Result<(), String> {
    repository.prune(retention_days)
}
