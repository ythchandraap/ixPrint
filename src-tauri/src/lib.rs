mod auth;
mod callback;
mod canvas;
mod commands;
mod config;
mod diagnostics;
mod history;
mod job;
mod layout;
mod printer;
mod remote;
mod server;
mod settings;
mod template;
mod testpage;

use config::ConfigStore;
use history::History;
use job::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[cfg(desktop)]
fn focus_main(app: &tauri::AppHandle, tab: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(tab) = tab {
        let _ = app.emit("tray-navigate", tab);
    }
}

#[cfg(desktop)]
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri_plugin_autostart::ManagerExt;

    let state = app.state::<Arc<AppState>>();
    let bridge_item = CheckMenuItem::with_id(
        app,
        "bridge-toggle",
		"Application bridge",
        true,
        state.config.get().bridge_enabled,
        None::<&str>,
    )?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart-toggle",
		"Run at login",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
	let printers_item = MenuItem::with_id(app, "view-printers", "View printers", true, None::<&str>)?;
    let history_item = MenuItem::with_id(app, "view-history", "View history log", true, None::<&str>)?;
	let errors_item = MenuItem::with_id(app, "view-errors", "View error log", true, None::<&str>)?;
	let open_item = MenuItem::with_id(app, "open", "Open ix.print", true, None::<&str>)?;
	let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &bridge_item,
            &autostart_item,
            &separator,
            &printers_item,
            &history_item,
			&errors_item,
            &open_item,
            &separator2,
            &quit_item,
        ],
    )?;

    let bridge_for_event = bridge_item.clone();
    let autostart_for_event = autostart_item.clone();
    let mut tray = TrayIconBuilder::new();
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "bridge-toggle" => {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let enabled = !state.config.get().bridge_enabled;
                let bridge_item = bridge_for_event.clone();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = crate::commands::set_bridge_enabled_inner(state, enabled).await;
                    let _ = bridge_item.set_checked(if result.is_ok() { enabled } else { !enabled });
                    if result.is_ok() {
                        let _ = app.emit("bridge-changed", enabled);
                    }
                });
            }
            "autostart-toggle" => {
                let manager = app.autolaunch();
                let enabled = manager.is_enabled().unwrap_or(false);
                let result = if enabled { manager.disable() } else { manager.enable() };
                let _ = autostart_for_event.set_checked(if result.is_ok() { !enabled } else { enabled });
            }
            "view-printers" => focus_main(app, Some("printers")),
            "view-history" => focus_main(app, Some("history")),
			"view-errors" => focus_main(app, Some("errors")),
            "open" => focus_main(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Directories that may hold the bundled PDFium library, most specific first.
fn pdfium_dirs(app: &tauri::App) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("pdfium").join("lib"));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        dirs.push(exe_dir);
    }
    if cfg!(debug_assertions) {
        dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdfium").join("lib"));
    }
    dirs
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        None,
    ));

    builder
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            // Config (token hashes, presets), history and print jobs: owner only.
            std::fs::create_dir_all(&data_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let diagnostics = diagnostics::Diagnostics::new(&data_dir);
            diagnostics.start_session();
            diagnostics.install_panic_hook();
            if let Err(e) = layout::init(&pdfium_dirs(app)) {
                let _ = diagnostics.record("error", "pdf-engine", &e, None);
                eprintln!("{}", e);
            }

            let jobs_dir = data_dir.join("jobs");
            // Leftovers from a crash may contain documents; don't keep them around.
            let _ = std::fs::remove_dir_all(&jobs_dir);
            std::fs::create_dir_all(&jobs_dir)?;

            let config = ConfigStore::load(&data_dir);
            let remotes = remote::RemoteStore::load(&data_dir);
            let history = History::new(data_dir.join("history"));
            history.prune(config.get().history_retention_days);

            let state = Arc::new(AppState {
                config,
                desktop_key: uuid::Uuid::new_v4().to_string(),
                remotes,
                remote_cache: Default::default(),
                server_error: Default::default(),
                server_handle: Default::default(),
                token_usage: Default::default(),
                failed_auth: Default::default(),
                history,
                diagnostics,
                jobs_dir,
            });
            app.manage(state.clone());

            if state.config.get().bridge_enabled {
                server::spawn(state.clone());
            }
            #[cfg(desktop)]
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::this_local_ip,
            commands::open_docs,
            commands::data_folder_path,
            commands::open_data_folder,
            commands::os_detected,
            commands::engine_status,
            commands::server_status,
            commands::bridge_enabled,
            commands::set_bridge_enabled,
            commands::set_server_port,
            commands::remote_list,
            commands::remote_add,
            commands::remote_update,
            commands::remote_remove,
            commands::remote_test,
            commands::get_printers,
            commands::get_capabilities,
            commands::printer_devices,
            commands::printer_drivers,
            commands::printer_details,
            commands::printer_add,
            commands::printer_update,
            commands::printer_remove,
            commands::printer_set_os_default,
            commands::get_queue,
            commands::job_status,
            commands::app_print,
            commands::preview_pdf_sheet,
            commands::get_config,
            commands::set_default_printer,
            commands::set_default_preset,
            commands::save_preset,
            commands::delete_preset,
            commands::preview_template,
            commands::preview_print,
            commands::set_history_retention,
            commands::create_token,
            commands::revoke_token,
            commands::set_require_token,
            commands::set_allowed_origins,
            commands::token_usage,
            commands::export_presets,
            commands::preview_import,
            commands::apply_import,
            commands::history_days,
            commands::history_page,
            commands::error_log,
            commands::record_error,
            commands::clear_error_log,
            commands::open_system_logs,
            commands::test_print,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<Arc<AppState>>();
                state.inner().diagnostics.finish_session();
            }
        });
}
