use crate::domain::preset::*;
use crate::domain::settings::{Paper, PrintSettings};
use crate::domain::settings_config::*;
use crate::infrastructure::persistence::config_store::ConfigStore;
use std::fs;
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
    assert!(fresh.lan_access);
    assert_eq!(fresh.server_port, DEFAULT_SERVER_PORT);
    // Saved before origins existed: every website stays allowed, visibly.
    let old: Config = serde_json::from_str(r#"{"version":2,"require_token":false}"#).unwrap();
    assert!(!old.require_token);
    assert!(old.lan_access);
    assert_eq!(old.allowed_origins, vec!["*"]);
    assert_eq!(old.server_port, DEFAULT_SERVER_PORT);
    assert!(old.shared_presets.is_empty());
    assert!(!old.preset_list_endpoint_enabled);
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
fn shared_presets_must_be_published() {
    let mut config = Config::default();
    config.preset_drafts.insert("draft-only".into(), Preset::default());
    config.shared_presets.insert("draft-only".into());
    assert!(config.validate().unwrap_err().contains("published preset"));
    config.presets.insert("published".into(), Preset::default());
    config.shared_presets.clear();
    config.shared_presets.insert("published".into());
    assert!(config.validate().is_ok());
}

#[test]
fn lan_access_false_roundtrips() {
    let config = Config {
        lan_access: false,
        ..Config::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let roundtrip: Config = serde_json::from_str(&json).unwrap();
    assert!(!roundtrip.lan_access);
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
