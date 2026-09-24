use serde::Serialize;

use crate::application::state::AppState;
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
        config
            .presets
            .values_mut()
            .for_each(|preset| rename(&mut preset.settings.printer));
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
