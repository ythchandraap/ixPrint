use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use serde_json::Value;

use crate::domain::api_access::ApiAccessEntry;
use crate::domain::callback::{Attempt, Callback, Payload};
use crate::domain::diagnostics::DiagnosticEntry;
use crate::domain::history::{CallbackStatus, HistoryDay, HistoryEntry, HistoryPage, HistoryPdfKind};
use crate::domain::incoming_job::IncomingJob;
use crate::domain::incoming_job::IncomingSource;
use crate::domain::layout::{LayoutResult, Sheet};
use crate::domain::printer::{Capabilities, DeviceOptions, JobStatus, Printer, PrinterQueue};
use crate::domain::printer_admin::{Device, Driver, NewPrinter, PrinterDetails, PrinterEdit};
use crate::domain::remote::{ReadyPrintResult, Remote, RemoteError, RemotePrintResult, SharedPresetListing};
use crate::domain::settings::{JobSettings, PrintSettings};
use crate::domain::settings_config::Config;
use crate::domain::template::{Record, Template};

pub trait ConfigRepository: Send + Sync {
    fn get(&self) -> Config;
    /// Applies the mutator once to a copy, validates it, then commits the result.
    fn update(&self, change: &mut dyn FnMut(&mut Config) -> Result<(), String>) -> Result<Config, String>;
    fn data_dir(&self) -> PathBuf;
}

pub enum Reservation {
    New,
    Existing(String),
    Pending,
    Conflict,
}

pub trait IdempotencyRepository: Send + Sync {
    /// Atomically reserve a key before the spooler is contacted.
    fn reserve(&self, key_hash: &str, fingerprint: &str, job_id: &str) -> Result<Reservation, String>;
}

pub trait RemoteRepository: Send + Sync {
    fn list(&self) -> Vec<Remote>;
    fn update(&self, change: &mut dyn FnMut(&mut Vec<Remote>) -> Result<(), String>) -> Result<Vec<Remote>, String>;
}

pub trait HistoryRepository: Send + Sync {
    fn append(&self, entry: &HistoryEntry) -> Result<(), String>;
    fn append_callback(&self, job_id: &str, status: CallbackStatus) -> Result<(), String>;
    fn find(&self, job_id: &str) -> Option<HistoryEntry>;
    fn read_page(&self, date: &str, page: usize, per_page: usize, failed_only: bool) -> Result<HistoryPage, String>;
    fn search(&self, query: &str, page: usize, per_page: usize) -> Result<HistoryPage, String>;
    fn days(&self) -> Vec<HistoryDay>;
    fn prune(&self, retention_days: u32);
    fn content_job_started(&self, _job_id: &str) {}
    fn content_job_finished(&self, _job_id: &str) {}
    fn save_input_json(&self, _entry: &HistoryEntry, _value: &Value) -> Result<(), String> {
        Err("History content storage is unavailable".into())
    }
    fn save_input_pdf(&self, _entry: &HistoryEntry, _source: &Path) -> Result<(), String> {
        Err("History content storage is unavailable".into())
    }
    fn save_output_pdf(&self, _entry: &HistoryEntry, _source: &Path) -> Result<(), String> {
        Err("History content storage is unavailable".into())
    }
    fn read_input_json(&self, _entry: &HistoryEntry) -> Result<Option<Value>, String> {
        Ok(None)
    }
    fn pdf_path(&self, _entry: &HistoryEntry, _kind: HistoryPdfKind) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
}

pub trait IncomingDocumentRepository: Send + Sync {
    fn create(&self, job: IncomingJob, source: &Path) -> Result<(IncomingJob, bool), String>;
    fn canonical_file(&self, path: &Path, max_bytes: u64) -> Result<PathBuf, String>;
    fn staging_path(&self, id: &str) -> Result<PathBuf, String>;
    fn copy_to_staging(&self, id: &str, source: &Path) -> Result<PathBuf, String>;
    fn discard_staging(&self, id: &str) -> Result<(), String>;
    fn list(&self) -> Result<Vec<IncomingJob>, String>;
    fn get(&self, id: &str) -> Result<Option<IncomingJob>, String>;
    fn document_path(&self, id: &str) -> Result<PathBuf, String>;
    fn save(&self, job: &IncomingJob) -> Result<(), String>;
    fn claim_for_submission(&self, id: &str, printer: &str) -> Result<bool, String>;
    fn remove_document(&self, id: &str) -> Result<(), String>;
    fn prune(&self, retention_days: u32) -> Result<(), String>;
}

pub trait JobMarker: Send + Sync {}

pub trait DiagnosticsRepository: Send + Sync {
    fn begin_job(&self, id: &str, source: &str, printer: &str) -> Result<Box<dyn JobMarker>, String>;
    fn record(&self, level: &str, source: &str, message: &str, details: Option<&str>) -> Result<(), String>;
    fn read(&self) -> Result<Vec<DiagnosticEntry>, String>;
    fn clear(&self) -> Result<(), String>;
    fn finish_session(&self);
}

pub trait ApiAccessLogRepository: Send + Sync {
    fn append(&self, entry: &ApiAccessEntry) -> Result<(), String>;
    fn read(&self) -> Result<Vec<ApiAccessEntry>, String>;
    fn clear(&self) -> Result<(), String>;
}

pub trait PrinterGateway: Send + Sync {
    fn list_printers(&self) -> Vec<Printer>;
    fn os_default_printer(&self) -> Option<String>;
    fn capabilities(&self, printer: &str) -> Result<Capabilities, String>;
    fn default_paper_mm(&self, printer: &str) -> (f32, f32);
    fn print(
        &self,
        printer: &str,
        pdf: &Path,
        sheets: &[Sheet],
        options: &DeviceOptions,
    ) -> Result<Option<String>, String>;
    fn queue(&self, printer: &str) -> Result<PrinterQueue, String>;
    fn job_status(&self, printer: &str, spooler_job: &str) -> Result<JobStatus, String>;
    fn cancel(&self, _printer: &str, _spooler_job: &str) -> Result<(), String> {
        Err("Queue cancellation is unavailable on this printer adapter".into())
    }
}

pub trait PrinterAdmin: Send + Sync {
    fn devices(&self) -> Result<Vec<Device>, String>;
    fn drivers(&self, query: &str) -> Result<Vec<Driver>, String>;
    fn details(&self, name: &str) -> Result<PrinterDetails, String>;
    fn add(&self, printer: &NewPrinter) -> Result<(), String>;
    fn update(&self, name: &str, edit: &PrinterEdit) -> Result<(), String>;
    fn remove(&self, name: &str) -> Result<(), String>;
    fn set_os_default(&self, name: &str) -> Result<(), String>;
}

pub trait PdfEngine: Send + Sync {
    fn engine_status(&self) -> Result<(), String>;
    fn build_layout(
        &self,
        source: &Path,
        output: &Path,
        settings: &JobSettings,
        default_paper_mm: (f32, f32),
    ) -> Result<LayoutResult, String>;
    fn render_template(
        &self,
        path: &Path,
        template: &Template,
        records: &[Record],
        header: &Record,
    ) -> Result<(), String>;
    fn pages_png(
        &self,
        template: &Template,
        records: &[Record],
        header: &Record,
        max_pages: usize,
        px_per_mm: f32,
    ) -> Result<(Vec<Vec<u8>>, usize), String>;
    fn preview_png(&self, template: &Template, record: &Record, px_per_mm: f32) -> Result<Vec<u8>, String>;
    fn render_page_png(&self, path: &Path, page: usize, px_per_mm: f32) -> Result<Vec<u8>, String>;
    fn page_count(&self, path: &Path) -> Result<usize, String>;
    fn page_sizes(&self, _path: &Path) -> Result<Vec<crate::domain::incoming_job::PageMedia>, String> {
        Err("Page size inspection is unavailable".into())
    }
    fn image_to_pdf(&self, _source: &Path, _output: &Path) -> Result<(), String> {
        Err("Image conversion is unavailable".into())
    }
    fn test_page(&self, path: &Path, width_mm: f32, height_mm: f32, info: &[String], pages: u32) -> Result<(), String>;
    fn remove_generated_file(&self, path: &Path);
}

pub trait RemoteGateway: Send + Sync {
    fn shared_presets(&self, remote: &Remote) -> Result<SharedPresetListing, RemoteError>;
    fn fetch_shared_preset(&self, remote: &Remote, name: &str) -> Result<Vec<u8>, RemoteError>;
    fn fetch_printers(&self, remote: &Remote) -> Result<Vec<Printer>, RemoteError>;
    fn capabilities(&self, remote: &Remote, name: &str) -> Result<Capabilities, RemoteError>;
    fn queue(&self, remote: &Remote, name: &str) -> Result<PrinterQueue, RemoteError>;
    fn job_status(&self, remote: &Remote, job_id: &str) -> Result<Value, RemoteError>;
    fn print_pdf(
        &self,
        remote: &Remote,
        path: &Path,
        file_name: &str,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError>;
    fn print_ready_pdf(
        &self,
        _remote: &Remote,
        _path: &Path,
        _file_name: &str,
        _source: IncomingSource,
        _source_job_id: &str,
        _copies: u32,
        _printer: &str,
    ) -> Result<ReadyPrintResult, RemoteError> {
        Err(RemoteError::Failed(
            "Ready-PDF forwarding is unavailable on this remote adapter".into(),
        ))
    }
    fn test_page(
        &self,
        remote: &Remote,
        pages: u32,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError>;
}

pub trait CallbackSender: Send + Sync {
    fn deliver(&self, callback: &Callback, payload: &Payload, progress: &mut dyn FnMut(&str, u32, &Attempt));
}

pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

pub trait ServerControl: Send + Sync {
    fn start(&self, state: std::sync::Arc<crate::application::state::AppState>);
    fn stop(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    fn is_running(&self) -> bool;
    fn last_error(&self) -> Option<String>;
    fn clear_error(&self);
}
