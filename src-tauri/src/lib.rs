pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interfaces;
pub mod shared;

use application::ports::IdempotencyRepository;
use application::state::AppState;
use infrastructure::net::{callback_client::CallbackClient, remote_client::RemoteClient};
use infrastructure::os::SystemClock;
use infrastructure::pdf::PdfEngineAdapter;
use infrastructure::persistence::{
    api_access_log::ApiAccessLog, config_store::ConfigStore, diagnostics_store::Diagnostics, history_store::History,
    idempotency_store::IdempotencyStore, incoming_store::IncomingStore, remote_store::RemoteStore,
};
use infrastructure::printing::{admin::LocalPrinterAdmin, LocalPrinterGateway};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

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

    interfaces::desktop::register_commands(
        builder
            .plugin(tauri_plugin_os::init())
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let data_dir = app.path().app_data_dir()?;
                // Config, print history, API access logs and temporary jobs: owner only.
                std::fs::create_dir_all(&data_dir)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
                }
                let diagnostics = Diagnostics::new(&data_dir);
                diagnostics.start_session();
                diagnostics.install_panic_hook();
                if let Err(e) = infrastructure::pdf::layout_engine::init(&pdfium_dirs(app)) {
                    let _ = diagnostics.record("error", "pdf-engine", &e, None);
                    eprintln!("{}", e);
                }

                let jobs_dir = data_dir.join("jobs");
                // Leftovers from a crash may contain documents; don't keep them around.
                let _ = std::fs::remove_dir_all(&jobs_dir);
                std::fs::create_dir_all(&jobs_dir)?;

                let config = ConfigStore::load(&data_dir);
                let remotes = RemoteStore::load(&data_dir);
                let history = History::with_content_dir(data_dir.join("history"), data_dir.join("history-content"));
                let idempotency = IdempotencyStore::new(data_dir.join("idempotency"));
                idempotency.prune();
                if let Err(error) = idempotency.recover(&history) {
                    let _ = diagnostics.record(
                        "error",
                        "idempotency",
                        "Could not recover print operation states",
                        Some(&error),
                    );
                }
                let incoming_documents = IncomingStore::new(data_dir.join("incoming-documents"));

                let state = Arc::new(AppState {
                    config: Arc::new(config),
                    desktop_key: uuid::Uuid::new_v4().to_string(),
                    remotes: Arc::new(remotes),
                    remote_cache: Mutex::new(Default::default()),
                    server: Arc::new(interfaces::http::HttpServerControl::default()),
                    token_usage: Default::default(),
                    in_flight: Default::default(),
                    failed_auth: Default::default(),
                    history: Arc::new(history),
                    idempotency: Arc::new(idempotency),
                    incoming_documents: Arc::new(incoming_documents),
                    diagnostics: Arc::new(diagnostics),
                    api_access_log: Arc::new(ApiAccessLog::new(&data_dir)),
                    printer_gateway: Arc::new(LocalPrinterGateway),
                    printer_admin: Arc::new(LocalPrinterAdmin),
                    pdf_engine: Arc::new(PdfEngineAdapter),
                    remote_gateway: Arc::new(RemoteClient),
                    callback_sender: Arc::new(CallbackClient),
                    clock: Arc::new(SystemClock),
                    jobs_dir,
                });
                application::history_service::prune_current_retention(&state);
                app.manage(state.clone());

                let callback_state = state.clone();
                std::thread::spawn(move || application::callback_service::resume_pending(callback_state));

                let retention_state = state.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60 * 60));
                    application::history_service::prune_current_retention(&retention_state);
                });

                if state.config.get().bridge_enabled {
                    interfaces::http::spawn(state.clone());
                }
                #[cfg(desktop)]
                interfaces::desktop::tray::setup_tray(app)?;
                Ok(())
            }),
    )
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
