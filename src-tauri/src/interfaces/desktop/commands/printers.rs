use super::super::blocking;
use super::super::AppStateRef;
use crate::application::printer_service::PrinterView;
/// `Err` with the reason when PDFium could not be loaded.
#[tauri::command]
pub fn engine_status(state: AppStateRef<'_>) -> Result<(), String> {
    crate::application::printer_service::engine_status(&state)
}

#[tauri::command]
pub async fn get_printers(state: AppStateRef<'_>) -> Result<Vec<PrinterView>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::printer_views(&state, true)).await
}

#[tauri::command]
pub async fn get_capabilities(
    state: AppStateRef<'_>,
    printer: String,
) -> Result<crate::domain::printer::Capabilities, String> {
    let state = state.inner().clone();
    blocking(move || {
        crate::application::printer_service::capabilities(&state, &printer, false).map_err(|error| match error {
            crate::application::printer_service::PrinterLookupError::NotFound(message) => message,
            crate::application::printer_service::PrinterLookupError::Remote(error) => error.to_string(),
        })
    })
    .await?
}

#[tauri::command]
pub async fn get_queue(
    state: AppStateRef<'_>,
    printer: String,
) -> Result<crate::domain::printer::PrinterQueue, String> {
    let state = state.inner().clone();
    blocking(move || {
        crate::application::printer_service::queue(&state, &printer, false).map_err(|error| match error {
            crate::application::printer_service::PrinterLookupError::NotFound(message) => message,
            crate::application::printer_service::PrinterLookupError::Remote(error) => error.to_string(),
        })
    })
    .await?
}

/// Live status of a job printed through ix.print (see `/job-status`).
#[tauri::command]
pub async fn job_status(state: AppStateRef<'_>, job_id: String) -> Result<serde_json::Value, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::job_status_json(&state, &job_id))
        .await?
        .ok_or_else(|| "Job not found".to_string())
}

#[tauri::command]
pub async fn printer_set_os_default(state: AppStateRef<'_>, name: String) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::set_os_default(&state, &name)).await?
}
