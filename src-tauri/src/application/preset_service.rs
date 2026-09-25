use base64::Engine;

use crate::application::state::AppState;
use crate::domain::preset::{self, Preset};
use crate::domain::settings::{JobSettings, Paper, PrintSettings};
use crate::domain::settings_config::Config;
use crate::domain::template::{Record, Template, MAX_RECORDS};
mod transfer;
pub use transfer::{
    apply_import, apply_remote_import, export_bundle, export_shared_preset, preview_import, preview_remote_import,
    shared_listing, shared_presets,
};

#[cfg(test)]
mod tests;

#[derive(serde::Serialize)]
pub struct PreflightReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub fields: Vec<String>,
    pub required_fields: Vec<String>,
    pub records: usize,
}

pub fn preflight_data(
    state: &AppState,
    preset_name: Option<&str>,
    data: &serde_json::Value,
    header: Option<&serde_json::Value>,
    settings: &PrintSettings,
) -> Result<PreflightReport, String> {
    settings.validate()?;
    let config = state.config.get();
    let name = preset_name
        .or(config.default_preset.as_deref())
        .ok_or("Specify a preset with a design")?;
    let design = config
        .presets
        .get(name)
        .ok_or_else(|| format!("Preset '{}' was not found", name))?
        .design
        .as_ref()
        .ok_or_else(|| format!("Preset '{}' has no design", name))?;
    let header = header
        .map(crate::domain::template::record_from_json)
        .transpose()?
        .unwrap_or_default();
    let records = match data {
        serde_json::Value::Array(items) => {
            if items.len() > MAX_RECORDS {
                return Err(format!("data: at most {} items per request", MAX_RECORDS));
            }
            items
                .iter()
                .map(crate::domain::template::record_from_json)
                .collect::<Result<Vec<_>, _>>()?
        }
        other => vec![crate::domain::template::record_from_json(other)?],
    };
    let errors = design.preflight_errors(&records, &header);
    Ok(PreflightReport {
        valid: errors.is_empty(),
        errors,
        fields: design.fields(),
        required_fields: design.required_fields.clone(),
        records: records.len(),
    })
}

pub fn config(state: &AppState) -> Config {
    state.config.get()
}

pub fn bridge_presets(state: &AppState) -> serde_json::Value {
    let config = state.config.get();
    serde_json::json!({
        "default_printer": config.default_printer,
        "default_preset": config.default_preset,
        "presets": config.presets,
    })
}

pub fn templates_json(state: &AppState) -> serde_json::Value {
    let config = state.config.get();
    serde_json::Value::Array(
        config
            .presets
            .iter()
            .filter_map(|(name, preset)| {
                let design = preset.design.as_ref()?;
                Some(serde_json::json!({
                    "name": name,
                    "preset": name,
                    "width_mm": design.width_mm,
                    "height_mm": design.height_mm,
                    "fields": design.fields(),
                    "required_fields": design.required_fields,
                    "auto_fields": design.auto_fields(),
                    "sample": design.sample,
                }))
            })
            .collect(),
    )
}

pub fn set_default_printer(state: &AppState, printer: Option<String>) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.default_printer = printer.clone().filter(|value| !value.trim().is_empty());
        Ok(())
    })
}

pub fn set_default_preset(state: &AppState, name: Option<String>) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.default_preset = name.clone().filter(|value| !value.is_empty());
        Ok(())
    })
}

pub fn set_shared(state: &AppState, name: String, shared: bool) -> Result<Config, String> {
    preset::validate_preset_name(&name)?;
    state.config.update(&mut |config| {
        if !config.presets.contains_key(&name) {
            return Err(format!("Published preset '{}' was not found", name));
        }
        if shared {
            config.shared_presets.insert(name.clone());
        } else {
            config.shared_presets.remove(&name);
        }
        Ok(())
    })
}

pub fn set_preset_list_endpoint_enabled(state: &AppState, enabled: bool) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.preset_list_endpoint_enabled = enabled;
        Ok(())
    })
}

pub fn save(state: &AppState, name: String, original_name: Option<String>, preset: Preset) -> Result<Config, String> {
    preset::validate_preset_name(&name)?;
    preset.validate()?;
    let mut preset = Some(preset);
    state.config.update(&mut |config| {
        if original_name.is_none() && config.presets.contains_key(&name) {
            return Err(format!("Preset '{}' already exists; edit it as a draft", name));
        }
        if original_name.as_deref() == Some(name.as_str()) && !config.presets.contains_key(&name) {
            return Err(format!("Preset '{}' was not found", name));
        }
        if let Some(original) = original_name.clone().filter(|original| original != &name) {
            if config.presets.contains_key(&name) {
                return Err(format!("Preset '{}' already exists", name));
            }
            if config.presets.remove(&original).is_none() {
                return Err(format!("Preset '{}' was not found", original));
            }
            if let Some(draft) = config.preset_drafts.remove(&original) {
                config.preset_drafts.insert(name.clone(), draft);
            }
            if let Some(previous) = config.preset_previous.remove(&original) {
                config.preset_previous.insert(name.clone(), previous);
            }
            if config.shared_presets.remove(&original) {
                config.shared_presets.insert(name.clone());
            }
            if config.default_preset.as_ref() == Some(&original) {
                config.default_preset = Some(name.clone());
            }
        }
        let Some(preset) = preset.take() else {
            return Err("Preset update was applied more than once".into());
        };
        if let Some(previous) = config.presets.insert(name.clone(), preset) {
            config.preset_previous.insert(name.clone(), previous);
        }
        config.preset_drafts.remove(&name);
        Ok(())
    })
}

/// Store an edit without changing the preset that API clients currently use.
pub fn save_draft(state: &AppState, name: String, preset: Preset) -> Result<Config, String> {
    preset::validate_preset_name(&name)?;
    preset.validate()?;
    state.config.update(&mut |config| {
        if !config.presets.contains_key(&name) {
            return Err(format!("Preset '{}' was not found", name));
        }
        config.preset_drafts.insert(name.clone(), preset.clone());
        Ok(())
    })
}

pub fn publish_draft(state: &AppState, name: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        if !config.presets.contains_key(name) {
            return Err(format!("Preset '{}' was not found", name));
        }
        let draft = config.preset_drafts.remove(name).ok_or("No draft for this preset")?;
        if let Some(previous) = config.presets.insert(name.to_string(), draft) {
            config.preset_previous.insert(name.to_string(), previous);
        }
        Ok(())
    })
}

pub fn discard_draft(state: &AppState, name: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.preset_drafts.remove(name);
        Ok(())
    })
}

pub fn restore_previous(state: &AppState, name: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        if !config.presets.contains_key(name) {
            return Err("Published preset was not found".into());
        }
        let previous = config
            .preset_previous
            .remove(name)
            .ok_or("No previous published version")?;
        let current = config
            .presets
            .insert(name.to_string(), previous)
            .expect("checked above");
        config.preset_previous.insert(name.to_string(), current);
        Ok(())
    })
}

pub fn remove(state: &AppState, name: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.presets.remove(name);
        config.preset_drafts.remove(name);
        config.preset_previous.remove(name);
        config.shared_presets.remove(name);
        if config.default_preset.as_deref() == Some(name) {
            config.default_preset = None;
        }
        Ok(())
    })
}

pub fn preview_print(
    state: &AppState,
    template: &Template,
    rows: usize,
    px_per_mm: Option<f32>,
) -> Result<(Vec<String>, usize), String> {
    let scale = px_per_mm.unwrap_or(4.0).clamp(1.0, 12.0);
    let records = vec![template.sample.clone(); rows.clamp(1, MAX_RECORDS)];
    let (pages, total) = state
        .pdf_engine
        .pages_png(template, &records, &template.sample, 6, scale)?;
    Ok((pages.into_iter().map(data_url).collect(), total))
}

pub fn preview_print_data(
    state: &AppState,
    template: &Template,
    records: &[Record],
    header: &Record,
    px_per_mm: Option<f32>,
) -> Result<(Vec<String>, usize), String> {
    if records.is_empty() || records.len() > MAX_RECORDS {
        return Err(format!("Preview needs 1–{} data items", MAX_RECORDS));
    }
    template.preflight(records, header)?;
    let scale = px_per_mm.unwrap_or(4.0).clamp(1.0, 12.0);
    let (pages, total) = state.pdf_engine.pages_png(template, records, header, 6, scale)?;
    Ok((pages.into_iter().map(data_url).collect(), total))
}

pub fn preview_template(
    state: &AppState,
    template: &Template,
    data: &Record,
    px_per_mm: Option<f32>,
) -> Result<String, String> {
    let scale = px_per_mm.unwrap_or(8.0).clamp(1.0, 24.0);
    Ok(data_url(state.pdf_engine.preview_png(template, data, scale)?))
}

pub fn preview_pdf_sheet(
    state: &AppState,
    pdf_path: &str,
    preset: Option<&str>,
    settings: &PrintSettings,
) -> Result<String, String> {
    let path = std::path::PathBuf::from(pdf_path);
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err("Choose a PDF file with the .pdf extension.".into());
    }
    let config = state.config.get();
    let preset_settings = match preset.filter(|name| !name.trim().is_empty()) {
        Some(name) => config
            .presets
            .get(name)
            .map(|preset| preset.settings.clone())
            .ok_or_else(|| format!("Preset '{}' was not found", name))?,
        None => Default::default(),
    };
    let merged = preset_settings.overlay(settings);
    merged.validate()?;
    let printer_name = merged
        .printer
        .clone()
        .or_else(|| state.default_printer())
        .unwrap_or_default();
    let job = JobSettings::resolve(&merged, printer_name.clone());
    let default_paper = if job.paper == Paper::PrinterDefault && !printer_name.is_empty() {
        state.printer_gateway.default_paper_mm(&printer_name)
    } else {
        (210.0, 297.0)
    };
    let output = state.jobs_dir.join(format!("preview-{}.pdf", uuid::Uuid::new_v4()));
    let result = (|| -> Result<String, String> {
        state.pdf_engine.build_layout(&path, &output, &job, default_paper)?;
        Ok(data_url(state.pdf_engine.render_page_png(&output, 0, 6.0)?))
    })();
    state.pdf_engine.remove_generated_file(&output);
    result
}

fn data_url(png: Vec<u8>) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}
