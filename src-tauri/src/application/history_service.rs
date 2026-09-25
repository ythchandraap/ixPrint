use crate::application::state::AppState;
use crate::domain::history::{HistoryDay, HistoryPage};
use crate::domain::settings_config::Config;

pub fn set_retention(state: &AppState, days: u32) -> Result<Config, String> {
    let config = state.config.update(&mut |config| {
        config.history_retention_days = days;
        Ok(())
    })?;
    state.history.prune(days);
    Ok(config)
}

pub fn days(state: &AppState) -> Vec<HistoryDay> {
    state.history.days()
}

pub fn page(
    state: &AppState,
    date: &str,
    page: usize,
    per_page: usize,
    failed_only: bool,
) -> Result<HistoryPage, String> {
    state.history.read_page(date, page, per_page, failed_only)
}

pub fn search(state: &AppState, query: &str, page: usize, per_page: usize) -> Result<HistoryPage, String> {
    state.history.search(query, page, per_page)
}
