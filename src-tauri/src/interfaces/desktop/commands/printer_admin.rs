use super::super::blocking;
use super::super::AppStateRef;
use crate::domain::settings_config::Config;
// ---- printer devices (OS level; not exposed over HTTP) ----------------------

#[tauri::command]
pub async fn printer_devices(state: AppStateRef<'_>) -> Result<Vec<crate::domain::printer_admin::Device>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::devices(&state)).await?
}

#[tauri::command]
pub async fn printer_drivers(
    state: AppStateRef<'_>,
    query: String,
) -> Result<Vec<crate::domain::printer_admin::Driver>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::drivers(&state, &query)).await?
}

#[tauri::command]
pub async fn printer_details(
    state: AppStateRef<'_>,
    name: String,
) -> Result<crate::domain::printer_admin::PrinterDetails, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::details(&state, &name)).await?
}

#[tauri::command]
pub async fn printer_add(
    state: AppStateRef<'_>,
    printer: crate::domain::printer_admin::NewPrinter,
) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::add(&state, &printer)).await?
}

/// Edits a printer; a rename also updates the default printer and presets.
#[tauri::command]
pub async fn printer_update(
    state: AppStateRef<'_>,
    name: String,
    edit: crate::domain::printer_admin::PrinterEdit,
) -> Result<Config, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::update_local(&state, &name, &edit)).await?
}

#[tauri::command]
pub async fn printer_remove(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::printer_service::remove(&state, &name)).await?
}
