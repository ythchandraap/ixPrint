use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::application::ports::{HistoryRepository, PdfEngine, PrinterGateway};
use crate::application::state::test_support;
use crate::domain::history::{CallbackStatus, HistoryDay, HistoryPage};
use crate::domain::layout::{LayoutResult, Sheet};
use crate::domain::printer::{Capabilities, JobStatus, Printer, PrinterQueue};
use crate::domain::settings::PrintSettings;
use crate::domain::template::{Record, Template};

#[derive(Default)]
struct FakePrinter {
    present: bool,
    print_calls: Mutex<Vec<(usize, u32)>>,
}

impl PrinterGateway for FakePrinter {
    fn list_printers(&self) -> Vec<Printer> {
        if self.present {
            vec![Printer {
                name: "Office".into(),
                status: "Ready".into(),
                message: None,
                shared: String::new(),
                os_default: false,
            }]
        } else {
            Vec::new()
        }
    }
    fn os_default_printer(&self) -> Option<String> {
        None
    }
    fn capabilities(&self, _printer: &str) -> Result<Capabilities, String> {
        Ok(Capabilities::default())
    }
    fn default_paper_mm(&self, _printer: &str) -> (f32, f32) {
        (210.0, 297.0)
    }
    fn print(
        &self,
        _printer: &str,
        _pdf: &Path,
        sheets: &[Sheet],
        options: &DeviceOptions,
    ) -> Result<Option<String>, String> {
        self.print_calls.lock().unwrap().push((sheets.len(), options.copies));
        Ok(Some("spooler-1".into()))
    }
    fn queue(&self, _printer: &str) -> Result<PrinterQueue, String> {
        unreachable!()
    }
    fn job_status(&self, _printer: &str, _spooler_job: &str) -> Result<JobStatus, String> {
        unreachable!()
    }
}

struct FakePdf {
    layout_error: bool,
    sheet_count: usize,
}

impl PdfEngine for FakePdf {
    fn engine_status(&self) -> Result<(), String> {
        Ok(())
    }

    fn build_layout(
        &self,
        _source: &Path,
        _output: &Path,
        _settings: &JobSettings,
        _paper: (f32, f32),
    ) -> Result<LayoutResult, String> {
        if self.layout_error {
            return Err("fake layout failure".into());
        }
        Ok(LayoutResult {
            sheets: (0..self.sheet_count)
                .map(|_| Sheet {
                    width_pt: 1.0,
                    height_pt: 1.0,
                })
                .collect(),
        })
    }
    fn render_template(
        &self,
        _path: &Path,
        _template: &Template,
        _records: &[Record],
        _header: &Record,
    ) -> Result<(), String> {
        Ok(())
    }
    fn pages_png(
        &self,
        _template: &Template,
        _records: &[Record],
        _header: &Record,
        _max: usize,
        _scale: f32,
    ) -> Result<(Vec<Vec<u8>>, usize), String> {
        unreachable!()
    }
    fn preview_png(&self, _template: &Template, _record: &Record, _scale: f32) -> Result<Vec<u8>, String> {
        unreachable!()
    }
    fn render_page_png(&self, _path: &Path, _page: usize, _scale: f32) -> Result<Vec<u8>, String> {
        unreachable!()
    }
    fn test_page(&self, _path: &Path, _width: f32, _height: f32, _info: &[String], _pages: u32) -> Result<(), String> {
        Ok(())
    }
    fn remove_generated_file(&self, _path: &Path) {}
}

#[derive(Default)]
struct FakeHistory(Mutex<Vec<HistoryEntry>>);
impl HistoryRepository for FakeHistory {
    fn append(&self, entry: &HistoryEntry) -> Result<(), String> {
        self.0.lock().unwrap().push(entry.clone());
        Ok(())
    }
    fn append_callback(&self, _job_id: &str, _status: CallbackStatus) -> Result<(), String> {
        Ok(())
    }
    fn find(&self, _job_id: &str) -> Option<HistoryEntry> {
        None
    }
    fn read_page(
        &self,
        _date: &str,
        _page: usize,
        _per_page: usize,
        _failed_only: bool,
    ) -> Result<HistoryPage, String> {
        unreachable!()
    }
    fn days(&self) -> Vec<HistoryDay> {
        Vec::new()
    }
    fn search(&self, _query: &str, _page: usize, _per_page: usize) -> Result<HistoryPage, String> {
        unreachable!()
    }
    fn prune(&self, _retention_days: u32) {}
}

fn state(printer: Arc<FakePrinter>, pdf: Arc<FakePdf>, history: Arc<FakeHistory>) -> AppState {
    let mut state = Arc::try_unwrap(test_support::state()).ok().unwrap();
    state.printer_gateway = printer;
    state.pdf_engine = pdf;
    state.history = history;
    state
}

fn request(preset: Option<&str>) -> JobRequest {
    JobRequest {
        job_id: None,
        document: Document::File(PathBuf::from("source.pdf")),
        file_name: "source.pdf".into(),
        settings: PrintSettings {
            printer: Some("Office".into()),
            copies: Some(3),
            ..Default::default()
        },
        preset: preset.map(str::to_string),
        source: "http",
        client: None,
        token: None,
        hop: false,
    }
}

#[test]
fn printer_not_found_is_bad_request() {
    let state = state(
        Arc::new(FakePrinter::default()),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 2,
        }),
        Arc::new(FakeHistory::default()),
    );
    assert!(
        matches!(run(&state, request(None)), Err(JobError::BadRequest(message)) if message.contains("was not found"))
    );
}

#[test]
fn unknown_preset_is_bad_request() {
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 2,
        }),
        Arc::new(FakeHistory::default()),
    );
    assert!(
        matches!(run(&state, request(Some("missing"))), Err(JobError::BadRequest(message)) if message == "Preset 'missing' was not found")
    );
}

#[test]
fn pdf_engine_failure_is_failed_job() {
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: true,
            sheet_count: 2,
        }),
        Arc::new(FakeHistory::default()),
    );
    assert!(matches!(run(&state, request(None)), Err(JobError::Failed(message)) if message == "fake layout failure"));
}

#[test]
fn success_records_history_and_prints_expected_sheet_count() {
    let printer = Arc::new(FakePrinter {
        present: true,
        ..Default::default()
    });
    let pdf = Arc::new(FakePdf {
        layout_error: false,
        sheet_count: 2,
    });
    let history = Arc::new(FakeHistory::default());
    let state = state(printer.clone(), pdf, history.clone());
    let result = run(&state, request(None)).unwrap();
    assert_eq!(result.sheets, 6);
    assert_eq!(*printer.print_calls.lock().unwrap(), vec![(2, 3)]);
    let entries = history.0.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].success);
    assert_eq!(entries[0].sheets, 6);
}
