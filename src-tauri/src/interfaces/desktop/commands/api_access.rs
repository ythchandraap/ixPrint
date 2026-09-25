use super::super::blocking;
use super::super::AppStateRef;
use crate::domain::api_access::ApiAccessEntry;

#[tauri::command]
pub async fn api_access_log(state: AppStateRef<'_>) -> Result<Vec<ApiAccessEntry>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::api_access_service::list(&state)).await?
}

#[tauri::command]
pub async fn clear_api_access_log(state: AppStateRef<'_>) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || crate::application::api_access_service::clear(&state)).await?
}
