use super::*;
use crate::application::state::test_support::state;
use crate::domain::preset::{ImportAction, ImportDecision, ImportOptions, PresetBundle};
use crate::domain::settings::{Paper, PrintSettings};

fn preset(printer: &str) -> Preset {
    Preset::from(PrintSettings {
        printer: Some(printer.into()),
        paper: Some(Paper::Standard("A4")),
        ..Default::default()
    })
}

#[test]
fn draft_does_not_change_live_preset_until_publish_and_can_be_restored() {
    let state = state();
    save(&state, "receipt".into(), None, preset("Printer A")).unwrap();
    assert!(save(&state, "receipt".into(), None, preset("Accidental overwrite")).is_err());
    save_draft(&state, "receipt".into(), preset("Printer B")).unwrap();
    let config = state.config.get();
    assert_eq!(config.presets["receipt"].settings.printer.as_deref(), Some("Printer A"));
    assert_eq!(
        config.preset_drafts["receipt"].settings.printer.as_deref(),
        Some("Printer B")
    );
    assert!(bridge_presets(&state).to_string().contains("Printer A"));
    assert!(!bridge_presets(&state).to_string().contains("Printer B"));

    publish_draft(&state, "receipt").unwrap();
    let config = state.config.get();
    assert_eq!(config.presets["receipt"].settings.printer.as_deref(), Some("Printer B"));
    assert_eq!(
        config.preset_previous["receipt"].settings.printer.as_deref(),
        Some("Printer A")
    );
    assert!(!config.preset_drafts.contains_key("receipt"));

    restore_previous(&state, "receipt").unwrap();
    assert_eq!(
        state.config.get().presets["receipt"].settings.printer.as_deref(),
        Some("Printer A")
    );
}

#[test]
fn sharing_requires_published_preset_and_tracks_rename_and_delete() {
    let state = state();
    assert!(set_shared(&state, "missing".into(), true).is_err());
    state
        .config
        .update(&mut |config| {
            config.preset_drafts.insert("draft-only".into(), preset("P"));
            Ok(())
        })
        .unwrap();
    assert!(set_shared(&state, "draft-only".into(), true).is_err());

    save(&state, "old name".into(), None, preset("P")).unwrap();
    set_shared(&state, "old name".into(), true).unwrap();
    let config = save(&state, "new name".into(), Some("old name".into()), preset("Q")).unwrap();
    assert!(!config.shared_presets.contains("old name"));
    assert!(config.shared_presets.contains("new name"));
    let config = remove(&state, "new name").unwrap();
    assert!(!config.shared_presets.contains("new name"));
}

#[test]
fn preflight_reports_missing_required_values_without_printing() {
    let state = state();
    let design: Template = serde_json::from_value(serde_json::json!({
        "width_mm": 50, "height_mm": 30,
        "required_fields": ["sku"],
        "elements": [{"type":"text", "x":1, "y":1, "w":40, "h":8, "text":"{{sku}}"}]
    }))
    .unwrap();
    save(
        &state,
        "label".into(),
        None,
        Preset {
            settings: Default::default(),
            design: Some(design),
        },
    )
    .unwrap();
    let report = preflight_data(
        &state,
        Some("label"),
        &serde_json::json!([{}]),
        None,
        &PrintSettings::default(),
    )
    .unwrap();
    assert!(!report.valid);
    assert!(report.errors[0].contains("data[1].sku"));
    let report = preflight_data(
        &state,
        Some("label"),
        &serde_json::json!([{"sku":"A1"}]),
        None,
        &PrintSettings::default(),
    )
    .unwrap();
    assert!(report.valid);
    assert_eq!(report.records, 1);
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
            config.shared_presets.insert("kartu".into());
            Ok(())
        })
        .unwrap();
    save_draft(&state, "kartu".into(), preset("DRAFT")).unwrap();
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
    assert!(!config.preset_drafts.contains_key("kartu"));
    assert!(!config.shared_presets.contains("kartu"));
    assert_eq!(
        config.preset_previous["kartu"].settings.printer.as_deref(),
        Some("REPLACE")
    );

    let options = ImportOptions {
        replace_all: true,
        include_defaults: false,
        decisions: vec![],
    };
    set_shared(&state, "struk".into(), true).unwrap();
    let (config, _) = apply_import(&state, &bundle, &options).unwrap();
    assert!(config.preset_drafts.is_empty());
    assert!(config.preset_previous.is_empty());
    assert!(config.shared_presets.is_empty());
}

struct SharedBundleGateway(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl crate::application::ports::RemoteGateway for SharedBundleGateway {
    fn shared_presets(
        &self,
        _remote: &crate::domain::remote::Remote,
    ) -> Result<crate::domain::remote::SharedPresetListing, crate::domain::remote::RemoteError> {
        Ok(crate::domain::remote::SharedPresetListing {
            listing_enabled: true,
            presets: vec![],
        })
    }

    fn fetch_shared_preset(
        &self,
        _remote: &crate::domain::remote::Remote,
        _name: &str,
    ) -> Result<Vec<u8>, crate::domain::remote::RemoteError> {
        Ok(self.0.lock().unwrap_or_else(|error| error.into_inner()).clone())
    }

    fn fetch_printers(
        &self,
        _remote: &crate::domain::remote::Remote,
    ) -> Result<Vec<crate::domain::printer::Printer>, crate::domain::remote::RemoteError> {
        unreachable!()
    }
    fn capabilities(
        &self,
        _remote: &crate::domain::remote::Remote,
        _name: &str,
    ) -> Result<crate::domain::printer::Capabilities, crate::domain::remote::RemoteError> {
        unreachable!()
    }
    fn queue(
        &self,
        _remote: &crate::domain::remote::Remote,
        _name: &str,
    ) -> Result<crate::domain::printer::PrinterQueue, crate::domain::remote::RemoteError> {
        unreachable!()
    }
    fn job_status(
        &self,
        _remote: &crate::domain::remote::Remote,
        _job_id: &str,
    ) -> Result<serde_json::Value, crate::domain::remote::RemoteError> {
        unreachable!()
    }
    fn print_pdf(
        &self,
        _remote: &crate::domain::remote::Remote,
        _path: &std::path::Path,
        _file_name: &str,
        _settings: &PrintSettings,
    ) -> Result<crate::domain::remote::RemotePrintResult, crate::domain::remote::RemoteError> {
        unreachable!()
    }
    fn test_page(
        &self,
        _remote: &crate::domain::remote::Remote,
        _pages: u32,
        _settings: &PrintSettings,
    ) -> Result<crate::domain::remote::RemotePrintResult, crate::domain::remote::RemoteError> {
        unreachable!()
    }
}

#[test]
fn remote_preview_detects_missing_printer_and_import_checks_hash_and_limits_options() {
    let mut source = Config::default();
    source.presets.insert("Shared".into(), preset("Printer not installed"));
    let bundle = PresetBundle::from_config(&source, Some(&["Shared".into()]), false);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let remote_bytes = std::sync::Arc::new(std::sync::Mutex::new(bytes.clone()));
    let mut owned_state = std::sync::Arc::try_unwrap(state()).ok().unwrap();
    owned_state.remote_gateway = std::sync::Arc::new(SharedBundleGateway(remote_bytes.clone()));
    let state = std::sync::Arc::new(owned_state);
    state
        .remotes
        .update(&mut |remotes| {
            remotes.push(crate::domain::remote::Remote {
                id: "source-id".into(),
                name: "Source".into(),
                url: "http://127.0.0.1:18032".into(),
                token: "fxp_source-token".into(),
                preset: None,
                created_at: String::new(),
            });
            Ok(())
        })
        .unwrap();
    state
        .config
        .update(&mut |config| {
            config.presets.insert("Keep".into(), preset("Local"));
            config.default_printer = Some("Local default".into());
            config.shared_presets.insert("Keep".into());
            Ok(())
        })
        .unwrap();

    let preview = preview_remote_import(&state, "source-id", "Shared").unwrap();
    assert!(preview.items[0].printer_missing);
    let expected = preview.bundle_sha256.unwrap();
    remote_bytes.lock().unwrap().push(b' ');
    let options = ImportOptions {
        replace_all: true,
        include_defaults: true,
        decisions: vec![],
    };
    let before = state.config.get();
    let error = apply_remote_import(&state, "source-id", "Shared", &expected, &options).unwrap_err();
    assert!(error.contains("changed on the source"));
    assert!(state.config.get().presets.contains_key("Keep"));
    assert_eq!(state.config.get().default_printer, before.default_printer);
    assert_eq!(state.config.get().shared_presets, before.shared_presets);

    *remote_bytes.lock().unwrap() = bytes;
    let (config, imported) = apply_remote_import(&state, "source-id", "Shared", &expected, &options).unwrap();
    assert_eq!(imported, vec!["Shared"]);
    assert!(config.presets.contains_key("Keep"));
    assert_eq!(config.default_printer.as_deref(), Some("Local default"));
    assert!(config.shared_presets.contains("Keep"));
    assert!(!config.shared_presets.contains("Shared"));
}
