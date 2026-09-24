//! Per-computer configuration and origin validation.

use super::preset::{validate_preset_name, LegacyTemplate, Preset};
use crate::domain::auth::ApiToken;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 2: templates became the optional `design` of a preset.
pub const CONFIG_VERSION: u32 = 2;
pub const DEFAULT_SERVER_PORT: u16 = 18032;

fn default_retention() -> u32 {
    90
}

fn default_true() -> bool {
    true
}

pub fn default_server_port() -> u16 {
    DEFAULT_SERVER_PORT
}

/// Configs saved before origins could be restricted allowed every website;
/// they keep doing so (visibly, as `*`) until changed in the API settings.
fn legacy_origins() -> Vec<String> {
    vec![ANY_ORIGIN.to_string()]
}

/// In `allowed_origins`: any website may call the API from a browser.
pub const ANY_ORIGIN: &str = "*";
const MAX_ORIGINS: usize = 50;

/// `https://App.Example.com/` → `https://app.example.com`. An origin is a
/// scheme, host and optional port only: no path, query or credentials.
pub fn normalize_origin(origin: &str) -> Result<String, String> {
    let o = origin.trim().trim_end_matches('/').to_ascii_lowercase();
    if o == ANY_ORIGIN {
        return Ok(o);
    }
    let bad = || {
        format!(
            "Origin '{}' is invalid: use a format such as https://app.example.com or http://192.168.1.20:8080",
            origin.trim()
        )
    };
    let rest = o
        .strip_prefix("https://")
        .or_else(|| o.strip_prefix("http://"))
        .ok_or_else(bad)?;
    let valid = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']');
    if rest.is_empty() || !rest.chars().all(valid) || rest.starts_with(['.', '-', ':']) {
        return Err(bad());
    }
    Ok(o)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub version: u32,
    /// TCP port used by the local HTTP bridge.
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_printer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_preset: Option<String>,
    #[serde(default)]
    pub presets: BTreeMap<String, Preset>,
    /// Days of print history to keep.
    #[serde(default = "default_retention")]
    pub history_retention_days: u32,
    /// Reject HTTP requests without a valid API token (on by default; with no
    /// tokens created yet, every API request is refused).
    #[serde(default = "default_true")]
    pub require_token: bool,
    /// Websites (origins) allowed to call the API from a browser; `*` = any.
    /// Requests without an `Origin` (servers, curl, other apps) are unaffected.
    #[serde(default = "legacy_origins")]
    pub allowed_origins: Vec<String>,
    /// Whether the local HTTP bridge accepts connections from web apps.
    #[serde(default = "default_true")]
    pub bridge_enabled: bool,
    /// Whether the bridge accepts connections from other computers on the network.
    #[serde(default = "default_true")]
    pub lan_access: bool,
    #[serde(default)]
    pub tokens: Vec<ApiToken>,
    /// Version 1 templates; moved into `presets` when the config is loaded.
    #[serde(default, skip_serializing)]
    pub templates: BTreeMap<String, LegacyTemplate>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
            server_port: default_server_port(),
            default_printer: None,
            default_preset: None,
            presets: BTreeMap::new(),
            history_retention_days: default_retention(),
            require_token: true,
            allowed_origins: Vec::new(),
            bridge_enabled: true,
            lan_access: true,
            tokens: Vec::new(),
            templates: BTreeMap::new(),
        }
    }
}

impl Config {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1024..=u16::MAX).contains(&self.server_port) {
            return Err("Server port must be between 1024 and 65535".into());
        }
        for (name, preset) in &self.presets {
            validate_preset_name(name)?;
            preset.validate().map_err(|e| format!("Preset '{}': {}", name, e))?;
        }
        if let Some(p) = &self.default_preset {
            if !self.presets.contains_key(p) {
                return Err(format!("Default preset '{}' does not exist", p));
            }
        }
        if self.allowed_origins.len() > MAX_ORIGINS {
            return Err(format!("At most {} origins are allowed", MAX_ORIGINS));
        }
        for o in &self.allowed_origins {
            if normalize_origin(o)? != *o {
                return Err(format!("Origin '{}' is not in canonical form", o));
            }
        }
        if !(1..=3650).contains(&self.history_retention_days) {
            return Err("History retention must be 1–3650 days".into());
        }
        Ok(())
    }
}

impl Config {
    /// Moves v1 templates into presets with a design. Each gets the settings of
    /// the preset it pointed at; a name already taken by a preset gets " (desain)".
    /// Returns whether anything changed.
    pub fn migrate(&mut self) -> bool {
        if self.templates.is_empty() {
            return false;
        }
        for (name, legacy) in std::mem::take(&mut self.templates) {
            let settings = legacy
                .preset
                .as_ref()
                .and_then(|p| self.presets.get(p))
                .map(|p| p.settings.clone())
                .unwrap_or_default();
            let target = if self.presets.contains_key(&name) {
                (1..)
                    .map(|i| {
                        if i == 1 {
                            format!("{} (design)", name)
                        } else {
                            format!("{} (design {})", name, i)
                        }
                    })
                    .find(|n| !self.presets.contains_key(n))
                    .unwrap()
            } else {
                name
            };
            self.presets.insert(
                target,
                Preset {
                    settings,
                    design: Some(legacy.design),
                },
            );
        }
        true
    }
}
