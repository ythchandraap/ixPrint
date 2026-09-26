use serde::Serialize;

use crate::application::state::AppState;

/// Cancel only a local spooler job previously submitted by this installation.
/// The queue lookup prevents acting on a reused or already-finished spooler ID.
pub fn cancel_own_job(state: &AppState, job_id: &str) -> Result<(), String> {
    let entry = state
        .history
        .find(job_id)
        .ok_or("Job was not found in recent history")?;
    if !status_is_recent(&entry.time) {
        return Err("Only recent jobs can be cancelled".into());
    }
    if !entry.success || entry.remote.is_some() {
        return Err("Only a local submitted job can be cancelled here".into());
    }
    let spooler = entry
        .spooler_job
        .ok_or("The printer did not provide a spooler job ID")?;
    let queue = state.printer_gateway.queue(&entry.printer)?;
    let job = queue
        .jobs
        .iter()
        .find(|job| job.id == spooler)
        .ok_or("Job is no longer in the queue")?;
    if !matches!(
        job.state,
        crate::domain::printer::JobState::Queued
            | crate::domain::printer::JobState::Paused
            | crate::domain::printer::JobState::Error
    ) {
        return Err("Job has already started or finished printing".into());
    }
    state.printer_gateway.cancel(&entry.printer, &spooler)
}
use crate::domain::printer::{Capabilities, Printer, PrinterQueue};
use crate::domain::printer_admin::{Device, Driver, NewPrinter, PrinterDetails, PrinterEdit};
use crate::domain::remote::{RemoteError, Target, SEPARATOR};
use crate::domain::settings_config::Config;

#[derive(Serialize)]
pub struct PrinterView {
    #[serde(flatten)]
    pub printer: Printer,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

pub enum PrinterLookupError {
    NotFound(String),
    Remote(RemoteError),
}

pub fn printer_views(state: &AppState, include_remote: bool) -> Vec<PrinterView> {
    let default = state.default_printer();
    let mut views: Vec<PrinterView> = state
        .printer_gateway
        .list_printers()
        .into_iter()
        .map(|printer| PrinterView {
            is_default: default.as_deref() == Some(printer.name.as_str()),
            printer,
            remote: None,
        })
        .collect();
    if include_remote {
        let remotes = state.remotes.list();
        let remote_views = std::thread::scope(|scope| {
            remotes
                .iter()
                .map(|remote| scope.spawn(|| crate::application::remote_service::printers(state, remote).0))
                .collect::<Vec<_>>()
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .map(|printer| PrinterView {
                    is_default: default.as_deref() == Some(printer.name.as_str()),
                    remote: remotes.iter().find_map(|remote| {
                        printer
                            .name
                            .strip_prefix(&format!("{}{}", remote.name, SEPARATOR))
                            .map(|_| remote.name.clone())
                    }),
                    printer,
                })
                .collect::<Vec<_>>()
        });
        views.extend(remote_views);
    }
    views
}

pub fn capabilities(state: &AppState, name: &str, hop: bool) -> Result<Capabilities, PrinterLookupError> {
    match crate::application::remote_service::resolve(state, name) {
        Target::Local(printer) => state
            .printer_gateway
            .capabilities(&printer)
            .map_err(PrinterLookupError::NotFound),
        Target::Unknown => Err(PrinterLookupError::NotFound(format!(
            "Printer '{}' was not found on this computer",
            name
        ))),
        Target::Remote { .. } if hop => Err(PrinterLookupError::Remote(RemoteError::BadRequest(
            "Forwarding between ix.print computers is limited to one hop".into(),
        ))),
        Target::Remote { remote, printer } => state
            .remote_gateway
            .capabilities(&remote, &printer)
            .map_err(PrinterLookupError::Remote),
    }
}

pub fn queue(state: &AppState, name: &str, hop: bool) -> Result<PrinterQueue, PrinterLookupError> {
    match crate::application::remote_service::resolve(state, name) {
        Target::Local(printer) => state
            .printer_gateway
            .queue(&printer)
            .map_err(PrinterLookupError::NotFound),
        Target::Unknown => Err(PrinterLookupError::NotFound(format!(
            "Printer '{}' was not found on this computer",
            name
        ))),
        Target::Remote { .. } if hop => Err(PrinterLookupError::Remote(RemoteError::BadRequest(
            "Forwarding between ix.print computers is limited to one hop".into(),
        ))),
        Target::Remote { remote, printer } => {
            let mut queue = state
                .remote_gateway
                .queue(&remote, &printer)
                .map_err(PrinterLookupError::Remote)?;
            queue.printer = name.to_string();
            Ok(queue)
        }
    }
}

pub fn job_status_json(state: &AppState, job_id: &str) -> Option<serde_json::Value> {
    let entry = state.history.find(job_id)?;
    let base = serde_json::json!({
        "job_id": entry.id,
        "printer": entry.printer,
        "spooler_job": entry.spooler_job,
    });
    let status = if !entry.success {
        serde_json::json!({ "state": "failed", "message": entry.error })
    } else if !status_is_recent(&entry.time) {
        serde_json::json!({ "state": "unknown", "message": "Live queue status is unavailable for older jobs" })
    } else if let Some(remote_job) = &entry.remote {
        let mut status = match state
            .remotes
            .list()
            .into_iter()
            .find(|remote| remote.id == remote_job.remote_id)
        {
            Some(remote) => match state.remote_gateway.job_status(&remote, &remote_job.job_id) {
                Ok(status) => status,
                Err(error) => {
                    serde_json::json!({ "state": "unknown", "message": error.to_string(), "remote_unreachable": true })
                }
            },
            None => serde_json::json!({
                "state": "unknown",
                "message": format!("Remote '{}' was removed", remote_job.remote_name)
            }),
        };
        if let Some(object) = status.as_object_mut() {
            object.insert("job_id".into(), serde_json::json!(entry.id));
            object.insert("printer".into(), serde_json::json!(entry.printer));
            object.insert("remote".into(), serde_json::json!(remote_job.remote_name));
        }
        status
    } else {
        match entry
            .spooler_job
            .as_deref()
            .map(|id| state.printer_gateway.job_status(&entry.printer, id))
        {
            Some(Ok(status)) => serde_json::to_value(status).unwrap_or_default(),
            Some(Err(error)) => serde_json::json!({ "state": "unknown", "message": error }),
            None => serde_json::json!({ "state": "unknown" }),
        }
    };
    let mut merged = base;
    if let (Some(target), Some(source)) = (merged.as_object_mut(), status.as_object()) {
        target.extend(source.clone());
    }
    Some(merged)
}

fn status_is_recent(time: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(time).ok().is_some_and(|then| {
        let age = chrono::Utc::now().signed_duration_since(then.with_timezone(&chrono::Utc));
        age >= chrono::Duration::zero() && age < chrono::Duration::days(2)
    })
}

pub fn reject_remote_printer(state: &AppState, name: &str) -> Result<(), String> {
    if matches!(
        crate::application::remote_service::resolve(state, name),
        Target::Remote { .. }
    ) {
        Err("Printers of another ix.print are managed on that computer".into())
    } else {
        Ok(())
    }
}

pub fn set_default(state: &AppState, printer: Option<String>) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.default_printer = printer.clone().filter(|value| !value.trim().is_empty());
        Ok(())
    })
}

pub fn engine_status(state: &AppState) -> Result<(), String> {
    state.pdf_engine.engine_status()
}

pub fn devices(state: &AppState) -> Result<Vec<Device>, String> {
    state.printer_admin.devices()
}

pub fn drivers(state: &AppState, query: &str) -> Result<Vec<Driver>, String> {
    state.printer_admin.drivers(query)
}

pub fn details(state: &AppState, name: &str) -> Result<PrinterDetails, String> {
    reject_remote_printer(state, name)?;
    state.printer_admin.details(name)
}

pub fn add(state: &AppState, printer: &NewPrinter) -> Result<(), String> {
    state.printer_admin.add(printer)
}

pub fn update(state: &AppState, name: &str, edit: &PrinterEdit) -> Result<(), String> {
    state.printer_admin.update(name, edit)
}

pub fn update_local(state: &AppState, name: &str, edit: &PrinterEdit) -> Result<Config, String> {
    reject_remote_printer(state, name)?;
    let new_name = edit.name.clone();
    state.printer_admin.update(name, edit)?;
    if name == new_name {
        return Ok(state.config.get());
    }
    state.config.update(&mut |config| {
        let rename = |printer: &mut Option<String>| {
            if printer.as_deref() == Some(name) {
                *printer = Some(new_name.clone());
            }
        };
        rename(&mut config.default_printer);
        for preset in config
            .presets
            .values_mut()
            .chain(config.preset_drafts.values_mut())
            .chain(config.preset_previous.values_mut())
        {
            rename(&mut preset.settings.printer);
        }
        Ok(())
    })
}

pub fn remove(state: &AppState, name: &str) -> Result<Config, String> {
    reject_remote_printer(state, name)?;
    state.printer_admin.remove(name)?;
    state.config.update(&mut |config| {
        if config.default_printer.as_deref() == Some(name) {
            config.default_printer = None;
        }
        Ok(())
    })
}

pub fn set_os_default(state: &AppState, name: &str) -> Result<(), String> {
    reject_remote_printer(state, name)?;
    state.printer_admin.set_os_default(name)
}

#[cfg(test)]
mod cancel_tests {
    use super::*;
    use crate::application::ports::PrinterGateway;
    use crate::application::state::test_support::state;
    use crate::domain::history::HistoryEntry;
    use crate::domain::layout::Sheet;
    use crate::domain::printer::{Capabilities, DeviceOptions, JobState, JobStatus, Printer, PrinterQueue, QueueJob};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct QueuePrinter {
        state: JobState,
        cancelled: AtomicUsize,
    }
    impl PrinterGateway for QueuePrinter {
        fn list_printers(&self) -> Vec<Printer> {
            vec![]
        }
        fn os_default_printer(&self) -> Option<String> {
            None
        }
        fn capabilities(&self, _: &str) -> Result<Capabilities, String> {
            unreachable!()
        }
        fn default_paper_mm(&self, _: &str) -> (f32, f32) {
            (210.0, 297.0)
        }
        fn print(&self, _: &str, _: &Path, _: &[Sheet], _: &DeviceOptions) -> Result<Option<String>, String> {
            unreachable!()
        }
        fn queue(&self, printer: &str) -> Result<PrinterQueue, String> {
            Ok(PrinterQueue {
                printer: printer.into(),
                status: "Normal".into(),
                message: None,
                jobs: vec![QueueJob {
                    id: "spool-1".into(),
                    document: "test".into(),
                    position: 1,
                    state: self.state,
                    message: None,
                    pages: None,
                    pages_printed: None,
                }],
            })
        }
        fn job_status(&self, _: &str, _: &str) -> Result<JobStatus, String> {
            unreachable!()
        }
        fn cancel(&self, _: &str, _: &str) -> Result<(), String> {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn cancellation_is_limited_to_own_waiting_job() {
        let mut app = Arc::try_unwrap(state()).ok().unwrap();
        let printer = Arc::new(QueuePrinter {
            state: JobState::Queued,
            cancelled: AtomicUsize::new(0),
        });
        app.printer_gateway = printer.clone();
        let entry = HistoryEntry {
            id: "own-job".into(),
            time: chrono::Local::now().to_rfc3339(),
            source: "app".into(),
            client: None,
            token: None,
            file_name: "test".into(),
            preset: None,
            printer: "Printer".into(),
            spooler_job: Some("spool-1".into()),
            sheets: 1,
            settings: serde_json::json!({}),
            success: true,
            error: None,
            duration_ms: 1,
            callback: None,
            remote: None,
            content: None,
        };
        app.history.append(&entry).unwrap();
        assert!(cancel_own_job(&app, "own-job").is_ok());
        assert_eq!(printer.cancelled.load(Ordering::SeqCst), 1);
        assert!(cancel_own_job(&app, "someone-else").is_err());
        assert_eq!(printer.cancelled.load(Ordering::SeqCst), 1);
        app.printer_gateway = Arc::new(QueuePrinter {
            state: JobState::Printing,
            cancelled: AtomicUsize::new(0),
        });
        assert!(cancel_own_job(&app, "own-job").is_err());
    }
}
