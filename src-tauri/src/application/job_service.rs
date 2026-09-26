//! Print job use case shared by the HTTP bridge and desktop window.

use std::path::Path;
use std::time::Instant;

use crate::application::ports::JobMarker;
use crate::application::ports::OperationState;
use crate::application::state::AppState;
use crate::domain::history::{HistoryContentStatus, HistoryEntry, HistoryInputKind, HistoryOutputState};
use crate::domain::job::{design_paper, Document, JobError, JobRequest, JobResult, MAX_TEST_PAGES};
use crate::domain::preset::Preset;
use crate::domain::printer::DeviceOptions;
use crate::domain::settings::{Align, JobSettings, Margins, Orientation, Paper, Scale};

pub fn run(state: &AppState, req: JobRequest) -> Result<JobResult, JobError> {
    run_with_preset(state, req, None)
}

/// Desktop-only test path. The published preset remains untouched.
pub fn run_with_preset(
    state: &AppState,
    req: JobRequest,
    preset_override: Option<Preset>,
) -> Result<JobResult, JobError> {
    let started = Instant::now();
    let job_id = req.job_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let config = state.config.get();
    let preset_name = req.preset.clone().or(config.default_preset.clone());
    state.history.content_job_started(&job_id);

    let mut record = HistoryEntry {
        id: job_id.clone(),
        time: state.clock.now_rfc3339(),
        source: req.source.to_string(),
        client: req.client.clone(),
        token: req.token.clone(),
        file_name: req.file_name.clone(),
        preset: preset_name.clone(),
        printer: String::new(),
        spooler_job: None,
        sheets: 0,
        settings: serde_json::to_value(&req.settings).unwrap_or_default(),
        success: false,
        error: None,
        error_code: None,
        duration_ms: 0,
        callback: None,
        remote: None,
        content: Some(HistoryContentStatus {
            input_kind: None,
            input_saved: false,
            output: HistoryOutputState::NotCreated,
        }),
    };

    match (&req.document, req.history_input.as_ref()) {
        (Document::File(path), _) => {
            record.content.as_mut().unwrap().input_kind = Some(HistoryInputKind::Pdf);
            let saved = state.history.save_input_pdf(&record, path).is_ok();
            record.content.as_mut().unwrap().input_saved = saved;
        }
        (_, Some(value)) => {
            record.content.as_mut().unwrap().input_kind = Some(HistoryInputKind::Json);
            let saved = state.history.save_input_json(&record, value).is_ok();
            record.content.as_mut().unwrap().input_saved = saved;
        }
        _ => {}
    }

    let mut result = (|| -> Result<JobResult, JobError> {
        let preset = if let Some(draft) = &preset_override {
            draft.clone()
        } else {
            match &preset_name {
                Some(name) => config
                    .presets
                    .get(name)
                    .cloned()
                    .ok_or_else(|| JobError::BadRequest(format!("Preset '{}' was not found", name)))?,
                None => Default::default(),
            }
        };
        let merged = preset.settings.overlay(&req.settings);
        merged.validate().map_err(JobError::BadRequest)?;

        if let Document::TestPage { .. } = &req.document {
            if let Some(design) = &preset.design {
                let sample = serde_json::json!({ "data": [design.sample.clone()], "header": design.sample.clone() });
                record.content.as_mut().unwrap().input_kind = Some(HistoryInputKind::Sample);
                let saved = state.history.save_input_json(&record, &sample).is_ok();
                record.content.as_mut().unwrap().input_saved = saved;
            }
        }

        let printer_name = merged
            .printer
            .clone()
            .or_else(|| state.default_printer())
            .ok_or_else(|| {
                JobError::BadRequest("No printer: choose one in the request, preset or default printer".into())
            })?;
        record.printer = printer_name.clone();
        let local_names: Vec<String> = state
            .printer_gateway
            .list_printers()
            .into_iter()
            .map(|printer| printer.name)
            .collect();
        match crate::domain::remote::resolve(&local_names, &state.remotes.list(), &printer_name) {
            crate::domain::remote::Target::Local(_) => {}
            crate::domain::remote::Target::Unknown => {
                return Err(JobError::BadRequest(format!(
                    "Printer '{}' was not found on this computer",
                    printer_name
                )));
            }
            crate::domain::remote::Target::Remote { remote, printer } => {
                let forwarded = crate::application::remote_service::forward(
                    state,
                    crate::application::remote_service::ForwardRequest {
                        job_id: &job_id,
                        preset_name: preset_name.as_deref(),
                        preset: &preset,
                        merged: &merged,
                        req: &req,
                        remote: &remote,
                        remote_printer: &printer,
                        record: &mut record,
                    },
                );
                if forwarded.is_ok() {
                    record.content.as_mut().unwrap().output = HistoryOutputState::RemoteUnavailable;
                }
                return forwarded;
            }
        }

        let mut job = JobSettings::resolve(&merged, printer_name.clone());
        let _active_job: Option<Box<dyn JobMarker>> =
            state.diagnostics.begin_job(&job_id, req.source, &printer_name).ok();

        let default_paper = if job.paper == Paper::PrinterDefault {
            state.printer_gateway.default_paper_mm(&printer_name)
        } else {
            (210.0, 297.0)
        };

        let generated = state.jobs_dir.join(format!("{}-test.pdf", job_id));
        let source = match &req.document {
            Document::File(path) => path.clone(),
            Document::Data { records, header } => {
                let template = preset.design.as_ref().ok_or_else(|| {
                    JobError::BadRequest(match &preset_name {
                        Some(name) => format!("Preset '{}' has no design; send a PDF instead of JSON data", name),
                        None => "Specify a preset with a design in the \"preset\" field".into(),
                    })
                })?;
                if records.len() > crate::domain::template::MAX_RECORDS {
                    return Err(JobError::BadRequest(format!(
                        "at most {} records per job",
                        crate::domain::template::MAX_RECORDS
                    )));
                }
                template.preflight(records, header).map_err(JobError::BadRequest)?;
                state
                    .pdf_engine
                    .render_template(&generated, template, records, header)
                    .map_err(JobError::BadRequest)?;
                job.paper = design_paper(template);
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Portrait;
                job.pages = None;
                generated.clone()
            }
            Document::TestPage { pages } if preset.design.is_some() => {
                let Some(design) = preset.design.as_ref() else {
                    return Err(JobError::BadRequest("Preset has no design".into()));
                };
                let records = vec![design.sample.clone(); (*pages).clamp(1, MAX_TEST_PAGES) as usize];
                state
                    .pdf_engine
                    .render_template(&generated, design, &records, &design.sample)
                    .map_err(JobError::BadRequest)?;
                job.paper = design_paper(design);
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Portrait;
                job.pages = None;
                job.copies = 1;
                generated.clone()
            }
            Document::TestPage { pages } => {
                let (width, height) = test_sheet_mm(&job, default_paper);
                let info = test_page_info(state, &job, preset_name.as_deref(), width, height);
                state
                    .pdf_engine
                    .test_page(&generated, width, height, &info, *pages)
                    .map_err(JobError::Failed)?;
                job.scale = Scale::None;
                job.margins = Margins::default();
                job.align = Align::Center;
                job.orientation = Orientation::Auto;
                job.pages = None;
                job.copies = 1;
                generated.clone()
            }
        };
        check_paper_fits(state, &job).map_err(JobError::BadRequest)?;
        record.settings = serde_json::to_value(&job).unwrap_or_default();

        let ready = state.jobs_dir.join(format!("{}-ready.pdf", job_id));
        let outcome = print_ready(state, &source, &ready, &job, default_paper, &req.file_name, &mut record);
        state.pdf_engine.remove_generated_file(&ready);
        state.pdf_engine.remove_generated_file(&generated);
        let (sheets, spooler_job) = outcome.map_err(JobError::Failed)?;

        record.sheets = sheets;
        record.spooler_job = spooler_job.clone();
        Ok(JobResult {
            job_id: job_id.clone(),
            printer: printer_name,
            preset: preset_name.clone(),
            sheets,
            spooler_job,
        })
    })();

    // Generated pages may hold patient data: remove them even when the job failed early.
    state
        .pdf_engine
        .remove_generated_file(&state.jobs_dir.join(format!("{}-test.pdf", job_id)));
    record.success = result.is_ok();
    record.error = result.as_ref().err().map(|error| error.message().to_string());
    record.error_code = result
        .as_ref()
        .err()
        .and_then(|error| matches!(error, JobError::OutcomeUnknown { .. }).then(|| "outcome_unknown".to_string()));
    record.duration_ms = started.elapsed().as_millis() as u64;
    match state.history.append(&record) {
        Ok(()) => {
            let state_update = if matches!(&result, Err(JobError::OutcomeUnknown { .. })) {
                state.idempotency.mark_unknown(&job_id)
            } else {
                state.idempotency.mark_finished(&job_id)
            };
            if let Err(error) = state_update {
                let _ = state.diagnostics.record(
                    "error",
                    "idempotency",
                    "Print result was saved to History but its operation state could not be finalized",
                    Some(&error),
                );
                // A later retry can recover `finished` from History. Keep the
                // active marker clear so this process can perform that replay.
                state.idempotency.release_reserved(&job_id);
            }
        }
        Err(error) => {
            let _ = state.diagnostics.record(
                "error",
                req.source,
                "Could not save print result to History",
                Some(&error),
            );
            let state_before_history_failure = state.idempotency.operation_state(&job_id).ok().flatten();
            if matches!(
                state_before_history_failure,
                Some(OperationState::Submitting | OperationState::Unknown)
            ) || result.is_ok()
                || matches!(&result, Err(JobError::OutcomeUnknown { .. }))
            {
                let message =
                    "Print submission started, but its result could not be recorded. Check History and the printer queue before creating a new job.";
                if let Err(persist_error) = state.idempotency.mark_unknown(&job_id) {
                    let _ = state.diagnostics.record(
                        "error",
                        "idempotency",
                        "Print outcome is unknown and its state could not be persisted",
                        Some(&persist_error),
                    );
                    state.idempotency.release_reserved(&job_id);
                }
                result = Err(JobError::OutcomeUnknown {
                    job_id: job_id.clone(),
                    message: message.into(),
                });
            } else {
                state.idempotency.release_reserved(&job_id);
            }
        }
    }
    state.history.content_job_finished(&job_id);
    if let Err(error) = &result {
        let _ = state.diagnostics.record("error", req.source, error.message(), None);
    }
    result
}

fn check_paper_fits(state: &AppState, job: &JobSettings) -> Result<(), String> {
    let (width, height) = match job.paper {
        Paper::Custom { width_mm, height_mm } => (width_mm, Some(height_mm)),
        Paper::Roll { width_mm } => (width_mm, None),
        _ => return Ok(()),
    };
    let Ok(capabilities) = state.printer_gateway.capabilities(&job.printer) else {
        return Ok(());
    };
    let too_wide = capabilities.max_width_mm.is_some_and(|max| width > max + 0.5);
    let too_long = matches!((height, capabilities.max_height_mm), (Some(height), Some(max)) if height > max + 0.5);
    if too_wide || too_long {
        let max = |value: Option<f32>| value.map(|v| format!("{:.0}", v)).unwrap_or_else(|| "?".into());
        return Err(format!(
            "Paper size {} is larger than printer '{}' maximum ({} × {} mm). Check the units (mm).",
            job.paper,
            job.printer,
            max(capabilities.max_width_mm),
            max(capabilities.max_height_mm)
        ));
    }
    Ok(())
}

pub(crate) fn test_sheet_mm(job: &JobSettings, default_paper: (f32, f32)) -> (f32, f32) {
    if let Paper::Roll { width_mm } = job.paper {
        return (width_mm, width_mm.max(80.0));
    }
    if let Paper::Custom { width_mm, height_mm } = job.paper {
        return (width_mm, height_mm);
    }
    let (width, height) = job.paper.size_mm().unwrap_or(default_paper);
    let (short, long) = (width.min(height), width.max(height));
    match job.orientation {
        Orientation::Landscape => (long, short),
        Orientation::Portrait | Orientation::Auto => (short, long),
    }
}

pub(crate) fn test_page_info(
    state: &AppState,
    job: &JobSettings,
    preset: Option<&str>,
    width: f32,
    height: f32,
) -> Vec<String> {
    let paper = match &job.paper {
        Paper::Roll { .. } => format!("{:.0} mm roll (example length {:.0} mm)", width, height),
        Paper::PrinterDefault => format!("{:.0} × {:.0} mm (default printer)", width, height),
        paper => format!("{} = {:.0} × {:.0} mm", paper, width, height),
    };
    let time = chrono::DateTime::parse_from_rfc3339(&state.clock.now_rfc3339())
        .map(|time| time.format("%d-%m-%Y %H:%M:%S").to_string())
        .unwrap_or_default();
    vec![
        format!("Paper: {}", paper),
        format!("Printer: {}", job.printer),
        format!("Preset: {}", preset.unwrap_or("—")),
        format!("Offset X: {} mm · Offset Y: {} mm", job.offset_x, job.offset_y),
        time,
    ]
}

fn print_ready(
    state: &AppState,
    source: &Path,
    ready: &Path,
    job: &JobSettings,
    default_paper: (f32, f32),
    file_name: &str,
    record: &mut HistoryEntry,
) -> Result<(u32, Option<String>), String> {
    let layout = state.pdf_engine.build_layout(source, ready, job, default_paper)?;
    let saved = state.history.save_output_pdf(record, ready).is_ok();
    record.content.as_mut().unwrap().output = if saved {
        HistoryOutputState::Saved
    } else {
        HistoryOutputState::SaveFailed
    };
    let options = DeviceOptions {
        job_name: file_name,
        copies: job.copies,
        collate: job.collate,
        paper: &job.paper,
        duplex: job.duplex,
        color: job.color,
        tray: job.tray.as_deref(),
        dpi: job.dpi,
    };
    state
        .idempotency
        .mark_submitting(&record.id)
        .map_err(|error| format!("Could not persist print operation state: {}", error))?;
    let spooler_job = state
        .printer_gateway
        .print(&job.printer, ready, &layout.sheets, &options)?;
    Ok((layout.sheets.len() as u32 * job.copies, spooler_job))
}
