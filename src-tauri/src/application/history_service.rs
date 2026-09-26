use crate::application::state::AppState;
use crate::domain::history::{
    HistoryContentStatus, HistoryDay, HistoryEntry, HistoryInputKind, HistoryOutputState, HistoryPage, HistoryPdfKind,
};
use crate::domain::settings_config::Config;
use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentAvailability {
    NotSaved,
    NoRequestBody,
    NotPdf,
    SaveFailed,
    NotCreated,
    RemoteUnavailable,
    Unavailable,
    Available,
}

#[derive(Serialize, Debug)]
pub struct ContentDetail {
    pub content: Option<HistoryContentStatus>,
    pub input_availability: ContentAvailability,
    pub input_json: Option<serde_json::Value>,
}

pub struct PdfPage {
    pub availability: ContentAvailability,
    pub page: usize,
    pub page_count: Option<usize>,
    pub png: Option<Vec<u8>>,
}

pub fn set_retention(state: &AppState, days: u32) -> Result<Config, String> {
    let config = state.config.update(&mut |config| {
        config.history_retention_days = days;
        Ok(())
    })?;
    prune_retained_data(state, days);
    Ok(config)
}

/// Runs the configured retention cleanup without taking down the print bridge if storage cleanup fails.
pub fn prune_current_retention(state: &AppState) {
    prune_retained_data(state, state.config.get().history_retention_days);
}

fn prune_retained_data(state: &AppState, days: u32) {
    if let Err(error) = state.history.prune(days) {
        let _ = state.diagnostics.record(
            "error",
            "history-retention",
            "History retention cleanup failed",
            Some(&error),
        );
    }
    if let Err(error) = crate::application::document_service::prune(state.incoming_documents.as_ref(), days) {
        let _ = state.diagnostics.record(
            "error",
            "incoming-retention",
            "Incoming document retention cleanup failed",
            Some(&error),
        );
    }
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

pub fn content(state: &AppState, job_id: &str) -> Result<ContentDetail, String> {
    let entry = history_entry(state, job_id)?;
    let Some(content) = entry.content.clone() else {
        return Ok(ContentDetail {
            content: None,
            input_availability: ContentAvailability::NotSaved,
            input_json: None,
        });
    };
    let (input_availability, input_json) = match content.input_kind {
        None => (ContentAvailability::NoRequestBody, None),
        Some(HistoryInputKind::Pdf) if !content.input_saved => (ContentAvailability::SaveFailed, None),
        Some(HistoryInputKind::Pdf) => match state.history.pdf_path(&entry, HistoryPdfKind::Input) {
            Ok(Some(_)) => (ContentAvailability::Available, None),
            Ok(None) => (ContentAvailability::Unavailable, None),
            Err(_) => return Err("Could not read saved History content".into()),
        },
        Some(HistoryInputKind::Json | HistoryInputKind::Sample) if !content.input_saved => {
            (ContentAvailability::SaveFailed, None)
        }
        Some(HistoryInputKind::Json | HistoryInputKind::Sample) => match state.history.read_input_json(&entry) {
            Ok(Some(value)) => (ContentAvailability::Available, Some(value)),
            Ok(None) => (ContentAvailability::Unavailable, None),
            Err(_) => return Err("Could not read saved History content".into()),
        },
    };
    Ok(ContentDetail {
        content: Some(content),
        input_availability,
        input_json,
    })
}

pub fn pdf_page(state: &AppState, job_id: &str, document: &str, page: usize) -> Result<PdfPage, String> {
    let kind = match document {
        "input" => HistoryPdfKind::Input,
        "output" => HistoryPdfKind::Output,
        _ => return Err("PDF document must be 'input' or 'output'".into()),
    };
    if page == 0 {
        return Err("PDF page numbers start at 1".into());
    }
    let entry = history_entry(state, job_id)?;
    let availability = pdf_availability(&entry, kind);
    if availability != ContentAvailability::Available {
        return Ok(PdfPage {
            availability,
            page,
            page_count: None,
            png: None,
        });
    }
    let path = state
        .history
        .pdf_path(&entry, kind)
        .map_err(|_| "Could not read saved PDF content")?;
    let Some(path) = path else {
        return Ok(PdfPage {
            availability: ContentAvailability::Unavailable,
            page,
            page_count: None,
            png: None,
        });
    };
    let page_count = state
        .pdf_engine
        .page_count(&path)
        .map_err(|_| "Could not read saved PDF content")?;
    if page_count == 0 {
        return Err("Saved PDF contains no pages".into());
    }
    if page > page_count {
        return Err("Requested page is outside the PDF".into());
    }
    let png = state
        .pdf_engine
        .render_page_png(&path, page - 1, 3.0)
        .map_err(|_| "Could not read saved PDF content")?;
    Ok(PdfPage {
        availability: ContentAvailability::Available,
        page,
        page_count: Some(page_count),
        png: Some(png),
    })
}

fn history_entry(state: &AppState, job_id: &str) -> Result<HistoryEntry, String> {
    state
        .history
        .find(job_id)
        .ok_or_else(|| "History job was not found".into())
}

fn pdf_availability(entry: &HistoryEntry, kind: HistoryPdfKind) -> ContentAvailability {
    let Some(content) = &entry.content else {
        return ContentAvailability::NotSaved;
    };
    match kind {
        HistoryPdfKind::Input => match content.input_kind {
            None => ContentAvailability::NoRequestBody,
            Some(HistoryInputKind::Json | HistoryInputKind::Sample) => ContentAvailability::NotPdf,
            Some(HistoryInputKind::Pdf) if !content.input_saved => ContentAvailability::SaveFailed,
            Some(HistoryInputKind::Pdf) => ContentAvailability::Available,
        },
        HistoryPdfKind::Output => match content.output {
            HistoryOutputState::NotCreated => ContentAvailability::NotCreated,
            HistoryOutputState::Saved => ContentAvailability::Available,
            HistoryOutputState::SaveFailed => ContentAvailability::SaveFailed,
            HistoryOutputState::RemoteUnavailable => ContentAvailability::RemoteUnavailable,
        },
    }
}
