use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::domain::settings::{Align, Margins, Orientation, Paper, PrintSettings, Scale};
use crate::domain::template::Record;

pub const MAX_TEST_PAGES: u32 = 50;
pub const UNKNOWN_OUTCOME_MESSAGE: &str =
    "The print outcome is not known. Check History and the printer queue before creating a new job.";

#[derive(Debug)]
pub enum JobError {
    /// Invalid request (unknown preset, bad settings, no printer): HTTP 400.
    BadRequest(String),
    /// The document or printer failed: HTTP 500.
    Failed(String),
    /// Submission began but its result cannot be determined safely.
    OutcomeUnknown { job_id: String, message: String },
}

impl JobError {
    pub fn message(&self) -> &str {
        match self {
            JobError::BadRequest(m) | JobError::Failed(m) => m,
            JobError::OutcomeUnknown { message, .. } => message,
        }
    }
}

pub enum Document {
    File(PathBuf),
    TestPage { pages: u32 },
    Data { records: Vec<Record>, header: Record },
}

pub struct JobRequest {
    /// Preassigned by the HTTP idempotency reservation; desktop jobs generate their own.
    pub job_id: Option<String>,
    pub document: Document,
    /// Parsed JSON body, kept separately from normalized print records for History.
    pub history_input: Option<serde_json::Value>,
    pub file_name: String,
    pub settings: PrintSettings,
    pub preset: Option<String>,
    pub source: &'static str,
    pub client: Option<String>,
    pub token: Option<String>,
    pub hop: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobResult {
    pub job_id: String,
    pub printer: String,
    pub preset: Option<String>,
    pub sheets: u32,
    pub spooler_job: Option<String>,
}

/// Paper for a design: its page, or a roll of its width when a list grows with its rows.
pub fn design_paper(template: &crate::domain::template::Template) -> Paper {
    match template.layout {
        crate::domain::template::Layout::List { roll: true, .. } => Paper::Roll {
            width_mm: template.width_mm,
        },
        _ => Paper::Custom {
            width_mm: template.width_mm,
            height_mm: template.height_mm,
        },
    }
}

/// Applies the print behavior used by generated design pages.
pub fn design_settings(settings: &mut PrintSettings, template: &crate::domain::template::Template, test: bool) {
    settings.paper = Some(design_paper(template));
    settings.scale = Some(Scale::None);
    settings.margins = Some(Margins::default());
    settings.align = Some(Align::Center);
    settings.orientation = Some(Orientation::Portrait);
    settings.pages = None;
    if test {
        settings.copies = Some(1);
    }
}
