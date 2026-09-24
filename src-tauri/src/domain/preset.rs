use super::settings_config::{Config, CONFIG_VERSION};
use crate::domain::settings::PrintSettings;
use crate::domain::template::Template;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BUNDLE_FORMAT: &str = "ixprint-presets";
const MAX_PRESET_NAME: usize = 64;

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
    pub(crate) design: Template,
    #[serde(default)]
    pub(crate) preset: Option<String>,
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
