use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::application::ports::{
    ApiAccessLogRepository, CallbackSender, Clock, ConfigRepository, DiagnosticsRepository, HistoryRepository,
    IdempotencyRepository, PdfEngine, PrinterAdmin, PrinterGateway, RemoteGateway, RemoteRepository, ServerControl,
};
use crate::application::security::{FailedAttempts, InFlight, TokenUsage};
use crate::domain::remote::CacheEntry;

pub struct AppState {
    pub config: Arc<dyn ConfigRepository>,
    /// Random in-process credential used only by the desktop window's local HTTP client.
    pub desktop_key: String,
    pub remotes: Arc<dyn RemoteRepository>,
    pub remote_cache: Mutex<HashMap<String, CacheEntry>>,
    pub server: Arc<dyn ServerControl>,
    pub token_usage: TokenUsage,
    pub in_flight: InFlight,
    pub failed_auth: FailedAttempts,
    pub history: Arc<dyn HistoryRepository>,
    pub idempotency: Arc<dyn IdempotencyRepository>,
    pub diagnostics: Arc<dyn DiagnosticsRepository>,
    pub api_access_log: Arc<dyn ApiAccessLogRepository>,
    pub printer_gateway: Arc<dyn PrinterGateway>,
    pub printer_admin: Arc<dyn PrinterAdmin>,
    pub pdf_engine: Arc<dyn PdfEngine>,
    pub remote_gateway: Arc<dyn RemoteGateway>,
    pub callback_sender: Arc<dyn CallbackSender>,
    pub clock: Arc<dyn Clock>,
    pub jobs_dir: PathBuf,
}

impl AppState {
    /// The printer used when neither the request nor the preset names one.
    pub fn default_printer(&self) -> Option<String> {
        self.config
            .get()
            .default_printer
            .or_else(|| self.printer_gateway.os_default_printer())
    }
}

#[cfg(test)]
pub mod test_support;
