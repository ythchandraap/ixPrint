use crate::application::state::AppState;
use crate::domain::preset::{self, ImportOptions, ImportPreview, PresetBundle};
use crate::domain::remote::{SharedPreset, SharedPresetListing};
use crate::domain::settings_config::Config;
use sha2::Digest;

fn stored_remote(state: &AppState, remote_id: &str) -> Result<crate::domain::remote::Remote, String> {
    state
        .remotes
        .list()
        .into_iter()
        .find(|remote| remote.id == remote_id)
        .ok_or_else(|| "Remote computer was not found".into())
}

pub fn shared_presets(state: &AppState, remote_id: &str) -> Result<SharedPresetListing, String> {
    let remote = stored_remote(state, remote_id)?;
    state
        .remote_gateway
        .shared_presets(&remote)
        .map_err(|error| error.to_string())
}

fn fetch_remote_bundle(state: &AppState, remote_id: &str, name: &str) -> Result<(PresetBundle, Vec<u8>), String> {
    preset::validate_preset_name(name)?;
    let remote = stored_remote(state, remote_id)?;
    let bytes = state
        .remote_gateway
        .fetch_shared_preset(&remote, name)
        .map_err(|error| error.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Remote returned an invalid preset bundle".to_string())?;
    let bundle = PresetBundle::parse(text).map_err(|_| "Remote returned an invalid preset bundle".to_string())?;
    if bundle.presets.len() != 1
        || !bundle.presets.contains_key(name)
        || bundle.default_printer.is_some()
        || bundle.default_preset.is_some()
    {
        return Err("Remote returned an invalid shared preset bundle".into());
    }
    Ok((bundle, bytes))
}

pub fn preview_remote_import(state: &AppState, remote_id: &str, name: &str) -> Result<ImportPreview, String> {
    let (bundle, bytes) = fetch_remote_bundle(state, remote_id, name)?;
    let installed = state
        .printer_gateway
        .list_printers()
        .into_iter()
        .map(|printer| printer.name)
        .collect::<Vec<_>>();
    let mut preview = preset::preview_import(&bundle, &state.config.get(), &installed);
    let digest = sha2::Sha256::digest(&bytes);
    preview.bundle_sha256 = Some(digest.iter().map(|byte| format!("{byte:02x}")).collect());
    Ok(preview)
}

pub fn apply_remote_import(
    state: &AppState,
    remote_id: &str,
    name: &str,
    expected_sha256: &str,
    options: &ImportOptions,
) -> Result<(Config, Vec<String>), String> {
    let (bundle, bytes) = fetch_remote_bundle(state, remote_id, name)?;
    let actual = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected_sha256 {
        return Err("The preset changed on the source computer. Preview it again before importing.".into());
    }
    let options = ImportOptions {
        replace_all: false,
        include_defaults: false,
        decisions: options.decisions.clone(),
    };
    apply_import(state, &bundle, &options)
}

pub fn export_bundle(state: &AppState, names: Option<&[String]>, include_defaults: bool) -> PresetBundle {
    preset::PresetBundle::from_config(&state.config.get(), names, include_defaults)
}

pub fn shared_listing(state: &AppState) -> (bool, Vec<SharedPreset>) {
    let config = state.config.get();
    let presets = config
        .shared_presets
        .iter()
        .filter_map(|name| {
            config.presets.get(name).map(|preset| SharedPreset {
                name: name.clone(),
                has_design: preset.design.is_some(),
            })
        })
        .collect();
    (config.preset_list_endpoint_enabled, presets)
}

pub fn export_shared_preset(state: &AppState, name: &str) -> Option<PresetBundle> {
    let config = state.config.get();
    if !config.shared_presets.contains(name) || !config.presets.contains_key(name) {
        return None;
    }
    let mut bundle = preset::PresetBundle::from_config(&config, Some(&[name.to_owned()]), false);
    // A generated timestamp would make the raw bundle hash differ on every
    // fetch, even if the published preset had not changed.
    bundle.exported_at.clear();
    Some(bundle)
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
        let prior: std::collections::BTreeMap<_, _> = bundle
            .presets
            .keys()
            .filter_map(|name| config.presets.get(name).map(|preset| (name.clone(), preset.clone())))
            .collect();
        imported = preset::apply_import(config, bundle, options)?;
        if options.replace_all {
            config.preset_drafts.clear();
            config.preset_previous.clear();
        } else {
            for name in &imported {
                config.preset_drafts.remove(name);
                if let Some(previous) = prior.get(name) {
                    config.preset_previous.insert(name.clone(), previous.clone());
                } else {
                    config.preset_previous.remove(name);
                }
            }
        }
        Ok(())
    })?;
    Ok((config, imported))
}
