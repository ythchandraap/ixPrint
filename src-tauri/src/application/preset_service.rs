use base64::Engine;

use crate::application::state::AppState;
use crate::domain::preset::{self, ImportOptions, ImportPreview, Preset, PresetBundle};
use crate::domain::settings::{JobSettings, Paper, PrintSettings};
use crate::domain::settings_config::Config;
use crate::domain::template::{Record, Template, MAX_RECORDS};

pub fn config(state: &AppState) -> Config {
    state.config.get()
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

pub fn save(state: &AppState, name: String, original_name: Option<String>, preset: Preset) -> Result<Config, String> {
    preset::validate_preset_name(&name)?;
    preset.validate()?;
    let mut preset = Some(preset);
    state.config.update(&mut |config| {
        if let Some(original) = original_name.clone().filter(|original| original != &name) {
            if config.presets.contains_key(&name) {
                return Err(format!("Preset '{}' already exists", name));
            }
            config.presets.remove(&original);
            if config.default_preset.as_ref() == Some(&original) {
                config.default_preset = Some(name.clone());
            }
        }
        let Some(preset) = preset.take() else {
            return Err("Preset update was applied more than once".into());
        };
        config.presets.insert(name.clone(), preset);
        Ok(())
    })
}

pub fn remove(state: &AppState, name: &str) -> Result<Config, String> {
    state.config.update(&mut |config| {
        config.presets.remove(name);
        if config.default_preset.as_deref() == Some(name) {
            config.default_preset = None;
        }
        Ok(())
    })
}

pub fn export_bundle(state: &AppState, names: Option<&[String]>, include_defaults: bool) -> PresetBundle {
    preset::PresetBundle::from_config(&state.config.get(), names, include_defaults)
}

pub fn preview_import(state: &AppState, bundle: &PresetBundle) -> ImportPreview {
    let installed = state
        .printer_gateway
        .list_printers()
        .into_iter()
        .map(|printer| printer.name)
        .collect::<Vec<_>>();
    preset::preview_import(bundle, &state.config.get(), &installed)
}

pub fn apply_import(
    state: &AppState,
    bundle: &PresetBundle,
    options: &ImportOptions,
) -> Result<(Config, Vec<String>), String> {
    let mut imported = Vec::new();
    let config = state.config.update(&mut |config| {
        imported = preset::apply_import(config, bundle, options)?;
        Ok(())
    })?;
    Ok((config, imported))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::state::test_support::state;
    use crate::domain::preset::{ImportAction, ImportDecision};
    use crate::domain::settings::{Paper, PrintSettings};

    fn preset(printer: &str) -> Preset {
        Preset::from(PrintSettings {
            printer: Some(printer.into()),
            paper: Some(Paper::Standard("A4")),
            ..Default::default()
        })
    }

    #[test]
    fn import_supports_overwrite_skip_and_rename() {
        let state = state();
        let mut source = Config::default();
        source.presets.insert("struk".into(), preset("EPSON"));
        source.presets.insert("laporan".into(), preset("HP"));
        source.presets.insert("kartu".into(), preset("CANON"));
        source.default_preset = Some("struk".into());
        source.default_printer = Some("HP".into());
        let bundle = PresetBundle::from_config(&source, None, true);

        state
            .config
            .update(&mut |config| {
                config.presets.insert("struk".into(), preset("OLD"));
                config.presets.insert("laporan".into(), preset("KEEP"));
                config.presets.insert("kartu".into(), preset("REPLACE"));
                Ok(())
            })
            .unwrap();
        let preview = preview_import(&state, &bundle);
        assert!(preview.items.iter().find(|item| item.name == "struk").unwrap().exists);

        let options = ImportOptions {
            replace_all: false,
            include_defaults: true,
            decisions: vec![
                ImportDecision {
                    name: "struk".into(),
                    action: ImportAction::Rename,
                    new_name: None,
                    printer: Some("LOCAL-EPSON".into()),
                },
                ImportDecision {
                    name: "laporan".into(),
                    action: ImportAction::Skip,
                    new_name: None,
                    printer: None,
                },
                ImportDecision {
                    name: "kartu".into(),
                    action: ImportAction::Overwrite,
                    new_name: None,
                    printer: None,
                },
            ],
        };
        let (config, imported) = apply_import(&state, &bundle, &options).unwrap();
        assert_eq!(imported, vec!["kartu".to_string(), "struk (2)".to_string()]);
        assert_eq!(config.presets["struk"].settings.printer.as_deref(), Some("OLD"));
        assert_eq!(config.presets["laporan"].settings.printer.as_deref(), Some("KEEP"));
        assert_eq!(config.presets["kartu"].settings.printer.as_deref(), Some("CANON"));
        assert_eq!(
            config.presets["struk (2)"].settings.printer.as_deref(),
            Some("LOCAL-EPSON")
        );
        assert_eq!(config.default_preset.as_deref(), Some("struk (2)"));
        assert_eq!(config.default_printer.as_deref(), Some("HP"));
    }
}
