use crate::application::state::AppState;
use crate::domain::api_access::ApiAccessEntry;

pub fn list(state: &AppState) -> Result<Vec<ApiAccessEntry>, String> {
    state.api_access_log.read()
}

pub fn clear(state: &AppState) -> Result<(), String> {
    state.api_access_log.clear()
}
