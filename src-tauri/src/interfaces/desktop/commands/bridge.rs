use super::super::AppStateRef;
use crate::application::state::AppState;
use crate::domain::settings_config::Config;
use std::sync::Arc;
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
    state.config.update(&mut |c| {
        c.bridge_enabled = enabled;
        Ok(())
    })?;

    if enabled {
        state.server.clear_error();
        if !state.server.is_running() {
            state.server.start(state.clone());
        }
    } else {
        state.server.clear_error();
        state.server.stop().await;
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

    let next = state.config.update(&mut |c| {
        c.server_port = port;
        Ok(())
    })?;

    if next.bridge_enabled {
        state.server.stop().await;
        state.server.clear_error();
        state.server.start(state.clone());
    }
    Ok(next)
}

/// Controls whether other computers on the network can reach the bridge.
#[tauri::command]
pub async fn set_lan_access(state: AppStateRef<'_>, enabled: bool) -> Result<Config, String> {
    let state = state.inner().clone();
    let current = state.config.get();
    if current.lan_access == enabled {
        return Ok(current);
    }

    let next = state.config.update(&mut |c| {
        c.lan_access = enabled;
        Ok(())
    })?;

    if next.bridge_enabled {
        state.server.stop().await;
        state.server.clear_error();
        state.server.start(state.clone());
    }
    Ok(next)
}
