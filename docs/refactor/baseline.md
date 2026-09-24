# Refactor baseline

- Baseline commit: `4a786ba63acc0f481f1ed5f682f3eb68f8035d80`
- `cargo test --lib` (`src-tauri`): 56 passed, 0 failed, 4 ignored. The sandbox run initially denied two local-operation tests; the authorized rerun passed.
- `bun run build`: passed (`vue-tsc --noEmit && vite build`).

## HTTP routes

| Method | Path |
| --- | --- |
| GET | `/` |
| GET | `/docs` |
| GET | `/list-printer` |
| GET | `/printer-capabilities` |
| GET | `/queue` |
| GET | `/job-status` |
| GET | `/presets` |
| GET | `/presets/export` |
| POST | `/print` |
| POST | `/test-page` |
| GET | `/templates` |
| POST | `/print-template` |

## Tauri commands

`this_local_ip`, `open_docs`, `data_folder_path`, `open_data_folder`, `os_detected`, `engine_status`, `server_status`, `bridge_enabled`, `set_bridge_enabled`, `set_server_port`, `remote_list`, `remote_add`, `remote_update`, `remote_remove`, `remote_test`, `get_printers`, `get_capabilities`, `printer_devices`, `printer_drivers`, `printer_details`, `printer_add`, `printer_update`, `printer_remove`, `printer_set_os_default`, `get_queue`, `job_status`, `app_print`, `preview_pdf_sheet`, `get_config`, `set_default_printer`, `set_default_preset`, `save_preset`, `delete_preset`, `preview_template`, `preview_print`, `set_history_retention`, `create_token`, `revoke_token`, `set_require_token`, `set_allowed_origins`, `token_usage`, `export_presets`, `preview_import`, `apply_import`, `history_days`, `history_page`, `error_log`, `record_error`, `clear_error_log`, `open_system_logs`, `test_print`.
