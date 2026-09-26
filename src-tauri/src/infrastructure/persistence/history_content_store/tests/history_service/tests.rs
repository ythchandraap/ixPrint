use crate::application::history_service::{self as service, ContentAvailability};
use crate::application::ports::{HistoryRepository, PdfEngine};
use crate::application::state::test_support;
use crate::domain::history::{HistoryContentStatus, HistoryEntry, HistoryInputKind, HistoryOutputState, RemoteJob};
use crate::domain::layout::LayoutResult;
use crate::domain::settings::JobSettings;
use crate::domain::template::{Record, Template};
use crate::infrastructure::persistence::history_store::History;
use std::path::Path;
use std::sync::Arc;

struct PreviewPdf {
    rendered_pages: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl PdfEngine for PreviewPdf {
    fn engine_status(&self) -> Result<(), String> {
        Ok(())
    }
    fn build_layout(&self, _: &Path, _: &Path, _: &JobSettings, _: (f32, f32)) -> Result<LayoutResult, String> {
        Err("unused".into())
    }
    fn render_template(&self, _: &Path, _: &Template, _: &[Record], _: &Record) -> Result<(), String> {
        Err("unused".into())
    }
    fn pages_png(
        &self,
        _: &Template,
        _: &[Record],
        _: &Record,
        _: usize,
        _: f32,
    ) -> Result<(Vec<Vec<u8>>, usize), String> {
        Err("unused".into())
    }
    fn preview_png(&self, _: &Template, _: &Record, _: f32) -> Result<Vec<u8>, String> {
        Err("unused".into())
    }
    fn render_page_png(&self, _: &Path, page: usize, _: f32) -> Result<Vec<u8>, String> {
        self.rendered_pages.lock().unwrap().push(page);
        Ok(b"fake png page".to_vec())
    }
    fn page_count(&self, _: &Path) -> Result<usize, String> {
        Ok(3)
    }
    fn test_page(&self, _: &Path, _: f32, _: f32, _: &[String], _: u32) -> Result<(), String> {
        Err("unused".into())
    }
    fn remove_generated_file(&self, _: &Path) {}
}

#[test]
fn remote_job_shows_local_input_and_does_not_claim_a_local_preview() {
    let root = std::env::temp_dir().join(format!("ixprint-history-service-{}", uuid::Uuid::new_v4()));
    let history = Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ));
    let mut app = Arc::try_unwrap(test_support::state()).ok().unwrap();
    app.history = history.clone();
    let entry = HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        time: chrono::Local::now().to_rfc3339(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "remote data".into(),
        preset: Some("label".into()),
        printer: "Remote / Printer".into(),
        spooler_job: None,
        sheets: 1,
        settings: serde_json::json!({}),
        success: true,
        error: None,
        error_code: None,
        duration_ms: 1,
        callback: None,
        remote: Some(RemoteJob {
            remote_id: "remote-id".into(),
            remote_name: "Remote".into(),
            job_id: "remote-job".into(),
        }),
        content: Some(HistoryContentStatus {
            input_kind: Some(HistoryInputKind::Json),
            input_saved: true,
            output: HistoryOutputState::RemoteUnavailable,
        }),
    };
    let input = serde_json::json!({ "data": { "sku": "LOCAL-INPUT" } });
    history.save_input_json(&entry, &input).unwrap();
    history.append(&entry).unwrap();
    let state = Arc::new(app);

    let content = service::content(&state, &entry.id).unwrap();
    assert_eq!(content.input_availability, ContentAvailability::Available);
    assert_eq!(content.input_json, Some(input));
    let preview = service::pdf_page(&state, &entry.id, "output", 1).unwrap();
    assert_eq!(preview.availability, ContentAvailability::RemoteUnavailable);
    assert!(preview.png.is_none());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn legacy_history_entries_report_that_content_was_not_saved() {
    let root = std::env::temp_dir().join(format!("ixprint-history-legacy-{}", uuid::Uuid::new_v4()));
    let history = Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ));
    let mut app = Arc::try_unwrap(test_support::state()).ok().unwrap();
    app.history = history.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let legacy: HistoryEntry = serde_json::from_value(serde_json::json!({
        "id": id,
        "time": chrono::Local::now().to_rfc3339(),
        "source": "http",
        "file_name": "old.pdf",
        "printer": "Printer",
        "sheets": 1,
        "settings": {},
        "success": true,
        "duration_ms": 1
    }))
    .unwrap();
    assert!(legacy.content.is_none());
    history.append(&legacy).unwrap();
    let state = Arc::new(app);

    let content = service::content(&state, &legacy.id).unwrap();
    assert_eq!(content.input_availability, ContentAvailability::NotSaved);
    let preview = service::pdf_page(&state, &legacy.id, "output", 1).unwrap();
    assert_eq!(preview.availability, ContentAvailability::NotSaved);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pdf_page_reader_returns_only_the_requested_page_for_input_and_output() {
    let root = std::env::temp_dir().join(format!("ixprint-history-pages-{}", uuid::Uuid::new_v4()));
    let history = Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ));
    let mut app = Arc::try_unwrap(test_support::state()).ok().unwrap();
    app.history = history.clone();
    let rendered_pages = Arc::new(std::sync::Mutex::new(Vec::new()));
    app.pdf_engine = Arc::new(PreviewPdf {
        rendered_pages: rendered_pages.clone(),
    });
    let id = uuid::Uuid::new_v4().to_string();
    let entry = HistoryEntry {
        id: id.clone(),
        time: chrono::Local::now().to_rfc3339(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "input.pdf".into(),
        preset: None,
        printer: "Printer".into(),
        spooler_job: Some("spool-1".into()),
        sheets: 1,
        settings: serde_json::json!({}),
        success: true,
        error: None,
        error_code: None,
        duration_ms: 1,
        callback: None,
        remote: None,
        content: Some(HistoryContentStatus {
            input_kind: Some(HistoryInputKind::Pdf),
            input_saved: true,
            output: HistoryOutputState::Saved,
        }),
    };
    let source = root.join("source.pdf");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&source, b"%PDF-input").unwrap();
    history.save_input_pdf(&entry, &source).unwrap();
    history.save_output_pdf(&entry, &source).unwrap();
    history.append(&entry).unwrap();
    let state = Arc::new(app);

    let first = service::pdf_page(&state, &id, "input", 1).unwrap();
    let third = service::pdf_page(&state, &id, "output", 3).unwrap();
    assert_eq!(
        (first.availability, first.page_count, first.png),
        (ContentAvailability::Available, Some(3), Some(b"fake png page".to_vec()))
    );
    assert_eq!(
        (third.availability, third.page_count, third.page),
        (ContentAvailability::Available, Some(3), 3)
    );
    assert_eq!(*rendered_pages.lock().unwrap(), vec![0, 2]);
    assert!(service::pdf_page(&state, &id, "arbitrary", 1).is_err());
    assert!(service::pdf_page(&state, &id, "output", 4).is_err());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn changing_retention_prunes_content_and_metadata_together() {
    let root = std::env::temp_dir().join(format!("ixprint-retention-{}", uuid::Uuid::new_v4()));
    let history = Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ));
    let mut app = Arc::try_unwrap(test_support::state()).ok().unwrap();
    app.history = history.clone();
    let mut old = test_entry("2001-01-01T08:00:00+07:00");
    old.content = Some(HistoryContentStatus {
        input_kind: Some(HistoryInputKind::Json),
        input_saved: true,
        output: HistoryOutputState::NotCreated,
    });
    history
        .save_input_json(&old, &serde_json::json!({ "old": true }))
        .unwrap();
    history.append(&old).unwrap();

    let mut recent = test_entry(&chrono::Local::now().to_rfc3339());
    recent.content = old.content.clone();
    history
        .save_input_json(&recent, &serde_json::json!({ "recent": true }))
        .unwrap();
    history.append(&recent).unwrap();
    let old_file = root
        .join("history-content")
        .join("2001-01-01")
        .join(&old.id)
        .join("input.json");
    let recent_file = root
        .join("history-content")
        .join(&recent.time[..10])
        .join(&recent.id)
        .join("input.json");
    let state = Arc::new(app);

    let config = service::set_retention(&state, 30).unwrap();
    assert_eq!(config.history_retention_days, 30);
    assert!(history.find(&old.id).is_none());
    assert!(history.find(&recent.id).is_some());
    assert!(!old_file.exists());
    assert!(recent_file.is_file());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn retention_cleanup_errors_are_diagnostic_and_do_not_fail_the_setting_change() {
    let root = std::env::temp_dir().join(format!("ixprint-retention-error-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let history_path = root.join("history-is-a-file");
    std::fs::write(&history_path, b"not a directory").unwrap();
    let history = Arc::new(History::with_content_dir(history_path, root.join("history-content")));
    let mut app = Arc::try_unwrap(test_support::state()).ok().unwrap();
    app.history = history;
    let state = Arc::new(app);

    let config = service::set_retention(&state, 7).unwrap();

    assert_eq!(config.history_retention_days, 7);
    assert!(state
        .diagnostics
        .read()
        .unwrap()
        .iter()
        .any(|entry| entry.source == "history-retention"));
    let _ = std::fs::remove_dir_all(root);
}

fn test_entry(time: &str) -> HistoryEntry {
    HistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        time: time.into(),
        source: "http".into(),
        client: None,
        token: None,
        file_name: "history-test.pdf".into(),
        preset: None,
        printer: "Printer".into(),
        spooler_job: None,
        sheets: 1,
        settings: serde_json::json!({}),
        success: true,
        error: None,
        error_code: None,
        duration_ms: 1,
        callback: None,
        remote: None,
        content: None,
    }
}
