use super::super::blocking;
use super::super::AppStateRef;
use crate::domain::remote::RemoteView;
#[tauri::command]
pub fn remote_list(state: AppStateRef<'_>) -> Vec<RemoteView> {
    crate::application::remote_service::list(&state)
}

#[tauri::command]
pub async fn remote_add(
    state: AppStateRef<'_>,
    name: String,
    url: String,
    token: String,
    preset: Option<String>,
) -> Result<Vec<RemoteView>, String> {
    let state = state.inner().clone();
    let remote = crate::application::remote_service::build_remote(&state, name, url, token, preset, None)?;
    blocking(move || crate::application::remote_service::add(&state, remote)).await?
}

#[tauri::command]
pub async fn remote_update(
    state: AppStateRef<'_>,
    id: String,
    name: String,
    url: String,
    token: Option<String>,
    preset: Option<String>,
) -> Result<Vec<RemoteView>, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::remote_service::update(&state, &id, name, url, token, preset)).await?
}

#[tauri::command]
pub fn remote_remove(state: AppStateRef<'_>, id: String) -> Result<Vec<RemoteView>, String> {
    crate::application::remote_service::remove(&state, &id)
}

#[tauri::command]
pub async fn remote_test(state: AppStateRef<'_>, id: String) -> Result<RemoteView, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::remote_service::test(&state, &id)).await?
}
