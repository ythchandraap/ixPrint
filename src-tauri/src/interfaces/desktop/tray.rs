use crate::application::state::AppState;
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
pub(crate) fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
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
                    let result =
                        crate::interfaces::desktop::commands::bridge::set_bridge_enabled_inner(state, enabled).await;
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
