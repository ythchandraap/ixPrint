use super::super::AppStateRef;
use crate::application::security::TokenUse;
use crate::application::token_service::CreatedToken;
use crate::domain::settings_config::Config;
#[tauri::command]
pub fn create_token(state: AppStateRef<'_>, name: String) -> Result<CreatedToken, String> {
    crate::application::token_service::create(&state, &name)
}

/// Revokes a token; clients using it get 401 from then on.
#[tauri::command]
pub fn revoke_token(state: AppStateRef<'_>, id: String) -> Result<Config, String> {
    crate::application::token_service::revoke(&state, &id)
}

#[tauri::command]
pub fn set_require_token(state: AppStateRef<'_>, required: bool) -> Result<Config, String> {
    crate::application::token_service::set_required(&state, required)
}

/// Replaces the websites allowed to call the API from a browser.
#[tauri::command]
pub fn set_allowed_origins(state: AppStateRef<'_>, origins: Vec<String>) -> Result<Config, String> {
    crate::application::token_service::set_allowed_origins(&state, &origins)
}

/// Last use of each token since ix.print started, by token id.
#[tauri::command]
pub fn token_usage(state: AppStateRef<'_>) -> std::collections::HashMap<String, TokenUse> {
    crate::application::token_service::usage(&state)
}
