use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use serde_json::Value;

use crate::domain::callback::{Attempt, Callback, Payload};
use crate::domain::diagnostics::DiagnosticEntry;
use crate::domain::history::{CallbackStatus, HistoryDay, HistoryEntry, HistoryPage};
use crate::domain::layout::{LayoutResult, Sheet};
use crate::domain::printer::{Capabilities, DeviceOptions, JobStatus, Printer, PrinterQueue};
use crate::domain::printer_admin::{Device, Driver, NewPrinter, PrinterDetails, PrinterEdit};
use crate::domain::remote::{Remote, RemoteError, RemotePrintResult};
use crate::domain::settings::{JobSettings, PrintSettings};
use crate::domain::settings_config::Config;
use crate::domain::template::{Record, Template};

pub trait ConfigRepository: Send + Sync {
    fn get(&self) -> Config;
    /// Applies the mutator once to a copy, validates it, then commits the result.
    fn update(&self, change: &mut dyn FnMut(&mut Config) -> Result<(), String>) -> Result<Config, String>;
    fn data_dir(&self) -> PathBuf;
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
    fn days(&self) -> Vec<HistoryDay>;
    fn prune(&self, retention_days: u32);
}

pub trait JobMarker: Send + Sync {}

pub trait DiagnosticsRepository: Send + Sync {
    fn begin_job(&self, id: &str, source: &str, printer: &str) -> Result<Box<dyn JobMarker>, String>;
    fn record(&self, level: &str, source: &str, message: &str, details: Option<&str>) -> Result<(), String>;
    fn read(&self) -> Result<Vec<DiagnosticEntry>, String>;
    fn clear(&self) -> Result<(), String>;
    fn finish_session(&self);
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
    fn test_page(&self, path: &Path, width_mm: f32, height_mm: f32, info: &[String], pages: u32) -> Result<(), String>;
    fn remove_generated_file(&self, path: &Path);
}

pub trait RemoteGateway: Send + Sync {
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
