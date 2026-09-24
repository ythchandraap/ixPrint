use super::super::AppStateRef;
use crate::domain::history::{HistoryDay, HistoryPage};
use crate::domain::settings_config::Config;
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
