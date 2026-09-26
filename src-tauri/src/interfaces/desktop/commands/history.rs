use super::super::AppStateRef;
use crate::domain::history::{HistoryDay, HistoryPage};
use crate::domain::settings_config::Config;
use base64::Engine;
use serde::Serialize;

#[derive(Serialize)]
pub struct PdfPageResponse {
    availability: crate::application::history_service::ContentAvailability,
    page: usize,
    page_count: Option<usize>,
    image_data_url: Option<String>,
}
#[tauri::command]
pub fn set_history_retention(state: AppStateRef<'_>, days: u32) -> Result<Config, String> {
    crate::application::history_service::set_retention(&state, days)
}

#[tauri::command]
pub fn history_days(state: AppStateRef<'_>) -> Vec<HistoryDay> {
    crate::application::history_service::days(&state)
}

/// One page of a day's history (1-based `page`).
#[tauri::command]
pub fn history_page(
    state: AppStateRef<'_>,
    date: String,
    page: usize,
    per_page: usize,
    failed_only: bool,
) -> Result<HistoryPage, String> {
    crate::application::history_service::page(&state, &date, page, per_page, failed_only)
}

#[tauri::command]
pub async fn search_history(
    state: AppStateRef<'_>,
    query: String,
    page: usize,
    per_page: usize,
) -> Result<HistoryPage, String> {
    let state = state.inner().clone();
    super::super::blocking(move || crate::application::history_service::search(&state, &query, page, per_page)).await?
}

#[tauri::command]
pub async fn history_content(
    state: AppStateRef<'_>,
    job_id: String,
) -> Result<crate::application::history_service::ContentDetail, String> {
    let state = state.inner().clone();
    super::super::blocking(move || crate::application::history_service::content(&state, &job_id)).await?
}

#[tauri::command]
pub async fn history_pdf_page(
    state: AppStateRef<'_>,
    job_id: String,
    document: String,
    page: usize,
) -> Result<PdfPageResponse, String> {
    let state = state.inner().clone();
    let result =
        super::super::blocking(move || crate::application::history_service::pdf_page(&state, &job_id, &document, page))
            .await??;
    Ok(PdfPageResponse {
        availability: result.availability,
        page: result.page,
        page_count: result.page_count,
        image_data_url: result.png.map(|png| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        }),
    })
}

#[tauri::command]
pub fn retry_callback(state: AppStateRef<'_>, job_id: String) -> Result<(), String> {
    crate::application::callback_service::retry_failed(state.inner().clone(), &job_id)
}

#[tauri::command]
pub async fn cancel_own_job(state: AppStateRef<'_>, job_id: String) -> Result<(), String> {
    let state = state.inner().clone();
    super::super::blocking(move || crate::application::printer_service::cancel_own_job(&state, &job_id)).await?
}
