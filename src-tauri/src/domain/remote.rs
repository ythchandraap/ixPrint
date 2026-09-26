use crate::domain::auth;
use crate::domain::job::JobError;
use crate::domain::printer;
use crate::domain::settings::PrintSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Instant;

pub const SEPARATOR: &str = " / ";
pub const HOP_HEADER: &str = "X-IxPrint-Hop";
const MAX_REMOTES: usize = 20;
const MAX_NAME: usize = 32;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub id: String,
    pub name: String,
    pub url: String,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RemoteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub preset: Option<String>,
    pub token_hint: String,
    pub created_at: String,
    pub online: Option<bool>,
    pub last_error: Option<String>,
    pub printers: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SharedPreset {
    pub name: String,
    pub has_design: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SharedPresetListing {
    pub listing_enabled: bool,
    pub presets: Vec<SharedPreset>,
}

pub struct CacheEntry {
    pub fetched: Instant,
    pub printers: Vec<printer::Printer>,
    pub error: Option<String>,
    pub ok_once: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Local(String),
    Remote { remote: Remote, printer: String },
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteError {
    BadRequest(String),
    Failed(String),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadRequest(message) | Self::Failed(message) => message,
        })
    }
}

impl RemoteError {
    pub fn into_job_error(self) -> JobError {
        match self {
            Self::BadRequest(message) => JobError::BadRequest(message),
            Self::Failed(message) => JobError::Failed(message),
        }
    }

    pub fn is_bad_request(&self) -> bool {
        matches!(self, Self::BadRequest(_))
    }
}

#[derive(Deserialize)]
pub struct RemotePrintResult {
    pub job_id: String,
    pub sheets: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReadyPrintResult {
    pub job_id: String,
    pub printer: String,
    pub sheets: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spooler_job: Option<String>,
}

pub(crate) fn validate_all(remotes: &[Remote]) -> Result<(), String> {
    if remotes.len() > MAX_REMOTES {
        return Err(format!("At most {} other ix.print computers are allowed", MAX_REMOTES));
    }
    let mut names = HashSet::new();
    for remote in remotes {
        validate_name(&remote.name)?;
        if !names.insert(remote.name.to_ascii_lowercase()) {
            return Err(format!("Remote name '{}' is already used", remote.name));
        }
        if normalize_url(&remote.url)? != remote.url {
            return Err(format!("Remote '{}' address is not normalised", remote.name));
        }
        validate_token(&remote.token)?;
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed != name
        || trimmed.is_empty()
        || trimmed.chars().count() > MAX_NAME
        || trimmed.contains('/')
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err("Remote name must be 1–32 characters without '/'".into());
    }
    Ok(())
}

pub fn normalize_url(url: &str) -> Result<String, String> {
    let input = url.trim().trim_end_matches('/');
    let bad = || format!("Remote address '{}' is invalid", url.trim());
    if input.is_empty() || input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let Some((scheme, authority)) = input.split_once("://") else {
        return Err(bad());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" || authority.is_empty() || authority.contains(['/', '?', '#', '@']) {
        return Err(bad());
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(bad)?;
        let host = &authority[..=end];
        let rest = &authority[end + 1..];
        let port = if rest.is_empty() {
            None
        } else {
            Some(
                rest.strip_prefix(':')
                    .ok_or_else(bad)?
                    .parse::<u16>()
                    .map_err(|_| bad())?,
            )
        };
        (host, port)
    } else if authority.matches(':').count() > 1 {
        return Err(bad());
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, Some(port.parse::<u16>().map_err(|_| bad())?))
    } else {
        (authority, None)
    };
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || port == Some(0) {
        return Err(bad());
    }
    let authority = match port {
        Some(port) => format!("{}:{}", host.to_ascii_lowercase(), port),
        None if scheme == "http" => format!("{}:18032", host.to_ascii_lowercase()),
        None => host.to_ascii_lowercase(),
    };
    Ok(format!("{}://{}", scheme, authority))
}

pub fn validate_token(token: &str) -> Result<(), String> {
    if token.trim() != token || !token.starts_with(auth::TOKEN_PREFIX) || token.len() > 200 {
        return Err("Remote token must start with fxp_ and be at most 200 characters".into());
    }
    Ok(())
}

pub fn resolve(local: &[String], remotes: &[Remote], name: &str) -> Target {
    if local.iter().any(|local_name| local_name == name) {
        return Target::Local(name.to_string());
    }
    remotes
        .iter()
        .find_map(|remote| {
            let prefix = format!("{}{}", remote.name, SEPARATOR);
            name.strip_prefix(&prefix)
                .filter(|printer| !printer.is_empty())
                .map(|printer| Target::Remote {
                    remote: remote.clone(),
                    printer: printer.to_string(),
                })
        })
        .unwrap_or(Target::Unknown)
}

pub fn settings_query(settings: &PrintSettings) -> Vec<(String, String)> {
    let value = serde_json::to_value(settings).unwrap_or(Value::Null);
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(key, value)| match value {
            Value::String(value) => Some((key.clone(), value.clone())),
            Value::Number(value) => Some((key.clone(), value.to_string())),
            Value::Bool(value) => Some((key.clone(), value.to_string())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::settings::{Margins, Paper, PrintSettings, Scale};

    fn remote(name: &str) -> Remote {
        Remote {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            url: "http://pc2.local:18032".into(),
            token: "fxp_test-token".into(),
            preset: None,
            created_at: chrono::Local::now().to_rfc3339(),
        }
    }

    #[test]
    fn normalizes_urls_and_rejects_paths() {
        assert_eq!(normalize_url("http://192.168.1.7").unwrap(), "http://192.168.1.7:18032");
        assert_eq!(
            normalize_url("HTTP://PC2.local:18032/").unwrap(),
            "http://pc2.local:18032"
        );
        assert_eq!(
            normalize_url("https://print.contoh.id").unwrap(),
            "https://print.contoh.id"
        );
        for value in ["192.168.1.7", "http://a/b", "http://u:p@a", "ftp://a"] {
            assert!(normalize_url(value).is_err(), "{}", value);
        }
    }

    #[test]
    fn validates_and_resolves_names() {
        assert!(validate_name("PC2").is_ok());
        assert!(validate_name("Gudang Lt 2").is_ok());
        for value in ["", " PC2", "PC2 ", "PC/2", &"x".repeat(33)] {
            assert!(validate_name(value).is_err(), "{}", value);
        }
        let r = remote("PC2");
        assert_eq!(
            resolve(&["PC2 / EPSON".into()], std::slice::from_ref(&r), "PC2 / EPSON"),
            Target::Local("PC2 / EPSON".into())
        );
        assert!(
            matches!(resolve(&[], std::slice::from_ref(&r), "PC2 / EPSON"), Target::Remote { printer, .. } if printer == "EPSON")
        );
        assert_eq!(resolve(&[], std::slice::from_ref(&r), "PC2 / "), Target::Unknown);
        assert_eq!(resolve(&[], std::slice::from_ref(&r), "pc2 / EPSON"), Target::Unknown);
    }

    #[test]
    fn settings_round_trip_to_query() {
        let settings = PrintSettings {
            copies: Some(2),
            paper: Some(Paper::Roll { width_mm: 80.0 }),
            offset_x: Some(1.5),
            margins: Some(Margins {
                top: 5.0,
                right: 3.0,
                bottom: 5.0,
                left: 3.0,
            }),
            collate: Some(false),
            scale: Some(Scale::Percent(90.0)),
            ..Default::default()
        };
        let params = settings_query(&settings).into_iter().collect();
        let (parsed, preset) = PrintSettings::from_query(&params).unwrap();
        assert_eq!(parsed, settings);
        assert!(preset.is_none());
    }
}
