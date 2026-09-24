use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::application::ports::{
    CallbackSender, Clock, ConfigRepository, DiagnosticsRepository, HistoryRepository, PdfEngine, PrinterAdmin,
    PrinterGateway, RemoteGateway, RemoteRepository, ServerControl,
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
    pub diagnostics: Arc<dyn DiagnosticsRepository>,
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
pub mod test_support {
    use super::*;
    use crate::application::ports::*;
    use crate::callback::{Attempt, Callback, Payload};
    use crate::domain::diagnostics::DiagnosticEntry;
    use crate::domain::history::{CallbackStatus, HistoryDay, HistoryEntry, HistoryPage};
    use crate::domain::layout::{LayoutResult, Sheet};
    use crate::domain::printer::{Capabilities, DeviceOptions, JobStatus, Printer, PrinterQueue};
    use crate::domain::printer_admin::{Device, Driver, NewPrinter, PrinterDetails, PrinterEdit};
    use crate::domain::remote::{Remote, RemoteError, RemotePrintResult};
    use crate::domain::settings::{JobSettings, PrintSettings};
    use crate::domain::settings_config::{Config, CONFIG_VERSION};
    use crate::domain::template::{Record, Template};
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::Mutex;

    struct MemoryConfig(Mutex<Config>);

    impl ConfigRepository for MemoryConfig {
        fn get(&self) -> Config {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }

        fn update(&self, change: &mut dyn FnMut(&mut Config) -> Result<(), String>) -> Result<Config, String> {
            let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
            let mut next = current.clone();
            change(&mut next)?;
            next.version = CONFIG_VERSION;
            next.validate()?;
            *current = next.clone();
            Ok(next)
        }

        fn data_dir(&self) -> PathBuf {
            std::env::temp_dir()
        }
    }

    #[derive(Default)]
    struct MemoryRemotes(Mutex<Vec<Remote>>);

    impl RemoteRepository for MemoryRemotes {
        fn list(&self) -> Vec<Remote> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }

        fn update(
            &self,
            change: &mut dyn FnMut(&mut Vec<Remote>) -> Result<(), String>,
        ) -> Result<Vec<Remote>, String> {
            let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
            let mut next = current.clone();
            change(&mut next)?;
            *current = next.clone();
            Ok(next)
        }
    }

    #[derive(Default)]
    struct MemoryHistory(Mutex<Vec<HistoryEntry>>);

    impl HistoryRepository for MemoryHistory {
        fn append(&self, entry: &HistoryEntry) -> Result<(), String> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(entry.clone());
            Ok(())
        }

        fn append_callback(&self, job_id: &str, status: CallbackStatus) -> Result<(), String> {
            if let Some(entry) = self
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter_mut()
                .find(|entry| entry.id == job_id)
            {
                entry.callback = Some(status);
            }
            Ok(())
        }

        fn find(&self, job_id: &str) -> Option<HistoryEntry> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|entry| entry.id == job_id)
                .cloned()
        }

        fn read_page(
            &self,
            date: &str,
            page: usize,
            per_page: usize,
            failed_only: bool,
        ) -> Result<HistoryPage, String> {
            let mut all: Vec<_> = self
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|entry| entry.time.starts_with(date))
                .cloned()
                .collect();
            all.sort_by(|a, b| b.time.cmp(&a.time));
            let failed = all.iter().filter(|entry| !entry.success).count();
            let entries = if failed_only {
                all.into_iter().filter(|entry| !entry.success).collect()
            } else {
                all
            };
            let per_page = per_page.clamp(1, 200);
            let pages = entries.len().div_ceil(per_page).max(1);
            let page = page.clamp(1, pages);
            let total = entries.len();
            let entries = entries.into_iter().skip((page - 1) * per_page).take(per_page).collect();
            Ok(HistoryPage {
                entries,
                total,
                failed,
                page,
                pages,
                per_page,
            })
        }

        fn days(&self) -> Vec<HistoryDay> {
            let mut days = BTreeMap::<String, (usize, usize)>::new();
            for entry in self.0.lock().unwrap_or_else(|e| e.into_inner()).iter() {
                let date = entry.time.get(..10).unwrap_or_default().to_string();
                let counts = days.entry(date).or_default();
                counts.0 += 1;
                counts.1 += usize::from(!entry.success);
            }
            days.into_iter()
                .rev()
                .map(|(date, (total, failed))| HistoryDay { date, total, failed })
                .collect()
        }

        fn prune(&self, _retention_days: u32) {}
    }

    struct NoopJobMarker;
    impl JobMarker for NoopJobMarker {}

    #[derive(Default)]
    struct MemoryDiagnostics(Mutex<Vec<DiagnosticEntry>>);

    impl DiagnosticsRepository for MemoryDiagnostics {
        fn begin_job(&self, _id: &str, _source: &str, _printer: &str) -> Result<Box<dyn JobMarker>, String> {
            Ok(Box::new(NoopJobMarker))
        }

        fn record(&self, level: &str, source: &str, message: &str, details: Option<&str>) -> Result<(), String> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(DiagnosticEntry {
                time: chrono::Local::now().to_rfc3339(),
                level: level.into(),
                source: source.into(),
                message: message.into(),
                details: details.map(str::to_string),
            });
            Ok(())
        }

        fn read(&self) -> Result<Vec<DiagnosticEntry>, String> {
            Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone())
        }

        fn clear(&self) -> Result<(), String> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
            Ok(())
        }

        fn finish_session(&self) {}
    }

    #[derive(Default)]
    struct EmptyPrinter;

    impl PrinterGateway for EmptyPrinter {
        fn list_printers(&self) -> Vec<Printer> {
            vec![]
        }
        fn os_default_printer(&self) -> Option<String> {
            None
        }
        fn capabilities(&self, _printer: &str) -> Result<Capabilities, String> {
            Err("Printer not found".into())
        }
        fn default_paper_mm(&self, _printer: &str) -> (f32, f32) {
            (210.0, 297.0)
        }
        fn print(
            &self,
            _printer: &str,
            _pdf: &Path,
            _sheets: &[Sheet],
            _options: &DeviceOptions,
        ) -> Result<Option<String>, String> {
            Err("Printer unavailable in test state".into())
        }
        fn queue(&self, _printer: &str) -> Result<PrinterQueue, String> {
            Err("Printer not found".into())
        }
        fn job_status(&self, _printer: &str, _spooler_job: &str) -> Result<JobStatus, String> {
            Err("Printer not found".into())
        }
    }

    #[derive(Default)]
    struct EmptyPrinterAdmin;

    impl PrinterAdmin for EmptyPrinterAdmin {
        fn devices(&self) -> Result<Vec<Device>, String> {
            Ok(vec![])
        }
        fn drivers(&self, _query: &str) -> Result<Vec<Driver>, String> {
            Ok(vec![])
        }
        fn details(&self, _name: &str) -> Result<PrinterDetails, String> {
            Err("Printer not found".into())
        }
        fn add(&self, _printer: &NewPrinter) -> Result<(), String> {
            Err("Unavailable in test state".into())
        }
        fn update(&self, _name: &str, _edit: &PrinterEdit) -> Result<(), String> {
            Err("Unavailable in test state".into())
        }
        fn remove(&self, _name: &str) -> Result<(), String> {
            Err("Unavailable in test state".into())
        }
        fn set_os_default(&self, _name: &str) -> Result<(), String> {
            Err("Unavailable in test state".into())
        }
    }

    #[derive(Default)]
    struct UnavailablePdf;

    impl PdfEngine for UnavailablePdf {
        fn build_layout(
            &self,
            _source: &Path,
            _output: &Path,
            _settings: &JobSettings,
            _paper: (f32, f32),
        ) -> Result<LayoutResult, String> {
            Err("PDF engine unavailable in test state".into())
        }
        fn render_template(
            &self,
            _path: &Path,
            _template: &Template,
            _records: &[Record],
            _header: &Record,
        ) -> Result<(), String> {
            Err("PDF engine unavailable in test state".into())
        }
        fn pages_png(
            &self,
            _template: &Template,
            _records: &[Record],
            _header: &Record,
            _max: usize,
            _scale: f32,
        ) -> Result<(Vec<Vec<u8>>, usize), String> {
            Err("PDF engine unavailable in test state".into())
        }
        fn preview_png(&self, _template: &Template, _record: &Record, _scale: f32) -> Result<Vec<u8>, String> {
            Err("PDF engine unavailable in test state".into())
        }
        fn render_page_png(&self, _path: &Path, _page: usize, _scale: f32) -> Result<Vec<u8>, String> {
            Err("PDF engine unavailable in test state".into())
        }
        fn test_page(&self, _path: &Path, _w: f32, _h: f32, _info: &[String], _pages: u32) -> Result<(), String> {
            Err("PDF engine unavailable in test state".into())
        }
    }

    #[derive(Default)]
    struct OfflineRemote;

    impl RemoteGateway for OfflineRemote {
        fn fetch_printers(&self, _remote: &Remote) -> Result<Vec<Printer>, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
        fn capabilities(&self, _remote: &Remote, _name: &str) -> Result<Capabilities, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
        fn queue(&self, _remote: &Remote, _name: &str) -> Result<PrinterQueue, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
        fn job_status(&self, _remote: &Remote, _id: &str) -> Result<serde_json::Value, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
        fn print_pdf(
            &self,
            _remote: &Remote,
            _path: &Path,
            _file_name: &str,
            _settings: &PrintSettings,
        ) -> Result<RemotePrintResult, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
        fn test_page(
            &self,
            _remote: &Remote,
            _pages: u32,
            _settings: &PrintSettings,
        ) -> Result<RemotePrintResult, RemoteError> {
            Err(RemoteError::Failed("Offline in test state".into()))
        }
    }

    #[derive(Default)]
    struct NoopCallback;
    impl CallbackSender for NoopCallback {
        fn deliver(&self, _callback: &Callback, _payload: &Payload, _progress: &mut dyn FnMut(&str, u32, &Attempt)) {}
    }

    #[derive(Default)]
    struct SystemClock;
    impl Clock for SystemClock {
        fn now_rfc3339(&self) -> String {
            chrono::Local::now().to_rfc3339()
        }
    }

    #[derive(Default)]
    struct NoopServerControl;
    impl ServerControl for NoopServerControl {
        fn start(&self, _state: Arc<AppState>) {}
        fn stop(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async {})
        }
        fn is_running(&self) -> bool {
            false
        }
        fn last_error(&self) -> Option<String> {
            None
        }
        fn clear_error(&self) {}
    }

    pub fn state() -> Arc<AppState> {
        let config = Config {
            bridge_enabled: false,
            require_token: false,
            ..Config::default()
        };
        let root = std::env::temp_dir().join(format!("ixprint-state-{}", uuid::Uuid::new_v4()));
        Arc::new(AppState {
            config: Arc::new(MemoryConfig(Mutex::new(config))),
            desktop_key: "test-desktop-key".into(),
            remotes: Arc::new(MemoryRemotes::default()),
            remote_cache: Mutex::new(HashMap::new()),
            server: Arc::new(NoopServerControl),
            token_usage: TokenUsage::default(),
            in_flight: InFlight::default(),
            failed_auth: FailedAttempts::default(),
            history: Arc::new(MemoryHistory::default()),
            diagnostics: Arc::new(MemoryDiagnostics::default()),
            printer_gateway: Arc::new(EmptyPrinter),
            printer_admin: Arc::new(EmptyPrinterAdmin),
            pdf_engine: Arc::new(UnavailablePdf),
            remote_gateway: Arc::new(OfflineRemote),
            callback_sender: Arc::new(NoopCallback),
            clock: Arc::new(SystemClock),
            jobs_dir: root.join("jobs"),
        })
    }
}
