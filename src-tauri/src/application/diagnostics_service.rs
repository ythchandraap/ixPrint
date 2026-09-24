use crate::application::state::AppState;
use crate::domain::diagnostics::DiagnosticEntry;

pub fn list(state: &AppState) -> Result<Vec<DiagnosticEntry>, String> {
    state.diagnostics.read()
}

pub fn record(state: &AppState, source: &str, message: &str, details: Option<&str>) -> Result<(), String> {
    state.diagnostics.record("error", source, message, details)
}

pub fn clear(state: &AppState) -> Result<(), String> {
    state.diagnostics.clear()
}
