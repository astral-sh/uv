use uv_config::GlobalConfig;

#[test]
fn test_set_and_get_version() {
    // Check Default Version Value
    let current_settings = GlobalConfig::settings().unwrap();
    assert_eq!(
        current_settings.version,
        env!("CARGO_PKG_VERSION").to_string()
    );

    let new_version = "1.2.3".to_string();
    // Note: This could affect other tests potentially if tests are running in parallel.
    let _ = GlobalConfig::update_version(new_version.clone());

    let settings = GlobalConfig::settings().unwrap();
    assert_eq!(settings.version, new_version);

    // Reset the version to default after testing to avoid side effects
    let _ = GlobalConfig::update_version(current_settings.version);
}
