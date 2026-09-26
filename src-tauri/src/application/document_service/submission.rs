use super::{stage_pdf, IncomingPrintResult, IncomingTargetCheck};
use crate::application::ports::ReadyPrintRequest;
use crate::application::state::AppState;
use crate::domain::history::{HistoryContentStatus, HistoryEntry, HistoryInputKind, HistoryOutputState, RemoteJob};
use crate::domain::incoming_job::{self, IncomingJob, IncomingSource, IncomingState};
use crate::domain::layout::Sheet;
use crate::domain::printer::DeviceOptions;
use crate::domain::remote::{RemoteError, Target};
use crate::domain::settings::Paper;
use std::path::Path;
use std::time::Instant;

pub fn check_target(state: &AppState, job_id: &str, printer_name: &str) -> Result<IncomingTargetCheck, String> {
    let job = incoming_job(state, job_id)?;
    if job.state != IncomingState::AwaitingDestination {
        return Err("This document is no longer awaiting a destination".into());
    }
    let printer_name = printer_name.trim();
    if printer_name.is_empty() {
        return Err("Choose a printer destination".into());
    }
    let target = crate::application::remote_service::resolve(state, printer_name);
    if target == Target::Unknown {
        return Err(format!("Printer '{}' is no longer available", printer_name));
    }
    let (remote, status, printer_message) = match &target {
        Target::Local(name) => {
            let printer = state
                .printer_gateway
                .list_printers()
                .into_iter()
                .find(|printer| printer.name == *name)
                .ok_or_else(|| format!("Printer '{}' is not available", printer_name))?;
            (None, printer.status, printer.message)
        }
        Target::Remote { remote, .. } => {
            let (printers, error) = crate::application::remote_service::printers(state, remote);
            let printer = printers.into_iter().find(|printer| printer.name == printer_name);
            match printer {
                Some(printer) => (Some(remote.name.clone()), printer.status, printer.message),
                None => return Err(error.unwrap_or_else(|| format!("Printer '{}' is not available", printer_name))),
            }
        }
        Target::Unknown => unreachable!(),
    };
    let status = status.to_ascii_lowercase();
    if status.contains("offline") || status.contains("disabled") || status.contains("unavailable") {
        return Ok(IncomingTargetCheck {
            compatible: false,
            message: Some(printer_message.unwrap_or_else(|| "Printer is offline".into())),
            supported_media: Vec::new(),
            remote,
        });
    }
    let capabilities =
        crate::application::printer_service::capabilities(state, printer_name, false).map_err(|error| match error {
            crate::application::printer_service::PrinterLookupError::NotFound(message) => message,
            crate::application::printer_service::PrinterLookupError::Remote(error) => error.to_string(),
        })?;
    let mut supported_media = capabilities
        .papers
        .iter()
        .filter_map(|paper| standard_media(paper.width_mm, paper.height_mm))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(default) = &capabilities.default_paper {
        if let Some(name) = standard_media(default.width_mm, default.height_mm) {
            supported_media.push(name.to_string());
        }
    }
    supported_media.sort();
    supported_media.dedup();
    let compatibility = incoming_job::check_compatibility(&job.pages, &capabilities);
    Ok(IncomingTargetCheck {
        compatible: compatibility.is_ok(),
        message: compatibility.err(),
        supported_media,
        remote,
    })
}

pub fn submit(state: &AppState, job_id: &str, printer_name: &str) -> Result<IncomingPrintResult, String> {
    let check = check_target(state, job_id, printer_name)?;
    if !check.compatible {
        return Err(check
            .message
            .unwrap_or_else(|| "Printer cannot accept this document".into()));
    }
    if !state.incoming_documents.claim_for_submission(job_id, printer_name)? {
        return Err("This document is already being submitted or has already been handled".into());
    }
    let mut job = incoming_job(state, job_id)?;
    job.state = IncomingState::Submitting;
    job.printer = Some(printer_name.to_string());
    let started = Instant::now();
    let outcome = submit_to_printer(state, &job, printer_name);
    match outcome {
        Ok(result) => {
            job.state = IncomingState::Queued;
            job.submission_started_at = None;
            job.spooler_job = result.spooler_job.clone();
            job.remote_job_id = result.remote_job_id.clone();
            job.remote_name = result.remote.clone();
            job.message = Some(if result.remote.is_some() {
                "Forwarded to the remote printer queue; physical completion is not confirmed".into()
            } else {
                "Sent to the printer queue; physical completion is not confirmed".into()
            });
            state.incoming_documents.save(&job)?;
            let mut history = make_history(
                state,
                &job,
                printer_name,
                result.sheets,
                true,
                None,
                started.elapsed().as_millis() as u64,
            );
            history.spooler_job = result.spooler_job.clone();
            if let (Some(remote), Some(remote_job_id)) = (result.remote.clone(), result.remote_job_id.clone()) {
                if let Some(remote_config) = state.remotes.list().iter().find(|item| item.name == remote) {
                    history.remote = Some(RemoteJob {
                        remote_id: remote_config.id.clone(),
                        remote_name: remote,
                        job_id: remote_job_id,
                    });
                }
            }
            append_history(state, history);
            Ok(IncomingPrintResult {
                job_id: result.remote_job_id.unwrap_or_else(|| job.id.clone()),
                printer: printer_name.to_string(),
                sheets: result.sheets,
                copies: job.copies,
                spooler_job: result.spooler_job,
                remote: result.remote,
            })
        }
        Err(error) => {
            job.state = IncomingState::Failed;
            job.submission_started_at = None;
            job.message = Some(error.clone());
            let _ = state.incoming_documents.save(&job);
            append_history(
                state,
                make_history(
                    state,
                    &job,
                    printer_name,
                    0,
                    false,
                    Some(error.clone()),
                    started.elapsed().as_millis() as u64,
                ),
            );
            Err(error)
        }
    }
}

/// Used by an authenticated remote receiver after the source computer's user confirms the destination.
pub fn receive_ready_pdf(
    state: &AppState,
    path: &Path,
    file_name: &str,
    source: IncomingSource,
    source_job_id: &str,
    copies: u32,
    printer: &str,
) -> Result<IncomingPrintResult, String> {
    if !state
        .printer_gateway
        .list_printers()
        .iter()
        .any(|item| item.name == printer)
    {
        return Err("Forwarded destination must be a printer on this computer".into());
    }
    let mut staged = stage_pdf(
        state,
        &path.to_string_lossy(),
        source,
        Some(source_job_id.to_string()),
        copies,
    )?;
    if staged.state == IncomingState::Queued {
        return Ok(IncomingPrintResult {
            job_id: staged.id,
            printer: staged.printer.unwrap_or_else(|| printer.to_string()),
            sheets: (staged.pages.len() as u32).saturating_mul(staged.copies),
            copies: staged.copies,
            spooler_job: staged.spooler_job,
            remote: None,
        });
    }
    if staged.state != IncomingState::AwaitingDestination {
        return Err("A matching forwarded job is already being processed; check History before retrying".into());
    }
    staged.document_name = sanitize_filename::sanitize(file_name);
    if staged.document_name.is_empty() {
        staged.document_name = "document.pdf".into();
    }
    state.incoming_documents.save(&staged)?;
    submit(state, &staged.id, printer)
}

struct SubmitOutcome {
    sheets: u32,
    spooler_job: Option<String>,
    remote_job_id: Option<String>,
    remote: Option<String>,
}

fn submit_to_printer(state: &AppState, job: &IncomingJob, printer_name: &str) -> Result<SubmitOutcome, String> {
    let path = state.incoming_documents.document_path(&job.id)?;
    let target = crate::application::remote_service::resolve(state, printer_name);
    let page_sheets = job
        .pages
        .iter()
        .map(|page| Sheet {
            width_pt: page.width_pt,
            height_pt: page.height_pt,
        })
        .collect::<Vec<_>>();
    match target {
        Target::Local(printer) => {
            let capabilities = state.printer_gateway.capabilities(&printer)?;
            incoming_job::check_compatibility(&job.pages, &capabilities)?;
            let options = DeviceOptions {
                job_name: &job.document_name,
                copies: job.copies,
                collate: true,
                paper: &Paper::PrinterDefault,
                duplex: None,
                color: None,
                tray: None,
                dpi: None,
            };
            let spooler_job = state.printer_gateway.print(&printer, &path, &page_sheets, &options)?;
            Ok(SubmitOutcome {
                sheets: (page_sheets.len() as u32).saturating_mul(job.copies),
                spooler_job,
                remote_job_id: None,
                remote: None,
            })
        }
        Target::Remote { remote, printer } => {
            let capabilities = state
                .remote_gateway
                .capabilities(&remote, &printer)
                .map_err(|error| error.to_string())?;
            incoming_job::check_compatibility(&job.pages, &capabilities)?;
            let result = state
                .remote_gateway
                .print_ready_pdf(ReadyPrintRequest {
                    remote: &remote,
                    path: &path,
                    file_name: &job.document_name,
                    source: job.source,
                    source_job_id: &job.id,
                    copies: job.copies,
                    printer: &printer,
                })
            .map_err(|error| match error {
                RemoteError::BadRequest(message) => message,
                RemoteError::Failed(message) | RemoteError::OutcomeUnknown(message) => format!(
                    "The remote submission result could not be confirmed. Check {} History and printer queue before reprinting. {}",
                    remote.name, message
                ),
            })?;
            Ok(SubmitOutcome {
                sheets: result.sheets,
                spooler_job: result.spooler_job,
                remote_job_id: Some(result.job_id),
                remote: Some(remote.name),
            })
        }
        Target::Unknown => Err(format!("Printer '{}' is no longer available", printer_name)),
    }
}

fn make_history(
    state: &AppState,
    job: &IncomingJob,
    printer: &str,
    sheets: u32,
    success: bool,
    error: Option<String>,
    duration_ms: u64,
) -> HistoryEntry {
    let mut entry = HistoryEntry {
        id: job.id.clone(),
        time: job.received_at.clone(),
        source: format!("incoming_{}", job.source.as_str()),
        client: None,
        token: None,
        file_name: job.document_name.clone(),
        preset: None,
        printer: printer.to_string(),
        spooler_job: None,
        sheets,
        settings: serde_json::Value::Null,
        success,
        error: error.clone(),
        error_code: None,
        duration_ms,
        callback: None,
        remote: None,
        content: Some(HistoryContentStatus {
            input_kind: Some(HistoryInputKind::Pdf),
            input_saved: false,
            output: HistoryOutputState::NotCreated,
        }),
    };
    if let Ok(path) = state.incoming_documents.document_path(&job.id) {
        let saved = state.history.save_input_pdf(&entry, &path).is_ok();
        entry.content.as_mut().unwrap().input_saved = saved;
    }
    entry.settings = serde_json::json!({ "copies": job.copies, "pages": job.pages });
    entry.sheets = sheets;
    entry.success = success;
    entry.error = error;
    entry.duration_ms = duration_ms;
    entry
}

fn append_history(state: &AppState, entry: HistoryEntry) {
    if let Err(error) = state.history.append(&entry) {
        eprintln!("Failed to write incoming print history: {}", error);
    }
}

pub(super) fn record_cancelled(state: &AppState, job: &IncomingJob) {
    let mut entry = make_history(
        state,
        job,
        job.printer.as_deref().unwrap_or("—"),
        0,
        false,
        Some("Cancelled before printing".into()),
        0,
    );
    entry.source = format!("incoming_cancelled_{}", job.source.as_str());
    append_history(state, entry);
}

fn standard_media(width_mm: f32, height_mm: f32) -> Option<&'static str> {
    let close = |a: f32, b: f32| (a - b).abs() <= 1.0;
    [("A4", 210.0, 297.0), ("Letter", 215.9, 279.4)]
        .into_iter()
        .find(|(_, width, height)| {
            close(width_mm, *width) && close(height_mm, *height) || close(width_mm, *height) && close(height_mm, *width)
        })
        .map(|(name, _, _)| name)
}

fn incoming_job(state: &AppState, job_id: &str) -> Result<IncomingJob, String> {
    state
        .incoming_documents
        .get(job_id)?
        .ok_or_else(|| "Incoming document was not found".into())
}
