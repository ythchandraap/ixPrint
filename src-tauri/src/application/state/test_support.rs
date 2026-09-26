use super::*;
use crate::application::ports::*;
use crate::domain::api_access::ApiAccessEntry;
use crate::domain::callback::{Attempt, Callback, Payload};
use crate::domain::diagnostics::DiagnosticEntry;
use crate::domain::history::{CallbackStatus, HistoryDay, HistoryEntry, HistoryPage};
use crate::domain::incoming_job::IncomingJob;
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

mod stores;
use stores::{MemoryApiAccessLog, MemoryConfig, MemoryDiagnostics, MemoryHistory, MemoryIdempotency, MemoryRemotes};

#[derive(Default)]
struct EmptyIncomingDocuments;

impl IncomingDocumentRepository for EmptyIncomingDocuments {
    fn create(&self, _job: IncomingJob, _source: &Path) -> Result<(IncomingJob, bool), String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn canonical_file(&self, _path: &Path, _max_bytes: u64) -> Result<PathBuf, String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn staging_path(&self, _id: &str) -> Result<PathBuf, String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn copy_to_staging(&self, _id: &str, _source: &Path) -> Result<PathBuf, String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn discard_staging(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }
    fn list(&self) -> Result<Vec<IncomingJob>, String> {
        Ok(Vec::new())
    }
    fn get(&self, _id: &str) -> Result<Option<IncomingJob>, String> {
        Ok(None)
    }
    fn document_path(&self, _id: &str) -> Result<PathBuf, String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn save(&self, _job: &IncomingJob) -> Result<(), String> {
        Err("Incoming document storage is unavailable in test state".into())
    }
    fn claim_for_submission(&self, _id: &str, _printer: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn remove_document(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }
    fn prune(&self, _retention_days: u32) -> Result<(), String> {
        Ok(())
    }
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
    fn engine_status(&self) -> Result<(), String> {
        Err("PDF engine unavailable in test state".into())
    }

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
    fn page_count(&self, _path: &Path) -> Result<usize, String> {
        Err("PDF engine unavailable in test state".into())
    }
    fn test_page(&self, _path: &Path, _w: f32, _h: f32, _info: &[String], _pages: u32) -> Result<(), String> {
        Err("PDF engine unavailable in test state".into())
    }
    fn remove_generated_file(&self, _path: &Path) {}
}

#[derive(Default)]
struct OfflineRemote;

impl RemoteGateway for OfflineRemote {
    fn shared_presets(&self, _remote: &Remote) -> Result<crate::domain::remote::SharedPresetListing, RemoteError> {
        Err(RemoteError::Failed("Offline in test state".into()))
    }
    fn fetch_shared_preset(&self, _remote: &Remote, _name: &str) -> Result<Vec<u8>, RemoteError> {
        Err(RemoteError::Failed("Offline in test state".into()))
    }
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
        _logical_job_id: &str,
    ) -> Result<RemotePrintResult, RemoteError> {
        Err(RemoteError::Failed("Offline in test state".into()))
    }
    fn test_page(
        &self,
        _remote: &Remote,
        _pages: u32,
        _settings: &PrintSettings,
        _logical_job_id: &str,
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
        idempotency: Arc::new(MemoryIdempotency::default()),
        incoming_documents: Arc::new(EmptyIncomingDocuments),
        diagnostics: Arc::new(MemoryDiagnostics::default()),
        api_access_log: Arc::new(MemoryApiAccessLog::default()),
        printer_gateway: Arc::new(EmptyPrinter),
        printer_admin: Arc::new(EmptyPrinterAdmin),
        pdf_engine: Arc::new(UnavailablePdf),
        remote_gateway: Arc::new(OfflineRemote),
        callback_sender: Arc::new(NoopCallback),
        clock: Arc::new(SystemClock),
        jobs_dir: root.join("jobs"),
    })
}
