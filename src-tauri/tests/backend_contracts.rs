use ix_print_lib::domain::settings_config::Config;

#[test]
fn legacy_config_v2_without_lan_access_is_readable() {
    let config: Config = serde_json::from_str(include_str!("fixtures/config_v2.json"))
        .expect("deserialize pre-LAN-access config v2 fixture");

    assert_eq!(config.version, 2);
    assert_eq!(config.server_port, 18032);
    assert!(config.lan_access);
}
