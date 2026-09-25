use super::super::AppStateRef;
use crate::domain::diagnostics::DiagnosticEntry;
#[tauri::command]
pub fn error_log(state: AppStateRef<'_>) -> Result<Vec<DiagnosticEntry>, String> {
    crate::application::diagnostics_service::list(&state)
}

#[tauri::command]
pub fn record_error(
    state: AppStateRef<'_>,
    source: String,
    message: String,
    details: Option<String>,
) -> Result<(), String> {
    crate::application::diagnostics_service::record(&state, &source, &message, details.as_deref())
}

#[tauri::command]
pub fn clear_error_log(state: AppStateRef<'_>) -> Result<(), String> {
    crate::application::diagnostics_service::clear(&state)
}

#[tauri::command]
pub fn open_system_logs() -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("mmc.exe")
            .arg("eventvwr.msc")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open Windows Event Viewer: {}", e))
    }
    #[cfg(not(windows))]
    {
        Err("Windows Event Viewer is only available on Windows".into())
    }
}
