use super::super::blocking;
use super::super::AppStateRef;
use crate::domain::incoming_job::{IncomingJob, IncomingSource};

#[tauri::command]
pub async fn list_incoming_documents(state: AppStateRef<'_>) -> Result<Vec<IncomingJob>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::list(&state)).await?
}

#[tauri::command]
pub async fn stage_incoming_pdf(
    state: AppStateRef<'_>,
    source_path: String,
    source: IncomingSource,
    os_job_id: Option<String>,
    copies: u32,
) -> Result<IncomingJob, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::stage_pdf(&state, &source_path, source, os_job_id, copies))
        .await?
}

#[tauri::command]
pub async fn stage_incoming_file(
    state: AppStateRef<'_>,
    source_path: String,
    source: IncomingSource,
    copies: u32,
) -> Result<IncomingJob, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::stage_file(&state, &source_path, source, copies)).await?
}

#[tauri::command]
pub async fn preview_incoming_pdf_page(state: AppStateRef<'_>, job_id: String, page: usize) -> Result<String, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::preview_page(&state, &job_id, page)).await?
}

#[tauri::command]
pub async fn check_incoming_target(
    state: AppStateRef<'_>,
    job_id: String,
    printer: String,
) -> Result<crate::application::document_service::IncomingTargetCheck, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::check_target(&state, &job_id, &printer)).await?
}

#[tauri::command]
pub async fn submit_incoming_document(
    state: AppStateRef<'_>,
    job_id: String,
    printer: String,
) -> Result<crate::application::document_service::IncomingPrintResult, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::submit(&state, &job_id, &printer)).await?
}

#[tauri::command]
pub async fn cancel_incoming_document(state: AppStateRef<'_>, job_id: String) -> Result<IncomingJob, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::document_service::cancel(&state, &job_id)).await?
}
