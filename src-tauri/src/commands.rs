//! Commands called by the ix.print window. Anything that touches printers runs
//! on a worker thread so the window stays responsive.

use crate::config::{self, Config, ImportOptions, ImportPreview, PresetBundle};
use crate::history::{HistoryDay, HistoryPage};
use crate::job::{self, AppState, Document, JobRequest, JobResult};
use crate::remote::{self, Remote, RemoteView, Target};
use crate::server::{job_status_json, printer_views, PrinterView};
use crate::settings::PrintSettings;
use crate::sync::LockExt;
use crate::{layout, printer};
use local_ip_address::local_ip;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

type AppStateRef<'a> = State<'a, Arc<AppState>>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// Opens the API documentation served at `/docs`, in its own window or the browser.
#[tauri::command]
pub async fn open_docs(app: tauri::AppHandle, state: AppStateRef<'_>, external: bool) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;

    let url = format!("http://127.0.0.1:{}/docs", state.config.get().server_port);
    if external {
        return app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string());
    }
    if let Some(window) = app.get_webview_window("docs") {
        let _ = window.unminimize();
        return window.set_focus().map_err(|e| e.to_string());
    }
    let url = url.parse::<tauri::Url>().map_err(|e| e.to_string())?;
    tauri::WebviewWindowBuilder::new(&app, "docs", tauri::WebviewUrl::External(url))
        .title("ix.print · Dokumentasi API")
        .inner_size(1100.0, 820.0)
        .min_inner_size(420.0, 500.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The folder where ix.print stores config, history and job files.
#[tauri::command]
pub fn data_folder_path(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Opens that folder in the OS file explorer, with config.json selected
/// when it already exists.
#[tauri::command]
pub fn open_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_file = dir.join("config.json");
    let target = if config_file.exists() { config_file } else { dir };
    app.opener().reveal_item_in_dir(target).map_err(|e| e.to_string())
}

/// LAN address of this computer; `None` when it has no network connection.
#[tauri::command]
pub fn this_local_ip() -> Option<String> {
    local_ip().ok().map(|ip| ip.to_string())
}

#[tauri::command]
pub fn os_detected() -> String {
    tauri_plugin_os::platform().to_string()
}

/// `Err` with the reason when the HTTP server is not running.
#[tauri::command]
pub fn server_status(state: AppStateRef<'_>) -> Result<(), String> {
    if !state.config.get().bridge_enabled {
        return Err("The application bridge is disabled".into());
    }
    match state.server_error.lock_safe().clone() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[tauri::command]
pub fn bridge_enabled(state: AppStateRef<'_>) -> bool {
    state.config.get().bridge_enabled
}

/// Enables or gracefully stops the local HTTP bridge while keeping the app open.
#[tauri::command]
pub async fn set_bridge_enabled(state: AppStateRef<'_>, enabled: bool) -> Result<Config, String> {
    set_bridge_enabled_inner(state.inner().clone(), enabled).await
}

/// Shared by the window command and the system-tray action.
pub async fn set_bridge_enabled_inner(state: Arc<AppState>, enabled: bool) -> Result<Config, String> {
    state.config.update(|c| {
        c.bridge_enabled = enabled;
        Ok(())
    })?;

    if enabled {
        state.server_error.lock_safe().take();
        let running = state.server_handle.lock_safe().is_some();
        if !running {
            crate::server::spawn(state.clone());
        }
    } else {
        let handle = state.server_handle.lock_safe().take();
        state.server_error.lock_safe().take();
        if let Some(handle) = handle {
            handle.stop(true).await;
        }
    }
    Ok(state.config.get())
}

/// Changes the bridge port and restarts it when the bridge is enabled.
#[tauri::command]
pub async fn set_server_port(state: AppStateRef<'_>, port: u16) -> Result<Config, String> {
    let state = state.inner().clone();
    let current = state.config.get();
    if current.server_port == port {
        return Ok(current);
    }

    let next = state.config.update(|c| {
        c.server_port = port;
        Ok(())
    })?;

    if next.bridge_enabled {
        let handle = state.server_handle.lock_safe().take();
        state.server_error.lock_safe().take();
        if let Some(handle) = handle {
            handle.stop(true).await;
        }
        crate::server::spawn(state);
    }
    Ok(next)
}

fn remote_from_form(
    name: String,
    url: String,
    token: String,
    preset: Option<String>,
    existing: Option<&Remote>,
) -> Result<Remote, String> {
    remote::validate_name(&name)?;
    let url = remote::normalize_url(&url)?;
    let token = token.trim().to_string();
    remote::validate_token(&token)?;
    Ok(Remote {
        id: existing
            .map(|r| r.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        url,
        token,
        preset: preset.filter(|p| !p.trim().is_empty()).map(|p| p.trim().to_string()),
        created_at: existing
            .map(|r| r.created_at.clone())
            .unwrap_or_else(|| chrono::Local::now().to_rfc3339()),
    })
}

#[tauri::command]
pub fn remote_list(state: AppStateRef<'_>) -> Vec<RemoteView> {
    remote::views(&state.remotes, &state.remote_cache)
}

#[tauri::command]
pub async fn remote_add(
    state: AppStateRef<'_>,
    name: String,
    url: String,
    token: String,
    preset: Option<String>,
) -> Result<Vec<RemoteView>, String> {
    let remote = remote_from_form(name, url, token, preset, None)?;
    let state = state.inner().clone();
    let tested = remote.clone();
    let io_state = state.clone();
    let (_, error) = blocking(move || remote::refresh_printers(&io_state, &tested, true)).await?;
    if let Some(error) = error {
        return Err(error);
    }
    state.remotes.update(|list| {
        if list.iter().any(|r| r.name.eq_ignore_ascii_case(&remote.name)) {
            return Err(format!("Remote name '{}' is already used", remote.name));
        }
        list.push(remote.clone());
        Ok(())
    })?;
    Ok(remote::views(&state.remotes, &state.remote_cache))
}

#[tauri::command]
pub async fn remote_update(
    state: AppStateRef<'_>,
    id: String,
    name: String,
    url: String,
    token: Option<String>,
    preset: Option<String>,
) -> Result<Vec<RemoteView>, String> {
    let state = state.inner().clone();
    let old = state
        .remotes
        .list()
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("Remote computer was not found")?;
    let remote = remote_from_form(name, url, token.unwrap_or(old.token.clone()), preset, Some(&old))?;
    let tested = remote.clone();
    let io_state = state.clone();
    let (_, error) = blocking(move || remote::refresh_printers(&io_state, &tested, true)).await?;
    if let Some(error) = error {
        return Err(error);
    }
    state.remotes.update(|list| {
        let item = list
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Remote computer was not found")?;
        *item = remote.clone();
        Ok(())
    })?;
    if old.name != remote.name {
        let old_prefix = format!("{}{}", old.name, remote::SEPARATOR);
        let new_prefix = format!("{}{}", remote.name, remote::SEPARATOR);
        state.config.update(|config| {
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
                .for_each(|p| rename(&mut p.settings.printer));
            Ok(())
        })?;
    }
    Ok(remote::views(&state.remotes, &state.remote_cache))
}

#[tauri::command]
pub fn remote_remove(state: AppStateRef<'_>, id: String) -> Result<Vec<RemoteView>, String> {
    let remote = state
        .remotes
        .list()
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("Remote computer was not found")?;
    state.remotes.update(|list| {
        list.retain(|r| r.id != id);
        Ok(())
    })?;
    state.remote_cache.lock_safe().remove(&id);
    state.config.update(|config| {
        if config
            .default_printer
            .as_deref()
            .is_some_and(|p| p == remote.name || p.starts_with(&format!("{}{}", remote.name, remote::SEPARATOR)))
        {
            config.default_printer = None;
        }
        Ok(())
    })?;
    Ok(remote::views(&state.remotes, &state.remote_cache))
}

#[tauri::command]
pub async fn remote_test(state: AppStateRef<'_>, id: String) -> Result<RemoteView, String> {
    let state = state.inner().clone();
    let remote = state
        .remotes
        .list()
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("Remote computer was not found")?;
    blocking(move || {
        remote::refresh_printers(&state, &remote, true);
        remote::view(&remote, &state.remote_cache)
    })
    .await
}

/// `Err` with the reason when PDFium could not be loaded.
#[tauri::command]
pub fn engine_status() -> Result<(), String> {
    layout::pdfium().map(|_| ())
}

#[tauri::command]
pub async fn get_printers(state: AppStateRef<'_>) -> Result<Vec<PrinterView>, String> {
    let state = state.inner().clone();
    blocking(move || printer_views(&state, true)).await
}

#[tauri::command]
pub async fn get_capabilities(state: AppStateRef<'_>, printer: String) -> Result<printer::Capabilities, String> {
    let state = state.inner().clone();
    blocking(move || {
        let local = printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>();
        match remote::resolve(&local, &state.remotes.list(), &printer) {
            Target::Local(name) => printer::capabilities(&name),
            Target::Unknown => Err(format!("Printer '{}' was not found on this computer", printer)),
            Target::Remote { remote, printer } => remote::capabilities(&remote, &printer).map_err(|e| e.to_string()),
        }
    })
    .await?
}

#[tauri::command]
pub async fn get_queue(state: AppStateRef<'_>, printer: String) -> Result<printer::PrinterQueue, String> {
    let state = state.inner().clone();
    blocking(move || {
        let local = printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>();
        match remote::resolve(&local, &state.remotes.list(), &printer) {
            Target::Local(name) => printer::queue(&name),
            Target::Unknown => Err(format!("Printer '{}' was not found on this computer", printer)),
            Target::Remote {
                remote,
                printer: remote_printer,
            } => {
                let mut queue = remote::queue(&remote, &remote_printer).map_err(|e| e.to_string())?;
                queue.printer = printer;
                Ok(queue)
            }
        }
    })
    .await?
}

/// Live status of a job printed through ix.print (see `/job-status`).
#[tauri::command]
pub async fn job_status(state: AppStateRef<'_>, job_id: String) -> Result<serde_json::Value, String> {
    let state = state.inner().clone();
    blocking(move || job_status_json(&state, &job_id))
        .await?
        .ok_or_else(|| "Job not found".to_string())
}

/// Sends the desktop form through the same authenticated HTTP endpoint used by
/// external clients. The process-only key never leaves Rust or appears in logs.
#[tauri::command]
pub async fn app_print(
    state: AppStateRef<'_>,
    json_body: Option<String>,
    pdf_path: Option<String>,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<JobResult, String> {
    let state = state.inner().clone();
    if !state.config.get().bridge_enabled {
        return Err("Print service is disabled. Turn on the bridge in the status bar, then try again.".into());
    }
    if json_body.is_some() == pdf_path.is_some() {
        return Err("Choose exactly one source: JSON data or a PDF file.".into());
    }
    blocking(move || submit_to_print_endpoint(&state, json_body, pdf_path, preset, settings)).await?
}

fn submit_to_print_endpoint(
    state: &AppState,
    json_body: Option<String>,
    pdf_path: Option<String>,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<JobResult, String> {
    let port = state.config.get().server_port;
    let endpoint = format!("http://127.0.0.1:{}/print", port);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(concat!("ix.print/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let mut response = if let Some(body) = json_body {
        agent
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("X-IxPrint-Desktop", &state.desktop_key)
            .send(body)
    } else if let Some(path) = pdf_path {
        let path = PathBuf::from(path);
        if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
            return Err("Choose a PDF file with the .pdf extension.".into());
        }
        let mut params = Vec::new();
        if let Some(preset) = preset.filter(|p| !p.trim().is_empty()) {
            params.push(("preset", preset));
        }
        let settings_json = serde_json::to_value(&settings).map_err(|_| "Could not read print settings".to_string())?;
        if let Some(values) = settings_json.as_object() {
            for (name, value) in values {
                if value.is_null() { continue; }
                let value = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
                params.push((name.as_str(), value));
            }
        }
        let query = params
            .iter()
            .map(|(name, value)| format!("{}={}", encode_query(name), encode_query(value)))
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() { endpoint } else { format!("{}?{}", endpoint, query) };
        let form = ureq::unversioned::multipart::Form::new()
            .file("file", &path)
            .map_err(|_| format!("Could not read PDF file: {}", path.file_name().unwrap_or_default().to_string_lossy()))?;
        agent
            .post(&url)
            .header("X-IxPrint-Desktop", &state.desktop_key)
            .send(form)
    } else {
        return Err("Choose JSON data or a PDF file before printing.".into());
    }
    .map_err(|_| format!("Could not reach the local print service on port {}. Check that the bridge is running; check History before retrying in case the job was already submitted.", port))?;

    let status = response.status();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    if !status.is_success() {
        let message = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("The print service rejected the request");
        return Err(if status.as_u16() == 401 {
            "The desktop print request was denied. Restart ix.print and try again.".into()
        } else {
            format!("Print request failed (HTTP {}): {}", status, message)
        });
    }
    serde_json::from_value(value)
        .map_err(|_| "The print service returned an unreadable job result. Check History before retrying.".into())
}

fn encode_query(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

/// PNG data URL of the first sheet a PDF file would print as, laid out with
/// `layout::build` — the exact same code path the real print job uses — so
/// the preview matches the printed output 1:1.
#[tauri::command]
pub async fn preview_pdf_sheet(
    state: AppStateRef<'_>,
    pdf_path: String,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<String, String> {
    let state = state.inner().clone();
    blocking(move || render_pdf_sheet_preview(&state, &pdf_path, preset, settings)).await?
}

fn render_pdf_sheet_preview(
    state: &AppState,
    pdf_path: &str,
    preset: Option<String>,
    settings: PrintSettings,
) -> Result<String, String> {
    use base64::Engine;

    let path = PathBuf::from(pdf_path);
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
        return Err("Choose a PDF file with the .pdf extension.".into());
    }
    let config = state.config.get();
    let preset_settings = match preset.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(name) => config
            .presets
            .get(name)
            .map(|p| p.settings.clone())
            .ok_or_else(|| format!("Preset '{}' was not found", name))?,
        None => Default::default(),
    };
    let merged = preset_settings.overlay(&settings);
    merged.validate()?;

    let printer_name = merged
        .printer
        .clone()
        .or_else(|| state.default_printer())
        .unwrap_or_default();
    let job = crate::settings::JobSettings::resolve(&merged, printer_name.clone());
    let default_paper = if job.paper == crate::settings::Paper::PrinterDefault && !printer_name.is_empty() {
        printer::default_paper_mm(&printer_name)
    } else {
        (210.0, 297.0)
    };

    let tmp = state.jobs_dir.join(format!("preview-{}.pdf", uuid::Uuid::new_v4()));
    let result = (|| -> Result<String, String> {
        layout::build(&path, &tmp, &job, default_paper)?;
        let pdfium = layout::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&tmp, None)
            .map_err(|e| format!("Could not open the laid-out sheet: {}", e))?;
        let png = crate::template::render_page_png(&doc, 0, 6.0)?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

// ---- printer devices (OS level; not exposed over HTTP) ----------------------

#[tauri::command]
pub async fn printer_devices() -> Result<Vec<printer::manage::Device>, String> {
    blocking(printer::manage::devices).await?
}

#[tauri::command]
pub async fn printer_drivers(query: String) -> Result<Vec<printer::manage::Driver>, String> {
    blocking(move || printer::manage::drivers(&query)).await?
}

#[tauri::command]
pub async fn printer_details(state: AppStateRef<'_>, name: String) -> Result<printer::manage::PrinterDetails, String> {
    reject_remote_printer(state.inner().clone(), &name).await?;
    blocking(move || printer::manage::details(&name)).await?
}

#[tauri::command]
pub async fn printer_add(printer: printer::manage::NewPrinter) -> Result<(), String> {
    blocking(move || printer::manage::add(&printer)).await?
}

/// Edits a printer; a rename also updates the default printer and presets.
#[tauri::command]
pub async fn printer_update(
    state: AppStateRef<'_>,
    name: String,
    edit: printer::manage::PrinterEdit,
) -> Result<Config, String> {
    reject_remote_printer(state.inner().clone(), &name).await?;
    let (old, new) = (name.clone(), edit.name.clone());
    blocking(move || printer::manage::update(&name, &edit)).await??;
    if old == new {
        return Ok(state.config.get());
    }
    state.config.update(|c| {
        let rename = |p: &mut Option<String>| {
            if p.as_deref() == Some(old.as_str()) {
                *p = Some(new.clone());
            }
        };
        rename(&mut c.default_printer);
        c.presets.values_mut().for_each(|p| rename(&mut p.settings.printer));
        Ok(())
    })
}

#[tauri::command]
pub async fn printer_remove(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    reject_remote_printer(state.inner().clone(), &name).await?;
    let target = name.clone();
    blocking(move || printer::manage::remove(&target)).await??;
    state.config.update(|c| {
        if c.default_printer.as_deref() == Some(name.as_str()) {
            c.default_printer = None;
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn printer_set_os_default(state: AppStateRef<'_>, name: String) -> Result<(), String> {
    reject_remote_printer(state.inner().clone(), &name).await?;
    blocking(move || printer::manage::set_os_default(&name)).await?
}

async fn reject_remote_printer(state: Arc<AppState>, name: &str) -> Result<(), String> {
    let name = name.to_string();
    blocking(move || {
        let local = printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>();
        if matches!(
            remote::resolve(&local, &state.remotes.list(), &name),
            Target::Remote { .. }
        ) {
            Err("Printers of another ix.print are managed on that computer".into())
        } else {
            Ok(())
        }
    })
    .await?
}

#[tauri::command]
pub fn get_config(state: AppStateRef<'_>) -> Config {
    state.config.get()
}

#[tauri::command]
pub fn set_default_printer(state: AppStateRef<'_>, printer: Option<String>) -> Result<Config, String> {
    state.config.update(|c| {
        c.default_printer = printer.filter(|p| !p.trim().is_empty());
        Ok(())
    })
}

#[tauri::command]
pub fn set_default_preset(state: AppStateRef<'_>, name: Option<String>) -> Result<Config, String> {
    state.config.update(|c| {
        c.default_preset = name.filter(|p| !p.is_empty());
        Ok(())
    })
}

/// Creates or updates a preset (settings + optional design); `original_name` renames it.
#[tauri::command]
pub fn save_preset(
    state: AppStateRef<'_>,
    name: String,
    original_name: Option<String>,
    preset: config::Preset,
) -> Result<Config, String> {
    config::validate_preset_name(&name)?;
    preset.validate()?;
    state.config.update(|c| {
        if let Some(original) = original_name.filter(|o| o != &name) {
            if c.presets.contains_key(&name) {
                return Err(format!("Preset '{}' already exists", name));
            }
            c.presets.remove(&original);
            if c.default_preset.as_ref() == Some(&original) {
                c.default_preset = Some(name.clone());
            }
        }
        c.presets.insert(name, preset);
        Ok(())
    })
}

/// Print preview: the design filled with its sample data `rows` times; returns
/// up to 6 page images (PNG data URLs) and the total page count.
#[tauri::command]
pub async fn preview_print(
    template: crate::template::Template,
    rows: usize,
    px_per_mm: Option<f32>,
) -> Result<(Vec<String>, usize), String> {
    use base64::Engine;
    let scale = px_per_mm.unwrap_or(4.0).clamp(1.0, 12.0);
    let records = vec![template.sample.clone(); rows.clamp(1, crate::template::MAX_RECORDS)];
    let (pages, total) =
        blocking(move || crate::template::pages_png(&template, &records, &template.sample, 6, scale)).await??;
    let urls = pages
        .into_iter()
        .map(|png| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        })
        .collect();
    Ok((urls, total))
}

/// PNG data URL of a design filled with `data`, rendered exactly as it prints.
#[tauri::command]
pub async fn preview_template(
    template: crate::template::Template,
    data: crate::template::Record,
    px_per_mm: Option<f32>,
) -> Result<String, String> {
    use base64::Engine;
    let scale = px_per_mm.unwrap_or(8.0).clamp(1.0, 24.0);
    let png = blocking(move || crate::template::preview_png(&template, &data, scale)).await??;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

#[tauri::command]
pub fn delete_preset(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    state.config.update(|c| {
        c.presets.remove(&name);
        if c.default_preset.as_ref() == Some(&name) {
            c.default_preset = None;
        }
        Ok(())
    })
}

#[derive(serde::Serialize)]
pub struct CreatedToken {
    /// The only time the plain token is available.
    token: String,
    config: Config,
}

#[tauri::command]
pub fn create_token(state: AppStateRef<'_>, name: String) -> Result<CreatedToken, String> {
    let (token, stored) = crate::auth::generate(&name)?;
    let config = state.config.update(|c| {
        c.tokens.push(stored);
        Ok(())
    })?;
    Ok(CreatedToken { token, config })
}

/// Revokes a token; clients using it get 401 from then on.
#[tauri::command]
pub fn revoke_token(state: AppStateRef<'_>, id: String) -> Result<Config, String> {
    state.config.update(|c| {
        c.tokens.retain(|t| t.id != id);
        Ok(())
    })
}

#[tauri::command]
pub fn set_require_token(state: AppStateRef<'_>, required: bool) -> Result<Config, String> {
    state.config.update(|c| {
        c.require_token = required;
        Ok(())
    })
}

/// Replaces the websites allowed to call the API from a browser.
#[tauri::command]
pub fn set_allowed_origins(state: AppStateRef<'_>, origins: Vec<String>) -> Result<Config, String> {
    let mut list: Vec<String> = Vec::new();
    for o in origins.iter().filter(|o| !o.trim().is_empty()) {
        let o = config::normalize_origin(o)?;
        if !list.contains(&o) {
            list.push(o);
        }
    }
    state.config.update(|c| {
        c.allowed_origins = list;
        Ok(())
    })
}

/// Last use of each token since ix.print started, by token id.
#[tauri::command]
pub fn token_usage(state: AppStateRef<'_>) -> std::collections::HashMap<String, crate::auth::TokenUse> {
    state.token_usage.snapshot()
}

#[tauri::command]
pub fn set_history_retention(state: AppStateRef<'_>, days: u32) -> Result<Config, String> {
    let config = state.config.update(|c| {
        c.history_retention_days = days;
        Ok(())
    })?;
    state.history.prune(days);
    Ok(config)
}

/// Writes selected presets (all when `names` is empty) to `path`.
#[tauri::command]
pub fn export_presets(
    state: AppStateRef<'_>,
    path: String,
    names: Option<Vec<String>>,
    include_defaults: bool,
) -> Result<usize, String> {
    let names = names.filter(|n| !n.is_empty());
    let bundle = PresetBundle::from_config(&state.config.get(), names.as_deref(), include_defaults);
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    json_file(&path)?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to save {}: {}", path, e))?;
    Ok(bundle.presets.len())
}

/// Preset files are the only files the window reads or writes by path, so
/// anything else (executables, config of other apps, …) is refused.
fn json_file(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        return Err("Preset file must be a .json file".into());
    }
    Ok(())
}

const MAX_BUNDLE_BYTES: u64 = 50 * 1024 * 1024;

fn read_bundle(path: &str) -> Result<PresetBundle, String> {
    json_file(path)?;
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read {}: {}", path, e))?
        .len();
    if size > MAX_BUNDLE_BYTES {
        return Err("Preset file is too large".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    PresetBundle::parse(&text)
}

#[tauri::command]
pub async fn preview_import(state: AppStateRef<'_>, path: String) -> Result<ImportPreview, String> {
    let bundle = read_bundle(&path)?;
    let config = state.config.get();
    let installed = blocking(|| printer::list_printers().into_iter().map(|p| p.name).collect::<Vec<_>>()).await?;
    Ok(config::preview_import(&bundle, &config, &installed))
}

#[derive(serde::Serialize)]
pub struct ImportOutcome {
    config: Config,
    imported: Vec<String>,
}

#[tauri::command]
pub fn apply_import(state: AppStateRef<'_>, path: String, options: ImportOptions) -> Result<ImportOutcome, String> {
    let bundle = read_bundle(&path)?;
    let mut imported = Vec::new();
    let config = state.config.update(|c| {
        imported = config::apply_import(c, &bundle, &options)?;
        Ok(())
    })?;
    Ok(ImportOutcome { config, imported })
}

#[tauri::command]
pub fn history_days(state: AppStateRef<'_>) -> Vec<HistoryDay> {
    state.history.days()
}

/// One page of a day's history (1-based `page`).
#[tauri::command]
pub fn history_page(
    state: AppStateRef<'_>,
    date: String,
    page: usize,
    per_page: usize,
    failed_only: bool,
) -> Result<HistoryPage, String> {
    state.history.read_page(&date, page, per_page, failed_only)
}

#[tauri::command]
pub fn error_log(state: AppStateRef<'_>) -> Result<Vec<crate::diagnostics::DiagnosticEntry>, String> {
    state.diagnostics.read()
}

#[tauri::command]
pub fn record_error(
    state: AppStateRef<'_>,
    source: String,
    message: String,
    details: Option<String>,
) -> Result<(), String> {
    state.diagnostics.record("error", &source, &message, details.as_deref())
}

#[tauri::command]
pub fn clear_error_log(state: AppStateRef<'_>) -> Result<(), String> {
    state.diagnostics.clear()
}

#[tauri::command]
pub fn open_system_logs() -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("mmc.exe")
            .arg("eventvwr.msc")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open Windows Event Viewer: {}", e))
    }
    #[cfg(not(windows))]
    {
        Err("Windows Event Viewer is only available on Windows".into())
    }
}

/// Prints `pages` edge-to-edge calibration pages at the sheet size of the preset/printer.
#[tauri::command]
pub async fn test_print(
    state: AppStateRef<'_>,
    printer: Option<String>,
    preset: Option<String>,
    pages: Option<u32>,
) -> Result<JobResult, String> {
    let state = state.inner().clone();
    let request = JobRequest {
        document: Document::TestPage {
            pages: pages.unwrap_or(1),
        },
        file_name: "ix.print test page.pdf".into(),
        settings: PrintSettings {
            printer,
            ..Default::default()
        },
        preset: preset.filter(|p| !p.is_empty()),
        source: "app",
        client: None,
        token: None,
        hop: false,
    };
    blocking(move || job::run(&state, request))
        .await?
        .map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod desktop_print_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn mock_endpoint() -> (u16, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 8192];
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..split]).to_ascii_lowercase();
                let body = &request[split + 4..];
                let complete = if let Some(len) = headers.lines().find_map(|line| {
                    line.strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                }) {
                    body.len() >= len
                } else {
                    body.windows(5).any(|w| w == b"0\r\n\r\n")
                };
                if complete {
                    break;
                }
            }
            let body = br#"{"job_id":"mock-job","printer":"Mock printer","sheets":3}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            request
        });
        (port, server)
    }

    fn mock_state(port: u16) -> AppState {
        let dir = std::env::temp_dir().join(format!("ixprint-app-post-{}", uuid::Uuid::new_v4()));
        let config = crate::config::ConfigStore::load(&dir);
        config
            .update(|c| {
                c.server_port = port;
                Ok(())
            })
            .unwrap();
        AppState {
            config,
            desktop_key: "private-test-key".into(),
            remotes: crate::remote::RemoteStore::load(&dir),
            remote_cache: Default::default(),
            server_error: Default::default(),
            server_handle: Default::default(),
            token_usage: Default::default(),
            failed_auth: Default::default(),
            history: crate::history::History::new(dir.join("history")),
            diagnostics: crate::diagnostics::Diagnostics::new(&dir),
            jobs_dir: dir.join("jobs"),
        }
    }

    fn headers_and_body(request: &[u8]) -> (String, String) {
        let split = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (
            String::from_utf8_lossy(&request[..split]).to_string(),
            String::from_utf8_lossy(&request[split + 4..]).to_string(),
        )
    }

    #[test]
    fn desktop_print_posts_json_and_pdf_with_requested_copies() {
        for copies in [1, 4] {
            let (port, server) = mock_endpoint();
            let state = mock_state(port);
            let body = format!(
                r#"{{"preset":"label","data":{{"name":"Example"}},"settings":{{"copies":{}}}}}"#,
                copies
            );
            let result = submit_to_print_endpoint(&state, Some(body), None, None, PrintSettings::default()).unwrap();
            let (headers, payload) = headers_and_body(&server.join().unwrap());
            assert_eq!(result.job_id, "mock-job");
            assert_eq!(result.sheets, 3);
            assert!(headers.starts_with("POST /print HTTP/1.1"));
            assert!(headers.to_ascii_lowercase().contains("content-type: application/json"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("x-ixprint-desktop: private-test-key"));
            assert!(payload.contains(&format!("\"copies\":{}", copies)));
        }

        let dir = std::env::temp_dir().join(format!("ixprint-pdf-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("source.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\nmock document").unwrap();
        for copies in [1, 4] {
            let (port, server) = mock_endpoint();
            let state = mock_state(port);
            let settings = PrintSettings {
                copies: Some(copies),
                printer: Some("Mock Printer / 1".into()),
                ..Default::default()
            };
            let result = submit_to_print_endpoint(
                &state,
                None,
                Some(pdf.to_string_lossy().into_owned()),
                Some("receipt".into()),
                settings,
            )
            .unwrap();
            let (headers, payload) = headers_and_body(&server.join().unwrap());
            assert_eq!(result.job_id, "mock-job");
            assert!(headers.starts_with("POST /print?"));
            assert!(headers.contains("preset=receipt"));
            assert!(headers.contains(&format!("copies={}", copies)));
            assert!(headers.contains("printer=Mock%20Printer%20%2F%201"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("content-type: multipart/form-data; boundary="));
            assert!(headers
                .to_ascii_lowercase()
                .contains("x-ixprint-desktop: private-test-key"));
            assert!(payload.contains("filename=\"source.pdf\""));
            assert!(payload.contains("%PDF-1.4"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod preview_pdf_tests {
    use super::*;

    fn mock_state(dir: &std::path::Path) -> AppState {
        std::fs::create_dir_all(dir.join("jobs")).unwrap();
        AppState {
            config: crate::config::ConfigStore::load(dir),
            desktop_key: "test-desktop-key".into(),
            remotes: crate::remote::RemoteStore::load(dir),
            remote_cache: Default::default(),
            server_error: Default::default(),
            server_handle: Default::default(),
            token_usage: Default::default(),
            failed_auth: Default::default(),
            history: crate::history::History::new(dir.join("history")),
            diagnostics: crate::diagnostics::Diagnostics::new(dir),
            jobs_dir: dir.join("jobs"),
        }
    }

    /// The preview PNG must be the same sheet size `layout::build` would print,
    /// scaled by the design's own width/height — i.e. portrait vs. landscape
    /// tracks the requested orientation.
    #[test]
    fn preview_matches_requested_orientation() {
        crate::layout::tests::init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-preview-{}", uuid::Uuid::new_v4()));
        let state = mock_state(&dir);
        let pdf = dir.join("source.pdf");
        crate::testpage::generate(&pdf, 210.0, 297.0, &["test".into()], 1).unwrap();

        let settings = PrintSettings {
            paper: Some("A4".parse().unwrap()),
            orientation: Some("landscape".parse().unwrap()),
            ..Default::default()
        };
        let url = render_pdf_sheet_preview(&state, pdf.to_str().unwrap(), None, settings).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &url["data:image/png;base64,".len()..],
        )
        .unwrap();
        let mut decoder = png::Decoder::new(png.as_slice());
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let reader = decoder.read_info().unwrap();
        let (w, h) = (reader.info().width, reader.info().height);
        assert!(w > h, "landscape A4 preview should be wider than tall, got {}x{}", w, h);

        // No PDF extension: rejected before touching PDFium or the filesystem.
        let err = render_pdf_sheet_preview(&state, "source.txt", None, PrintSettings::default()).unwrap_err();
        assert!(err.contains(".pdf"));

        // Unknown preset: rejected before rendering.
        let err = render_pdf_sheet_preview(
            &state,
            pdf.to_str().unwrap(),
            Some("nope".into()),
            PrintSettings::default(),
        )
        .unwrap_err();
        assert!(err.contains("was not found"));

        // Temp file cleaned up either way.
        assert_eq!(std::fs::read_dir(dir.join("jobs")).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
