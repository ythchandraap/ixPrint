use std::time::{Duration, Instant};

use crate::application::state::AppState;
use crate::domain::history::{HistoryEntry, RemoteJob};
use crate::domain::job::{design_settings, Document, JobError, JobRequest, JobResult, MAX_TEST_PAGES};
use crate::domain::printer::Printer;
use crate::domain::remote::{CacheEntry, Remote, RemoteError, RemotePrintResult, RemoteView, Target, SEPARATOR};
use crate::domain::settings::PrintSettings;
use crate::shared::sync::LockExt;

const CACHE_TTL: Duration = Duration::from_secs(5);

pub fn resolve(state: &AppState, name: &str) -> Target {
    let local = state
        .printer_gateway
        .list_printers()
        .into_iter()
        .map(|printer| printer.name)
        .collect::<Vec<_>>();
    crate::domain::remote::resolve(&local, &state.remotes.list(), name)
}

pub fn printers(state: &AppState, remote: &Remote) -> (Vec<Printer>, Option<String>) {
    refresh_printers(state, remote, false)
}

pub fn refresh_printers(state: &AppState, remote: &Remote, force: bool) -> (Vec<Printer>, Option<String>) {
    if !force {
        if let Some(entry) = state.remote_cache.lock_safe().get(&remote.id) {
            if entry.fetched.elapsed() < CACHE_TTL {
                return (
                    map_printers(
                        remote,
                        &entry.printers,
                        (!entry.ok_once).then_some(entry.error.as_deref().unwrap_or("Remote has not been reached")),
                    ),
                    entry.error.clone(),
                );
            }
        }
    }
    match state.remote_gateway.fetch_printers(remote) {
        Ok(list) => {
            let mut cache = state.remote_cache.lock_safe();
            cache.insert(
                remote.id.clone(),
                CacheEntry {
                    fetched: Instant::now(),
                    printers: list.clone(),
                    error: None,
                    ok_once: true,
                },
            );
            (map_printers(remote, &list, None), None)
        }
        Err(error) => {
            let message = error.to_string();
            let mut cache = state.remote_cache.lock_safe();
            let entry = cache.entry(remote.id.clone()).or_insert_with(|| CacheEntry {
                fetched: Instant::now(),
                printers: Vec::new(),
                error: None,
                ok_once: false,
            });
            entry.fetched = Instant::now();
            entry.error = Some(message.clone());
            (map_printers(remote, &entry.printers, Some(&message)), Some(message))
        }
    }
}

fn map_printers(remote: &Remote, printers: &[Printer], offline: Option<&str>) -> Vec<Printer> {
    printers
        .iter()
        .cloned()
        .map(|mut printer| {
            printer.name = format!("{}{}{}", remote.name, SEPARATOR, printer.name);
            printer.os_default = false;
            if let Some(error) = offline {
                printer.status = "Offline".into();
                printer.message = Some(error.to_string());
            }
            printer
        })
        .collect()
}

pub fn view(remote: &Remote, cache: &std::sync::Mutex<std::collections::HashMap<String, CacheEntry>>) -> RemoteView {
    let cache = cache.lock_safe();
    let entry = cache.get(&remote.id);
    RemoteView {
        id: remote.id.clone(),
        name: remote.name.clone(),
        url: remote.url.clone(),
        preset: remote.preset.clone(),
        token_hint: format!("{}…", &remote.token[..remote.token.len().min(10)]),
        created_at: remote.created_at.clone(),
        online: entry.map(|entry| entry.ok_once && entry.error.is_none()),
        last_error: entry.and_then(|entry| entry.error.clone()),
        printers: entry.map(|entry| entry.printers.len()).unwrap_or(0),
    }
}

pub fn views(state: &AppState, remotes: &[Remote]) -> Vec<RemoteView> {
    remotes.iter().map(|remote| view(remote, &state.remote_cache)).collect()
}

pub fn list(state: &AppState) -> Vec<RemoteView> {
    views(state, &state.remotes.list())
}

pub fn build_remote(
    state: &AppState,
    name: String,
    url: String,
    token: String,
    preset: Option<String>,
    existing: Option<&Remote>,
) -> Result<Remote, String> {
    crate::domain::remote::validate_name(&name)?;
    let url = crate::domain::remote::normalize_url(&url)?;
    let token = token.trim().to_string();
    crate::domain::remote::validate_token(&token)?;
    Ok(Remote {
        id: existing
            .map(|remote| remote.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        url,
        token,
        preset: preset
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string()),
        created_at: existing
            .map(|remote| remote.created_at.clone())
            .unwrap_or_else(|| state.clock.now_rfc3339()),
    })
}

pub fn add(state: &AppState, remote: Remote) -> Result<Vec<RemoteView>, String> {
    let (_, error) = refresh_printers(state, &remote, true);
    if let Some(error) = error {
        return Err(error);
    }
    state.remotes.update(&mut |list| {
        if list.iter().any(|item| item.name.eq_ignore_ascii_case(&remote.name)) {
            return Err(format!("Remote name '{}' is already used", remote.name));
        }
        list.push(remote.clone());
        Ok(())
    })?;
    Ok(list(state))
}

pub fn update(
    state: &AppState,
    id: &str,
    name: String,
    url: String,
    token: Option<String>,
    preset: Option<String>,
) -> Result<Vec<RemoteView>, String> {
    let old = state
        .remotes
        .list()
        .into_iter()
        .find(|remote| remote.id == id)
        .ok_or("Remote computer was not found")?;
    let remote = build_remote(state, name, url, token.unwrap_or(old.token.clone()), preset, Some(&old))?;
    let (_, error) = refresh_printers(state, &remote, true);
    if let Some(error) = error {
        return Err(error);
    }
    state.remotes.update(&mut |list| {
        let item = list
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("Remote computer was not found")?;
        *item = remote.clone();
        Ok(())
    })?;
    if old.name != remote.name {
        let old_prefix = format!("{}{}", old.name, SEPARATOR);
        let new_prefix = format!("{}{}", remote.name, SEPARATOR);
        state.config.update(&mut |config| {
            let rename = |value: &mut Option<String>| {
                if let Some(value) = value.as_mut() {
                    if let Some(rest) = value.strip_prefix(&old_prefix) {
                        *value = format!("{}{}", new_prefix, rest);
                    }
                }
            };
            rename(&mut config.default_printer);
            config
                .presets
                .values_mut()
                .for_each(|preset| rename(&mut preset.settings.printer));
            Ok(())
        })?;
    }
    Ok(list(state))
}

pub fn remove(state: &AppState, id: &str) -> Result<Vec<RemoteView>, String> {
    let remote = state
        .remotes
        .list()
        .into_iter()
        .find(|remote| remote.id == id)
        .ok_or("Remote computer was not found")?;
    state.remotes.update(&mut |list| {
        list.retain(|remote| remote.id != id);
        Ok(())
    })?;
    state.remote_cache.lock_safe().remove(id);
    state.config.update(&mut |config| {
        if config.default_printer.as_deref().is_some_and(|printer| {
            printer == remote.name || printer.starts_with(&format!("{}{}", remote.name, SEPARATOR))
        }) {
            config.default_printer = None;
        }
        Ok(())
    })?;
    Ok(list(state))
}

pub fn test(state: &AppState, id: &str) -> Result<RemoteView, String> {
    let remote = state
        .remotes
        .list()
        .into_iter()
        .find(|remote| remote.id == id)
        .ok_or("Remote computer was not found")?;
    refresh_printers(state, &remote, true);
    Ok(view(&remote, &state.remote_cache))
}

pub(crate) struct ForwardRequest<'a> {
    pub job_id: &'a str,
    pub preset_name: Option<&'a str>,
    pub preset: &'a crate::domain::preset::Preset,
    pub merged: &'a PrintSettings,
    pub req: &'a JobRequest,
    pub remote: &'a Remote,
    pub remote_printer: &'a str,
    pub record: &'a mut HistoryEntry,
}

pub(crate) fn forward(state: &AppState, request: ForwardRequest<'_>) -> Result<JobResult, JobError> {
    let ForwardRequest {
        job_id,
        preset_name,
        preset,
        merged,
        req,
        remote,
        remote_printer,
        record,
    } = request;
    if req.hop {
        return Err(JobError::BadRequest(
            "Forwarding between ix.print computers is limited to one hop".into(),
        ));
    }
    let mut settings = merged.clone();
    settings.printer = Some(remote_printer.to_string());
    let generated = state.jobs_dir.join(format!("{}-remote.pdf", job_id));
    let result = (|| -> Result<RemotePrintResult, RemoteError> {
        match &req.document {
            Document::File(path) => state.remote_gateway.print_pdf(remote, path, &req.file_name, &settings),
            Document::Data { records, header } => {
                let design = preset.design.as_ref().ok_or_else(|| {
                    RemoteError::BadRequest(match preset_name {
                        Some(name) => format!("Preset '{}' has no design; send a PDF instead of JSON data", name),
                        None => "Specify a preset with a design in the \"preset\" field".into(),
                    })
                })?;
                if records.len() > crate::domain::template::MAX_RECORDS {
                    return Err(RemoteError::BadRequest(format!(
                        "at most {} records per job",
                        crate::domain::template::MAX_RECORDS
                    )));
                }
                design.preflight(records, header).map_err(RemoteError::BadRequest)?;
                state
                    .pdf_engine
                    .render_template(&generated, design, records, header)
                    .map_err(RemoteError::BadRequest)?;
                design_settings(&mut settings, design, false);
                state
                    .remote_gateway
                    .print_pdf(remote, &generated, &req.file_name, &settings)
            }
            Document::TestPage { pages } if preset.design.is_some() => {
                let design = preset.design.as_ref().expect("guard checked design");
                let records = vec![design.sample.clone(); (*pages).clamp(1, MAX_TEST_PAGES) as usize];
                state
                    .pdf_engine
                    .render_template(&generated, design, &records, &design.sample)
                    .map_err(RemoteError::BadRequest)?;
                design_settings(&mut settings, design, true);
                state
                    .remote_gateway
                    .print_pdf(remote, &generated, &req.file_name, &settings)
            }
            Document::TestPage { pages } => state.remote_gateway.test_page(remote, *pages, &settings),
        }
    })();
    state.pdf_engine.remove_generated_file(&generated);
    let result = result.map_err(RemoteError::into_job_error)?;
    record.settings = serde_json::to_value(&settings).unwrap_or_default();
    record.sheets = result.sheets;
    record.remote = Some(RemoteJob {
        remote_id: remote.id.clone(),
        remote_name: remote.name.clone(),
        job_id: result.job_id,
    });
    Ok(JobResult {
        job_id: job_id.to_string(),
        printer: format!("{}{}{}", remote.name, SEPARATOR, remote_printer),
        preset: preset_name.map(str::to_string),
        sheets: result.sheets,
        spooler_job: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::state::test_support::state;
    use crate::domain::remote::Remote;

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
            duration_ms: 0,
            callback: None,
            remote: None,
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
}
