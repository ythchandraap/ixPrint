use super::super::blocking;
use super::super::AppStateRef;
use crate::domain::preset::{ImportOptions, ImportPreview, Preset, PresetBundle};
use crate::domain::remote::SharedPresetListing;
use crate::domain::settings_config::Config;
#[tauri::command]
pub fn get_config(state: AppStateRef<'_>) -> Config {
    crate::application::preset_service::config(&state)
}

#[tauri::command]
pub fn set_default_printer(state: AppStateRef<'_>, printer: Option<String>) -> Result<Config, String> {
    crate::application::printer_service::set_default(&state, printer)
}

#[tauri::command]
pub fn set_default_preset(state: AppStateRef<'_>, name: Option<String>) -> Result<Config, String> {
    crate::application::preset_service::set_default_preset(&state, name)
}

#[tauri::command]
pub fn set_preset_shared(state: AppStateRef<'_>, name: String, shared: bool) -> Result<Config, String> {
    crate::application::preset_service::set_shared(&state, name, shared)
}

#[tauri::command]
pub fn set_preset_list_endpoint_enabled(state: AppStateRef<'_>, enabled: bool) -> Result<Config, String> {
    crate::application::preset_service::set_preset_list_endpoint_enabled(&state, enabled)
}

#[tauri::command]
pub async fn get_remote_shared_presets(
    state: AppStateRef<'_>,
    remote_id: String,
) -> Result<SharedPresetListing, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::preset_service::shared_presets(&state, &remote_id)).await?
}

#[tauri::command]
pub async fn preview_remote_preset_import(
    state: AppStateRef<'_>,
    remote_id: String,
    name: String,
) -> Result<ImportPreview, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::preset_service::preview_remote_import(&state, &remote_id, &name)).await?
}

#[tauri::command]
pub async fn apply_remote_preset_import(
    state: AppStateRef<'_>,
    remote_id: String,
    name: String,
    expected_sha256: String,
    options: ImportOptions,
) -> Result<ImportOutcome, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (config, imported) = crate::application::preset_service::apply_remote_import(
            &state,
            &remote_id,
            &name,
            &expected_sha256,
            &options,
        )?;
        Ok(ImportOutcome { config, imported })
    })
    .await?
}

/// Creates or updates a preset (settings + optional design); `original_name` renames it.
#[tauri::command]
pub fn save_preset(
    state: AppStateRef<'_>,
    name: String,
    original_name: Option<String>,
    preset: Preset,
) -> Result<Config, String> {
    crate::application::preset_service::save(&state, name, original_name, preset)
}

#[tauri::command]
pub fn save_preset_draft(state: AppStateRef<'_>, name: String, preset: Preset) -> Result<Config, String> {
    crate::application::preset_service::save_draft(&state, name, preset)
}

#[tauri::command]
pub fn publish_preset_draft(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    crate::application::preset_service::publish_draft(&state, &name)
}

#[tauri::command]
pub fn discard_preset_draft(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    crate::application::preset_service::discard_draft(&state, &name)
}

#[tauri::command]
pub fn restore_preset_previous(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    crate::application::preset_service::restore_previous(&state, &name)
}

#[tauri::command]
pub async fn test_preset_draft(state: AppStateRef<'_>, name: String) -> Result<crate::domain::job::JobResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let draft = state
            .config
            .get()
            .preset_drafts
            .get(&name)
            .cloned()
            .ok_or("No draft for this preset")?;
        let request = crate::domain::job::JobRequest {
            job_id: None,
            document: crate::domain::job::Document::TestPage { pages: 1 },
            history_input: None,
            file_name: format!("Draft test: {}", name),
            settings: Default::default(),
            preset: Some(name),
            source: "app",
            client: None,
            token: None,
            hop: false,
        };
        crate::application::job_service::run_with_preset(&state, request, Some(draft))
            .map_err(|error| error.message().to_string())
    })
    .await?
}

/// Print preview: the design filled with its sample data `rows` times; returns
/// up to 6 page images (PNG data URLs) and the total page count.
#[tauri::command]
pub async fn preview_print(
    state: AppStateRef<'_>,
    template: crate::domain::template::Template,
    rows: usize,
    px_per_mm: Option<f32>,
) -> Result<(Vec<String>, usize), String> {
    let state = state.inner().clone();
    blocking(move || crate::application::preset_service::preview_print(&state, &template, rows, px_per_mm)).await?
}

#[tauri::command]
pub async fn preview_print_data(
    state: AppStateRef<'_>,
    template: crate::domain::template::Template,
    records: Vec<crate::domain::template::Record>,
    header: crate::domain::template::Record,
    px_per_mm: Option<f32>,
) -> Result<(Vec<String>, usize), String> {
    let state = state.inner().clone();
    blocking(move || {
        crate::application::preset_service::preview_print_data(&state, &template, &records, &header, px_per_mm)
    })
    .await?
}

/// PNG data URL of a design filled with `data`, rendered exactly as it prints.
#[tauri::command]
pub async fn preview_template(
    state: AppStateRef<'_>,
    template: crate::domain::template::Template,
    data: crate::domain::template::Record,
    px_per_mm: Option<f32>,
) -> Result<String, String> {
    let state = state.inner().clone();
    blocking(move || crate::application::preset_service::preview_template(&state, &template, &data, px_per_mm)).await?
}

#[tauri::command]
pub fn delete_preset(state: AppStateRef<'_>, name: String) -> Result<Config, String> {
    crate::application::preset_service::remove(&state, &name)
}

/// Writes selected presets (all when `names` is empty) to `path`.
#[tauri::command]
pub fn export_presets(
    state: AppStateRef<'_>,
    path: String,
    names: Option<Vec<String>>,
    include_defaults: bool,
) -> Result<usize, String> {
    let names = names.filter(|n| !n.is_empty());
    let bundle = crate::application::preset_service::export_bundle(&state, names.as_deref(), include_defaults);
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    json_file(&path)?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to save {}: {}", path, e))?;
    Ok(bundle.presets.len())
}

/// Preset files are the only files the window reads or writes by path, so
/// anything else (executables, config of other apps, …) is refused.
fn json_file(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        return Err("Preset file must be a .json file".into());
    }
    Ok(())
}

const MAX_BUNDLE_BYTES: u64 = 50 * 1024 * 1024;

fn read_bundle(path: &str) -> Result<PresetBundle, String> {
    json_file(path)?;
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read {}: {}", path, e))?
        .len();
    if size > MAX_BUNDLE_BYTES {
        return Err("Preset file is too large".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    PresetBundle::parse(&text)
}

#[tauri::command]
pub async fn preview_import(state: AppStateRef<'_>, path: String) -> Result<ImportPreview, String> {
    let bundle = read_bundle(&path)?;
    Ok(crate::application::preset_service::preview_import(&state, &bundle))
}

#[derive(serde::Serialize)]
pub struct ImportOutcome {
    config: Config,
    imported: Vec<String>,
}

#[tauri::command]
pub fn apply_import(state: AppStateRef<'_>, path: String, options: ImportOptions) -> Result<ImportOutcome, String> {
    let bundle = read_bundle(&path)?;
    let (config, imported) = crate::application::preset_service::apply_import(&state, &bundle, &options)?;
    Ok(ImportOutcome { config, imported })
}

#[cfg(test)]
mod preview_pdf_tests {
    use crate::application::state::test_support::state;
    use crate::domain::settings::PrintSettings;
    use std::sync::Arc;

    /// The preview PNG must be the same sheet size `layout::build` would print,
    /// scaled by the design's own width/height — i.e. portrait vs. landscape
    /// tracks the requested orientation.
    #[test]
    fn preview_matches_requested_orientation() {
        crate::infrastructure::pdf::layout_engine::tests::init_for_tests();
        let mut state = Arc::try_unwrap(state()).ok().unwrap();
        state.pdf_engine = Arc::new(crate::infrastructure::pdf::PdfEngineAdapter);
        let dir = state.jobs_dir.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&state.jobs_dir).unwrap();
        let pdf = dir.join("source.pdf");
        crate::infrastructure::pdf::test_page::generate(&pdf, 210.0, 297.0, &["test".into()], 1).unwrap();

        let settings = PrintSettings {
            paper: Some("A4".parse().unwrap()),
            orientation: Some("landscape".parse().unwrap()),
            ..Default::default()
        };
        let url = crate::application::preset_service::preview_pdf_sheet(&state, pdf.to_str().unwrap(), None, &settings)
            .unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &url["data:image/png;base64,".len()..],
        )
        .unwrap();
        let mut decoder = png::Decoder::new(png.as_slice());
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let reader = decoder.read_info().unwrap();
        let (w, h) = (reader.info().width, reader.info().height);
        assert!(w > h, "landscape A4 preview should be wider than tall, got {}x{}", w, h);

        // No PDF extension: rejected before touching PDFium or the filesystem.
        let err = crate::application::preset_service::preview_pdf_sheet(
            &state,
            "source.txt",
            None,
            &PrintSettings::default(),
        )
        .unwrap_err();
        assert!(err.contains(".pdf"));

        // Unknown preset: rejected before rendering.
        let err = crate::application::preset_service::preview_pdf_sheet(
            &state,
            pdf.to_str().unwrap(),
            Some("nope"),
            &PrintSettings::default(),
        )
        .unwrap_err();
        assert!(err.contains("was not found"));

        // Temp file cleaned up either way.
        assert_eq!(std::fs::read_dir(dir.join("jobs")).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
