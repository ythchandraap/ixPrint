use super::super::AppStateRef;
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
    crate::infrastructure::os::local_ip()
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
    match state.server.last_error() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
