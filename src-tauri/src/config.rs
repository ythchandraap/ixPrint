//! Per-computer configuration (default printer, presets with optional designs), stored as JSON in the
//! app data directory. Kept in memory; every change is written atomically
//! (temp file + rename) and the previous version is kept as `config.json.bak`.

use crate::auth::ApiToken;
use crate::settings::PrintSettings;
use crate::template::Template;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 2: templates became the optional `design` of a preset.
pub const CONFIG_VERSION: u32 = 2;
pub const BUNDLE_FORMAT: &str = "ixprint-presets";
const MAX_PRESET_NAME: usize = 64;
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

/// A preset: print settings plus an optional WYSIWYG design. With a design,
/// clients send only data (JSON) and the paper follows the design's size.
///
/// Stored flat — `{"paper": "A4", …, "design": {…}}` — so presets without a
/// design keep their old shape. Unknown settings are still rejected.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub settings: PrintSettings,
    pub design: Option<Template>,
}

impl Preset {
    pub fn validate(&self) -> Result<(), String> {
        self.settings.validate()?;
        if let Some(d) = &self.design {
            d.validate().map_err(|e| format!("design: {}", e))?;
        }
        Ok(())
    }
}

impl From<PrintSettings> for Preset {
    fn from(settings: PrintSettings) -> Self {
        Preset { settings, design: None }
    }
}

impl Serialize for Preset {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut v = serde_json::to_value(&self.settings).map_err(serde::ser::Error::custom)?;
        if let (Some(obj), Some(design)) = (v.as_object_mut(), &self.design) {
            obj.insert(
                "design".into(),
                serde_json::to_value(design).map_err(serde::ser::Error::custom)?,
            );
        }
        v.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Preset {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut obj = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let design = match obj.remove("design") {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => Some(serde_json::from_value(v).map_err(|e| D::Error::custom(format!("design: {}", e)))?),
        };
        let settings = serde_json::from_value(serde_json::Value::Object(obj)).map_err(D::Error::custom)?;
        Ok(Preset { settings, design })
    }
}

/// A v1 template: a design that pointed at a preset for its printer settings.
#[derive(Deserialize, Clone, Debug)]
pub struct LegacyTemplate {
    #[serde(flatten)]
    design: Template,
    #[serde(default)]
    preset: Option<String>,
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
            tokens: Vec::new(),
            templates: BTreeMap::new(),
        }
    }
}

impl Config {
    fn validate(&self) -> Result<(), String> {
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

pub fn validate_preset_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Preset name cannot be empty".into());
    }
    if name.trim() != name {
        return Err(format!("Preset name '{}' starts or ends with whitespace", name));
    }
    if name.chars().count() > MAX_PRESET_NAME {
        return Err(format!("Preset name must be at most {} characters", MAX_PRESET_NAME));
    }
    Ok(())
}

pub struct ConfigStore {
    path: PathBuf,
    data: RwLock<Config>,
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

impl ConfigStore {
    /// Loads `config.json` from `dir`, falling back to the backup and then to an
    /// empty config. An unreadable file is kept aside instead of being overwritten.
    pub fn load(dir: &Path) -> ConfigStore {
        let path = dir.join("config.json");
        let backup = path.with_extension("json.bak");
        let mut config = match read_config(&path) {
            Ok(Some(cfg)) => cfg,
            Ok(None) => Config::default(),
            Err(e) => {
                eprintln!("config.json is corrupted ({}), trying the backup", e);
                let aside =
                    path.with_extension(format!("json.corrupt-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
                let _ = fs::rename(&path, aside);
                read_config(&backup).ok().flatten().unwrap_or_default()
            }
        };
        if config.migrate() {
            config.version = CONFIG_VERSION;
            match write_atomic(&path, &config) {
                Ok(()) => eprintln!("[ix.print] old template moved to preset (design)"),
                Err(e) => eprintln!("[ix.print] template migration was not saved: {}", e),
            }
        }
        ConfigStore {
            path,
            data: RwLock::new(config),
        }
    }

    pub fn get(&self) -> Config {
        self.data.read().unwrap().clone()
    }

    /// Applies `change` to a copy, validates it, persists it and only then makes it current.
    pub fn update<F>(&self, change: F) -> Result<Config, String>
    where
        F: FnOnce(&mut Config) -> Result<(), String>,
    {
        let mut guard = self.data.write().unwrap();
        let mut next = guard.clone();
        change(&mut next)?;
        next.version = CONFIG_VERSION;
        next.validate()?;
        write_atomic(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

fn read_config(path: &Path) -> Result<Option<Config>, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let cfg: Config = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if cfg.version > CONFIG_VERSION {
                return Err(format!(
                    "Created by a newer version of ix.print (version {})",
                    cfg.version
                ));
            }
            Ok(Some(cfg))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn write_atomic(path: &Path, config: &Config) -> Result<(), String> {
    let dir = path.parent().ok_or("invalid config path")?;
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create the config folder: {}", e))?;
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to save config: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    if path.exists() {
        let _ = fs::copy(path, path.with_extension("json.bak"));
    }
    fs::rename(&tmp, path).map_err(|e| format!("Failed to save config: {}", e))
}

// ---------------------------------------------------------------------------
// Import / export

/// The file format for exported presets.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PresetBundle {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_printer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_preset: Option<String>,
    pub presets: BTreeMap<String, Preset>,
}

impl PresetBundle {
    /// `names = None` exports every preset.
    pub fn from_config(config: &Config, names: Option<&[String]>, include_defaults: bool) -> PresetBundle {
        let presets = config
            .presets
            .iter()
            .filter(|(n, _)| names.is_none_or(|names| names.contains(n)))
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect::<BTreeMap<_, _>>();
        PresetBundle {
            format: BUNDLE_FORMAT.into(),
            version: CONFIG_VERSION,
            exported_at: chrono::Local::now().to_rfc3339(),
            default_printer: include_defaults.then(|| config.default_printer.clone()).flatten(),
            default_preset: include_defaults
                .then(|| config.default_preset.clone())
                .flatten()
                .filter(|p| presets.contains_key(p)),
            presets,
        }
    }

    pub fn parse(text: &str) -> Result<PresetBundle, String> {
        let bundle: PresetBundle = serde_json::from_str(text).map_err(|e| format!("Preset file is invalid: {}", e))?;
        if bundle.format != BUNDLE_FORMAT {
            return Err(format!("Not an ix.print preset file (format '{}')", bundle.format));
        }
        if bundle.version > CONFIG_VERSION {
            return Err(format!(
                "File was created by a newer version of ix.print (version {})",
                bundle.version
            ));
        }
        for (name, preset) in &bundle.presets {
            validate_preset_name(name)?;
            preset.validate().map_err(|e| format!("Preset '{}': {}", name, e))?;
        }
        Ok(bundle)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ImportItem {
    pub name: String,
    pub settings: Preset,
    /// A preset with this name already exists on this computer.
    pub exists: bool,
    /// The preset names a printer that is not installed here.
    pub printer_missing: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ImportPreview {
    pub items: Vec<ImportItem>,
    pub default_printer: Option<String>,
    pub default_printer_missing: bool,
    pub default_preset: Option<String>,
}

pub fn preview_import(bundle: &PresetBundle, config: &Config, installed: &[String]) -> ImportPreview {
    let missing = |p: &Option<String>| p.as_ref().is_some_and(|p| !installed.contains(p));
    ImportPreview {
        items: bundle
            .presets
            .iter()
            .map(|(name, settings)| ImportItem {
                name: name.clone(),
                settings: settings.clone(),
                exists: config.presets.contains_key(name),
                printer_missing: missing(&settings.settings.printer),
            })
            .collect(),
        default_printer: bundle.default_printer.clone(),
        default_printer_missing: missing(&bundle.default_printer),
        default_preset: bundle.default_preset.clone(),
    }
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    Overwrite,
    Skip,
    Rename,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ImportDecision {
    pub name: String,
    pub action: ImportAction,
    /// For `Rename`; generated ("name (2)") when absent.
    #[serde(default)]
    pub new_name: Option<String>,
    /// Replaces the preset's printer: `Some("")` clears it so the default printer is used.
    #[serde(default)]
    pub printer: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ImportOptions {
    /// Remove all existing presets first.
    #[serde(default)]
    pub replace_all: bool,
    /// Also take over the default printer / default preset from the file.
    #[serde(default)]
    pub include_defaults: bool,
    /// Presets without a decision are imported with `Overwrite` if new, `Skip` if they exist.
    #[serde(default)]
    pub decisions: Vec<ImportDecision>,
}

/// Merges `bundle` into `config` according to `options`. Returns the names imported.
pub fn apply_import(
    config: &mut Config,
    bundle: &PresetBundle,
    options: &ImportOptions,
) -> Result<Vec<String>, String> {
    if options.replace_all {
        config.presets.clear();
        config.default_preset = None;
    }

    let mut imported = Vec::new();
    let mut renamed = BTreeMap::new();
    for (name, settings) in &bundle.presets {
        let decision = options.decisions.iter().find(|d| &d.name == name);
        let exists = config.presets.contains_key(name);
        let action = decision.map(|d| d.action).unwrap_or(if exists {
            ImportAction::Skip
        } else {
            ImportAction::Overwrite
        });

        let target = match action {
            ImportAction::Skip => continue,
            ImportAction::Overwrite => name.clone(),
            ImportAction::Rename => match decision.and_then(|d| d.new_name.clone()) {
                Some(n) => {
                    validate_preset_name(&n)?;
                    if config.presets.contains_key(&n) {
                        return Err(format!("Preset '{}' already exists", n));
                    }
                    n
                }
                None => unique_name(config, name),
            },
        };

        let mut settings = settings.clone();
        if let Some(printer) = decision.and_then(|d| d.printer.clone()) {
            settings.settings.printer = Some(printer).filter(|p| !p.trim().is_empty());
        }
        if &target != name {
            renamed.insert(name.clone(), target.clone());
        }
        config.presets.insert(target.clone(), settings);
        imported.push(target);
    }

    if options.include_defaults {
        if let Some(p) = &bundle.default_printer {
            config.default_printer = Some(p.clone());
        }
        if let Some(p) = &bundle.default_preset {
            let p = renamed.get(p).unwrap_or(p);
            if imported.contains(p) {
                config.default_preset = Some(p.clone());
            }
        }
    }
    Ok(imported)
}

fn unique_name(config: &Config, base: &str) -> String {
    (2..)
        .map(|i| format!("{} ({})", base, i))
        .find(|n| !config.presets.contains_key(n))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Paper;

    fn preset(printer: &str) -> Preset {
        Preset::from(PrintSettings {
            printer: Some(printer.into()),
            paper: Some(Paper::Standard("A4")),
            ..Default::default()
        })
    }

    #[test]
    fn origins_and_secure_defaults() {
        assert_eq!(
            normalize_origin(" https://SIMRS.Contoh.id/ ").unwrap(),
            "https://simrs.contoh.id"
        );
        assert_eq!(
            normalize_origin("http://192.168.1.20:8080").unwrap(),
            "http://192.168.1.20:8080"
        );
        assert_eq!(normalize_origin("*").unwrap(), "*");
        for bad in [
            "simrs.id",
            "https://",
            "https://a.id/path",
            "ftp://a.id",
            "https://a b",
            "https://u@a.id",
        ] {
            assert!(normalize_origin(bad).is_err(), "{}", bad);
        }
        // New config: token required, no website allowed.
        let fresh = Config::default();
        assert!(fresh.require_token && fresh.allowed_origins.is_empty());
        assert_eq!(fresh.server_port, DEFAULT_SERVER_PORT);
        // Saved before origins existed: every website stays allowed, visibly.
        let old: Config = serde_json::from_str(r#"{"version":2,"require_token":false}"#).unwrap();
        assert!(!old.require_token);
        assert_eq!(old.allowed_origins, vec!["*"]);
        assert_eq!(old.server_port, DEFAULT_SERVER_PORT);
        let mut c = Config {
            allowed_origins: vec!["https://A.id".into()],
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.allowed_origins = Vec::new();
        c.server_port = 1023;
        assert!(c.validate().is_err());
    }

    #[test]
    fn export_import_roundtrip() {
        let mut src = Config::default();
        src.presets.insert("struk".into(), preset("EPSON"));
        src.presets.insert("laporan".into(), preset("HP"));
        src.default_preset = Some("struk".into());
        src.default_printer = Some("HP".into());
        let text = serde_json::to_string(&PresetBundle::from_config(&src, None, true)).unwrap();
        let bundle = PresetBundle::parse(&text).unwrap();

        let mut dst = Config::default();
        dst.presets.insert("struk".into(), preset("OLD"));
        let preview = preview_import(&bundle, &dst, &["HP".to_string()]);
        let struk = preview.items.iter().find(|i| i.name == "struk").unwrap();
        assert!(struk.exists && struk.printer_missing);

        let options = ImportOptions {
            replace_all: false,
            include_defaults: true,
            decisions: vec![ImportDecision {
                name: "struk".into(),
                action: ImportAction::Rename,
                new_name: None,
                printer: Some("LOCAL-EPSON".into()),
            }],
        };
        let imported = apply_import(&mut dst, &bundle, &options).unwrap();
        assert_eq!(imported, vec!["laporan".to_string(), "struk (2)".to_string()]);
        assert_eq!(dst.presets["struk"].settings.printer.as_deref(), Some("OLD"));
        assert_eq!(
            dst.presets["struk (2)"].settings.printer.as_deref(),
            Some("LOCAL-EPSON")
        );
        assert_eq!(dst.default_preset.as_deref(), Some("struk (2)"));
        assert_eq!(dst.default_printer.as_deref(), Some("HP"));
        dst.validate().unwrap();
    }

    #[test]
    fn preset_json_shape_and_migration() {
        // Old presets (no design) keep their shape; unknown keys are still rejected.
        let p: Preset = serde_json::from_str(r#"{"paper":"A4","copies":2}"#).unwrap();
        assert!(p.design.is_none());
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"copies":2,"paper":"A4"}"#);
        assert!(serde_json::from_str::<Preset>(r#"{"papr":"A4"}"#).is_err());
        let with: Preset =
            serde_json::from_str(r#"{"printer":"Z","design":{"width_mm":60,"height_mm":40,"elements":[]}}"#).unwrap();
        assert_eq!(with.design.as_ref().unwrap().width_mm, 60.0);
        assert!(serde_json::to_string(&with).unwrap().contains("\"design\""));

        // v1 config: templates pointing at presets become presets with a design.
        let mut cfg: Config = serde_json::from_str(
            r#"{"version":1,
                "presets":{"Struck Zebra":{"printer":"Zebra","offset_x":1.5},"label":{"paper":"A4"}},
                "templates":{
                  "label":{"width_mm":60,"height_mm":40,"preset":"Struck Zebra","elements":[],"sample":{"nama":"A"}},
                  "kartu":{"width_mm":85,"height_mm":54,"elements":[]}}}"#,
        )
        .unwrap();
        assert!(cfg.migrate());
        assert!(cfg.templates.is_empty());
        let label = &cfg.presets["label (design)"];
        assert_eq!(label.settings.printer.as_deref(), Some("Zebra"));
        assert_eq!(label.settings.offset_x, Some(1.5));
        assert_eq!(label.design.as_ref().unwrap().sample["nama"], "A");
        assert!(cfg.presets["label"].design.is_none());
        assert!(cfg.presets["kartu"].design.is_some());
        cfg.validate().unwrap();
        assert!(!cfg.migrate());
        assert!(!serde_json::to_string(&cfg).unwrap().contains("templates"));
    }

    #[test]
    fn rejects_foreign_files() {
        assert!(PresetBundle::parse(r#"{"format":"other","version":1,"presets":{}}"#).is_err());
        assert!(
            PresetBundle::parse(r#"{"format":"ixprint-presets","version":1,"presets":{"a":{"paper":"A99"}}}"#).is_err()
        );
    }

    #[test]
    fn store_persists_and_keeps_backup() {
        let dir = std::env::temp_dir().join(format!("ixprint-test-{}", uuid::Uuid::new_v4()));
        let store = ConfigStore::load(&dir);
        store
            .update(|c| {
                c.presets.insert("a".into(), preset("X"));
                Ok(())
            })
            .unwrap();
        store
            .update(|c| {
                c.default_preset = Some("a".into());
                Ok(())
            })
            .unwrap();
        assert!(store
            .update(|c| {
                c.default_preset = Some("nope".into());
                Ok(())
            })
            .is_err());
        assert_eq!(ConfigStore::load(&dir).get().default_preset.as_deref(), Some("a"));
        assert!(dir.join("config.json.bak").exists());
        fs::write(dir.join("config.json"), "{broken").unwrap();
        let recovered = ConfigStore::load(&dir).get();
        assert!(recovered.presets.contains_key("a"));
        let _ = fs::remove_dir_all(dir);
    }
}
