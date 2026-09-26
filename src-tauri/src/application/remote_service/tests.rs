use super::*;
use crate::application::ports::{PdfEngine, RemoteGateway};
use crate::application::state::test_support::state;
use crate::domain::layout::LayoutResult;
use crate::domain::printer::{Capabilities, Printer, PrinterQueue};
use crate::domain::remote::{Remote, RemoteError, RemotePrintResult, SharedPresetListing};
use crate::domain::settings::{JobSettings, PrintSettings};
use crate::domain::template::{Record, Template};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
enum Forwarded {
    Pdf(String),
    TestPage(String),
}

#[derive(Default)]
struct RecordingRemote(Mutex<Vec<Forwarded>>);

impl RemoteGateway for RecordingRemote {
    fn shared_presets(&self, _: &Remote) -> Result<SharedPresetListing, RemoteError> {
        unreachable!()
    }
    fn fetch_shared_preset(&self, _: &Remote, _: &str) -> Result<Vec<u8>, RemoteError> {
        unreachable!()
    }
    fn fetch_printers(&self, _: &Remote) -> Result<Vec<Printer>, RemoteError> {
        unreachable!()
    }
    fn capabilities(&self, _: &Remote, _: &str) -> Result<Capabilities, RemoteError> {
        unreachable!()
    }
    fn queue(&self, _: &Remote, _: &str) -> Result<PrinterQueue, RemoteError> {
        unreachable!()
    }
    fn job_status(&self, _: &Remote, _: &str) -> Result<serde_json::Value, RemoteError> {
        unreachable!()
    }
    fn print_pdf(
        &self,
        _: &Remote,
        _: &Path,
        _: &str,
        _: &PrintSettings,
        logical_job_id: &str,
    ) -> Result<RemotePrintResult, RemoteError> {
        self.0.lock().unwrap().push(Forwarded::Pdf(logical_job_id.into()));
        Ok(RemotePrintResult {
            job_id: "remote-job".into(),
            sheets: 1,
        })
    }
    fn test_page(
        &self,
        _: &Remote,
        _: u32,
        _: &PrintSettings,
        logical_job_id: &str,
    ) -> Result<RemotePrintResult, RemoteError> {
        self.0.lock().unwrap().push(Forwarded::TestPage(logical_job_id.into()));
        Ok(RemotePrintResult {
            job_id: "remote-test-job".into(),
            sheets: 1,
        })
    }
}

struct PdfStub;

impl PdfEngine for PdfStub {
    fn engine_status(&self) -> Result<(), String> {
        Ok(())
    }
    fn build_layout(&self, _: &Path, _: &Path, _: &JobSettings, _: (f32, f32)) -> Result<LayoutResult, String> {
        unreachable!()
    }
    fn render_template(&self, _: &Path, _: &Template, _: &[Record], _: &Record) -> Result<(), String> {
        Ok(())
    }
    fn pages_png(
        &self,
        _: &Template,
        _: &[Record],
        _: &Record,
        _: usize,
        _: f32,
    ) -> Result<(Vec<Vec<u8>>, usize), String> {
        unreachable!()
    }
    fn preview_png(&self, _: &Template, _: &Record, _: f32) -> Result<Vec<u8>, String> {
        unreachable!()
    }
    fn render_page_png(&self, _: &Path, _: usize, _: f32) -> Result<Vec<u8>, String> {
        unreachable!()
    }
    fn page_count(&self, _: &Path) -> Result<usize, String> {
        unreachable!()
    }
    fn test_page(&self, _: &Path, _: f32, _: f32, _: &[String], _: u32) -> Result<(), String> {
        unreachable!()
    }
    fn remove_generated_file(&self, _: &Path) {}
}

fn request(document: Document, preset: Option<&str>) -> JobRequest {
    JobRequest {
        job_id: None,
        document,
        history_input: None,
        file_name: "remote-job.pdf".into(),
        settings: PrintSettings::default(),
        preset: preset.map(str::to_owned),
        source: "http",
        client: None,
        token: None,
        hop: false,
    }
}

fn record(id: &str) -> HistoryEntry {
    HistoryEntry {
        id: id.into(),
        time: String::new(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "remote-job.pdf".into(),
        preset: None,
        printer: String::new(),
        spooler_job: None,
        sheets: 0,
        settings: serde_json::json!({}),
        success: false,
        error: None,
        error_code: None,
        duration_ms: 0,
        callback: None,
        remote: None,
        content: None,
    }
}

fn remote() -> Remote {
    Remote {
        id: "remote-id".into(),
        name: "Office".into(),
        url: "http://office.local:18032".into(),
        token: "fxp_test-token".into(),
        preset: None,
        created_at: String::new(),
    }
}

#[test]
fn forward_rejects_second_hop_with_existing_error() {
    let state = state();
    let remote = Remote {
        id: "remote-id".into(),
        name: "Office".into(),
        url: "http://office.local:18032".into(),
        token: "fxp_test-token".into(),
        preset: None,
        created_at: String::new(),
    };
    let preset = crate::domain::preset::Preset::default();
    let req = JobRequest {
        job_id: None,
        document: Document::File("source.pdf".into()),
        history_input: None,
        file_name: "source.pdf".into(),
        settings: PrintSettings::default(),
        preset: None,
        source: "http",
        client: None,
        token: None,
        hop: true,
    };
    let mut record = HistoryEntry {
        id: "job-id".into(),
        time: String::new(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "source.pdf".into(),
        preset: None,
        printer: "Office / Printer".into(),
        spooler_job: None,
        sheets: 0,
        settings: serde_json::json!({}),
        success: false,
        error: None,
        error_code: None,
        duration_ms: 0,
        callback: None,
        remote: None,
        content: None,
    };
    assert!(matches!(
        forward(&state, ForwardRequest {
            job_id: "job-id",
            preset_name: None,
            preset: &preset,
            merged: &PrintSettings::default(),
            req: &req,
            remote: &remote,
            remote_printer: "Printer",
            record: &mut record,
        }),
        Err(JobError::BadRequest(message)) if message == "Forwarding between ix.print computers is limited to one hop"
    ));
}

#[test]
fn all_remote_print_paths_forward_the_logical_job_id() {
    let gateway = Arc::new(RecordingRemote::default());
    let mut owned_state = Arc::try_unwrap(state()).ok().unwrap();
    owned_state.remote_gateway = gateway.clone();
    owned_state.pdf_engine = Arc::new(PdfStub);
    let state = owned_state;
    let remote = remote();
    let plain = crate::domain::preset::Preset::default();
    let design: Template = serde_json::from_value(serde_json::json!({
        "width_mm": 60,
        "height_mm": 40,
        "sample": {"sku": "sample"},
        "elements": []
    }))
    .unwrap();
    let designed = crate::domain::preset::Preset {
        settings: PrintSettings::default(),
        design: Some(design.clone()),
    };
    let merged = PrintSettings::default();

    for (job_id, request, preset, preset_name) in [
        (
            "pdf-job",
            request(Document::File(PathBuf::from("source.pdf")), None),
            &plain,
            None,
        ),
        (
            "json-job",
            request(
                Document::Data {
                    records: vec![Record::new()],
                    header: Record::new(),
                },
                Some("label"),
            ),
            &designed,
            Some("label"),
        ),
        (
            "designed-test-job",
            request(Document::TestPage { pages: 1 }, Some("label")),
            &designed,
            Some("label"),
        ),
        ("test-job", request(Document::TestPage { pages: 1 }, None), &plain, None),
    ] {
        let mut history = record(job_id);
        forward(
            &state,
            ForwardRequest {
                job_id,
                preset_name,
                preset,
                merged: &merged,
                req: &request,
                remote: &remote,
                remote_printer: "Label",
                record: &mut history,
            },
        )
        .unwrap();
    }

    assert_eq!(
        *gateway.0.lock().unwrap(),
        vec![
            Forwarded::Pdf("pdf-job".into()),
            Forwarded::Pdf("json-job".into()),
            Forwarded::Pdf("designed-test-job".into()),
            Forwarded::TestPage("test-job".into()),
        ]
    );
}
