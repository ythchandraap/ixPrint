pub mod commands;
pub(crate) mod tray;

use crate::application::state::AppState;
use std::sync::Arc;
use tauri::State;

pub(super) type AppStateRef<'a> = State<'a, Arc<AppState>>;

pub(super) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

pub const COMMANDS: &[&str] = &[
    "this_local_ip",
    "open_docs",
    "data_folder_path",
    "open_data_folder",
    "os_detected",
    "engine_status",
    "server_status",
    "bridge_enabled",
    "set_bridge_enabled",
    "set_server_port",
    "remote_list",
    "remote_add",
    "remote_update",
    "remote_remove",
    "remote_test",
    "get_printers",
    "get_capabilities",
    "printer_devices",
    "printer_drivers",
    "printer_details",
    "printer_add",
    "printer_update",
    "printer_remove",
    "printer_set_os_default",
    "get_queue",
    "job_status",
    "app_print",
    "preview_pdf_sheet",
    "get_config",
    "set_default_printer",
    "set_default_preset",
    "set_preset_shared",
    "set_preset_list_endpoint_enabled",
    "get_remote_shared_presets",
    "preview_remote_preset_import",
    "apply_remote_preset_import",
    "set_lan_access",
    "save_preset",
    "save_preset_draft",
    "publish_preset_draft",
    "discard_preset_draft",
    "restore_preset_previous",
    "test_preset_draft",
    "delete_preset",
    "preview_template",
    "preview_print",
    "preview_print_data",
    "set_history_retention",
    "create_token",
    "revoke_token",
    "set_require_token",
    "set_allowed_origins",
    "token_usage",
    "export_presets",
    "preview_import",
    "apply_import",
    "history_days",
    "history_page",
    "search_history",
    "retry_callback",
    "cancel_own_job",
    "error_log",
    "api_access_log",
    "clear_api_access_log",
    "record_error",
    "clear_error_log",
    "open_system_logs",
    "test_print",
];

pub fn register_commands(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        commands::app::this_local_ip,
        commands::app::open_docs,
        commands::app::data_folder_path,
        commands::app::open_data_folder,
        commands::app::os_detected,
        commands::printers::engine_status,
        commands::app::server_status,
        commands::bridge::bridge_enabled,
        commands::bridge::set_bridge_enabled,
        commands::bridge::set_server_port,
        commands::remotes::remote_list,
        commands::remotes::remote_add,
        commands::remotes::remote_update,
        commands::remotes::remote_remove,
        commands::remotes::remote_test,
        commands::printers::get_printers,
        commands::printers::get_capabilities,
        commands::printer_admin::printer_devices,
        commands::printer_admin::printer_drivers,
        commands::printer_admin::printer_details,
        commands::printer_admin::printer_add,
        commands::printer_admin::printer_update,
        commands::printer_admin::printer_remove,
        commands::printers::printer_set_os_default,
        commands::printers::get_queue,
        commands::printers::job_status,
        commands::print::app_print,
        commands::print::preview_pdf_sheet,
        commands::presets::get_config,
        commands::presets::set_default_printer,
        commands::presets::set_default_preset,
        commands::presets::set_preset_shared,
        commands::presets::set_preset_list_endpoint_enabled,
        commands::presets::get_remote_shared_presets,
        commands::presets::preview_remote_preset_import,
        commands::presets::apply_remote_preset_import,
        commands::bridge::set_lan_access,
        commands::presets::save_preset,
        commands::presets::save_preset_draft,
        commands::presets::publish_preset_draft,
        commands::presets::discard_preset_draft,
        commands::presets::restore_preset_previous,
        commands::presets::test_preset_draft,
        commands::presets::delete_preset,
        commands::presets::preview_template,
        commands::presets::preview_print,
        commands::presets::preview_print_data,
        commands::history::set_history_retention,
        commands::tokens::create_token,
        commands::tokens::revoke_token,
        commands::tokens::set_require_token,
        commands::tokens::set_allowed_origins,
        commands::tokens::token_usage,
        commands::presets::export_presets,
        commands::presets::preview_import,
        commands::presets::apply_import,
        commands::history::history_days,
        commands::history::history_page,
        commands::history::search_history,
        commands::history::retry_callback,
        commands::history::cancel_own_job,
        commands::diagnostics::error_log,
        commands::api_access::api_access_log,
        commands::api_access::clear_api_access_log,
        commands::diagnostics::record_error,
        commands::diagnostics::clear_error_log,
        commands::diagnostics::open_system_logs,
        commands::print::test_print,
    ])
}

#[cfg(test)]
mod tests {
    use super::COMMANDS;

    #[test]
    fn all_tauri_commands_registered() {
        let source = include_str!("mod.rs");
        let handler = source
            .split_once("tauri::generate_handler![")
            .expect("generate_handler! registration")
            .1
            .split_once("])")
            .expect("generate_handler! closing delimiter")
            .0;
        let registered: Vec<_> = handler
            .lines()
            .filter_map(|line| {
                let path = line.trim().trim_end_matches(',');
                path.strip_prefix("commands::")?.rsplit("::").next()
            })
            .collect();

        assert_eq!(registered, COMMANDS);
    }
}
