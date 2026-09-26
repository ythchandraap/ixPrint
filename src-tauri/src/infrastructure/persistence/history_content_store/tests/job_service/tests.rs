use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::application::job_service::run;
use crate::application::ports::{HistoryRepository, PdfEngine, PrinterGateway};
use crate::application::state::test_support;
use crate::application::state::AppState;
use crate::domain::history::{CallbackStatus, HistoryDay, HistoryEntry, HistoryPage};
use crate::domain::history::{HistoryInputKind, HistoryOutputState};
use crate::domain::job::{Document, JobError, JobRequest};
use crate::domain::layout::{LayoutResult, Sheet};
use crate::domain::printer::{Capabilities, DeviceOptions, JobStatus, Printer, PrinterQueue};
use crate::domain::settings::{JobSettings, PrintSettings};
use crate::domain::template::{Record, Template};
use crate::infrastructure::persistence::history_store::History;

#[derive(Default)]
struct FakePrinter {
    present: bool,
    print_error: bool,
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
        if self.print_error {
            Err("fake printer failure".into())
        } else {
            Ok(Some("spooler-1".into()))
        }
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
        output: &Path,
        _settings: &JobSettings,
        _paper: (f32, f32),
    ) -> Result<LayoutResult, String> {
        if self.layout_error {
            return Err("fake layout failure".into());
        }
        write_fake_pdf(output, b"%PDF-final-output")?;
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
        path: &Path,
        _template: &Template,
        _records: &[Record],
        _header: &Record,
    ) -> Result<(), String> {
        write_fake_pdf(path, b"%PDF-design-sample")
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
        Ok(b"fake PNG page".to_vec())
    }
    fn page_count(&self, _path: &Path) -> Result<usize, String> {
        Ok(1)
    }
    fn test_page(&self, path: &Path, _width: f32, _height: f32, _info: &[String], _pages: u32) -> Result<(), String> {
        write_fake_pdf(path, b"%PDF-test-page")
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

fn state(printer: Arc<FakePrinter>, pdf: Arc<FakePdf>, history: Arc<dyn HistoryRepository>) -> AppState {
    let mut state = Arc::try_unwrap(test_support::state()).ok().unwrap();
    state.printer_gateway = printer;
    state.pdf_engine = pdf;
    state.history = history;
    state
}

fn write_fake_pdf(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn request(preset: Option<&str>) -> JobRequest {
    JobRequest {
        job_id: None,
        document: Document::File(PathBuf::from("source.pdf")),
        history_input: None,
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

fn disk_history(root: &Path) -> Arc<History> {
    Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ))
}

fn install_design(state: &AppState) -> Template {
    let design: Template = serde_json::from_value(serde_json::json!({
        "width_mm": 60,
        "height_mm": 40,
        "sample": {"sku": "SAMPLE-01"},
        "elements": []
    }))
    .unwrap();
    state
        .config
        .update(&mut |config| {
            config.presets.insert(
                "designed".into(),
                crate::domain::preset::Preset {
                    settings: PrintSettings::default(),
                    design: Some(design.clone()),
                },
            );
            Ok(())
        })
        .unwrap();
    design
}

fn record(pairs: &[(&str, &str)]) -> Record {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn json_job_archives_the_received_body_and_final_pdf_outside_history_jsonl() {
    let root = std::env::temp_dir().join(format!("ixprint-job-content-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let history = disk_history(&root);
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 1,
        }),
        history.clone(),
    );
    install_design(&state);
    let job_id = uuid::Uuid::new_v4().to_string();
    let input = serde_json::json!({
        "preset": "designed",
        "data": {"sku": "SECRET_INPUT_VALUE"},
        "header": {"batch": "B-7"},
        "settings": {"printer": "Office", "copies": 1},
        "callback_ref": "order-7"
    });
    let request = JobRequest {
        job_id: Some(job_id.clone()),
        document: Document::Data {
            records: vec![record(&[("sku", "SECRET_INPUT_VALUE")])],
            header: record(&[("batch", "B-7")]),
        },
        history_input: Some(input.clone()),
        file_name: "design designed (1 item(s))".into(),
        settings: PrintSettings {
            printer: Some("Office".into()),
            ..Default::default()
        },
        preset: Some("designed".into()),
        source: "http",
        client: None,
        token: None,
        hop: false,
    };
    run(&state, request).unwrap();

    let entry = history.find(&job_id).unwrap();
    assert_eq!(entry.content.as_ref().unwrap().input_kind, Some(HistoryInputKind::Json));
    assert_eq!(entry.content.as_ref().unwrap().output, HistoryOutputState::Saved);
    assert_eq!(history.read_input_json(&entry).unwrap(), Some(input));
    let day = &entry.time[..10];
    let content_dir = root.join("history-content").join(day).join(&job_id);
    assert!(content_dir.join("input.json").is_file());
    assert_eq!(
        std::fs::read(content_dir.join("output.pdf")).unwrap(),
        b"%PDF-final-output"
    );
    let jsonl = std::fs::read_to_string(root.join("history").join(format!("{day}.jsonl"))).unwrap();
    assert!(!jsonl.contains("SECRET_INPUT_VALUE"));
    let page = history.read_page(day, 1, 20, false).unwrap();
    assert!(!serde_json::to_string(&page).unwrap().contains("SECRET_INPUT_VALUE"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn uploaded_pdf_and_pre_render_failure_preserve_only_received_input() {
    let root = std::env::temp_dir().join(format!("ixprint-job-fail-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input_path = root.join("upload.pdf");
    std::fs::write(&input_path, b"%PDF-original-upload").unwrap();
    let history = disk_history(&root);
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: true,
            sheet_count: 1,
        }),
        history.clone(),
    );
    let job_id = uuid::Uuid::new_v4().to_string();
    let mut request = request(None);
    request.job_id = Some(job_id.clone());
    request.document = Document::File(input_path);
    assert!(matches!(run(&state, request), Err(JobError::Failed(message)) if message == "fake layout failure"));

    let entry = history.find(&job_id).unwrap();
    assert!(!entry.success);
    assert_eq!(entry.error.as_deref(), Some("fake layout failure"));
    assert_eq!(entry.content.as_ref().unwrap().input_kind, Some(HistoryInputKind::Pdf));
    assert!(entry.content.as_ref().unwrap().input_saved);
    assert_eq!(entry.content.as_ref().unwrap().output, HistoryOutputState::NotCreated);
    let content_dir = root.join("history-content").join(&entry.time[..10]).join(&job_id);
    assert_eq!(
        std::fs::read(content_dir.join("input.pdf")).unwrap(),
        b"%PDF-original-upload"
    );
    assert!(!content_dir.join("output.pdf").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn successful_pdf_job_keeps_both_the_uploaded_and_print_ready_documents() {
    let root = std::env::temp_dir().join(format!("ixprint-pdf-success-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input_path = root.join("upload.pdf");
    std::fs::write(&input_path, b"%PDF-original-upload").unwrap();
    let history = disk_history(&root);
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 1,
        }),
        history.clone(),
    );
    let job_id = uuid::Uuid::new_v4().to_string();
    let mut request = request(None);
    request.job_id = Some(job_id.clone());
    request.document = Document::File(input_path);
    run(&state, request).unwrap();

    let entry = history.find(&job_id).unwrap();
    let content_dir = root.join("history-content").join(&entry.time[..10]).join(&job_id);
    assert_eq!(
        std::fs::read(content_dir.join("input.pdf")).unwrap(),
        b"%PDF-original-upload"
    );
    assert_eq!(
        std::fs::read(content_dir.join("output.pdf")).unwrap(),
        b"%PDF-final-output"
    );
    assert_eq!(entry.content.as_ref().unwrap().output, HistoryOutputState::Saved);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn printer_rejection_keeps_the_final_pdf_for_error_investigation() {
    let root = std::env::temp_dir().join(format!("ixprint-job-printer-error-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let input_path = root.join("upload.pdf");
    std::fs::write(&input_path, b"%PDF-original-upload").unwrap();
    let history = disk_history(&root);
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            print_error: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 1,
        }),
        history.clone(),
    );
    let job_id = uuid::Uuid::new_v4().to_string();
    let mut request = request(None);
    request.job_id = Some(job_id.clone());
    request.document = Document::File(input_path);
    assert!(matches!(run(&state, request), Err(JobError::Failed(message)) if message == "fake printer failure"));

    let entry = history.find(&job_id).unwrap();
    assert!(!entry.success);
    assert_eq!(entry.error.as_deref(), Some("fake printer failure"));
    assert_eq!(entry.content.as_ref().unwrap().output, HistoryOutputState::Saved);
    let content_dir = root.join("history-content").join(&entry.time[..10]).join(&job_id);
    assert_eq!(
        std::fs::read(content_dir.join("output.pdf")).unwrap(),
        b"%PDF-final-output"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn local_test_print_records_sample_only_when_the_design_uses_it() {
    let root = std::env::temp_dir().join(format!("ixprint-test-content-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let history = disk_history(&root);
    let state = state(
        Arc::new(FakePrinter {
            present: true,
            ..Default::default()
        }),
        Arc::new(FakePdf {
            layout_error: false,
            sheet_count: 1,
        }),
        history.clone(),
    );
    let design = install_design(&state);

    let no_sample_id = uuid::Uuid::new_v4().to_string();
    let mut plain_test = request(None);
    plain_test.job_id = Some(no_sample_id.clone());
    plain_test.document = Document::TestPage { pages: 1 };
    plain_test.source = "app";
    run(&state, plain_test).unwrap();
    let no_sample = history.find(&no_sample_id).unwrap();
    assert_eq!(no_sample.content.as_ref().unwrap().input_kind, None);
    assert_eq!(no_sample.content.as_ref().unwrap().output, HistoryOutputState::Saved);

    let sample_id = uuid::Uuid::new_v4().to_string();
    let mut design_test = request(Some("designed"));
    design_test.job_id = Some(sample_id.clone());
    design_test.document = Document::TestPage { pages: 1 };
    design_test.source = "app";
    run(&state, design_test).unwrap();
    let with_sample = history.find(&sample_id).unwrap();
    assert_eq!(
        with_sample.content.as_ref().unwrap().input_kind,
        Some(HistoryInputKind::Sample)
    );
    assert_eq!(
        history.read_input_json(&with_sample).unwrap(),
        Some(serde_json::json!({ "data": [design.sample], "header": design.sample }))
    );
    let _ = std::fs::remove_dir_all(root);
}
