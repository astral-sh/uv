#[expect(clippy::disallowed_types)]
use std::fs::{File, FileTimes};
use std::io::Write;
use std::path::PathBuf;
use std::{env, fs};

use uv_static::EnvVars;

/// Filter JSON entries to only include those matching the target OS.
fn filter_by_platform(data: serde_json::Value) -> serde_json::Value {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Map Cargo target OS to the JSON `os` field value
    let json_os = match target_os.as_str() {
        "macos" => Some("darwin"),
        "linux" => Some("linux"),
        "windows" => Some("windows"),
        _ => None, // Unknown OS: include all entries
    };

    let Some(json_os) = json_os else {
        return data;
    };

    let Some(obj) = data.as_object() else {
        return data;
    };

    let filtered: serde_json::Map<String, serde_json::Value> = obj
        .iter()
        .filter(|(_, value)| {
            value
                .get("os")
                .and_then(|v| v.as_str())
                .is_some_and(|os| os == json_os)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    serde_json::Value::Object(filtered)
}

fn main() {
    let version_metadata = PathBuf::from_iter([
        env::var(EnvVars::CARGO_MANIFEST_DIR).unwrap(),
        "download-metadata.json".into(),
    ]);

    let version_metadata_minified = PathBuf::from_iter([
        env::var(EnvVars::OUT_DIR).unwrap(),
        "download-metadata-minified.json".into(),
    ]);

    println!(
        "cargo::rerun-if-changed={}",
        version_metadata.to_str().unwrap()
    );

    println!(
        "cargo::rerun-if-changed={}",
        version_metadata_minified.to_str().unwrap()
    );

    #[expect(clippy::disallowed_methods)]
    let json_data: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&version_metadata).expect("Failed to read download-metadata.json"),
    )
    .expect("Failed to parse JSON");

    let filtered_data = filter_by_platform(json_data);

    #[expect(clippy::disallowed_types)]
    let mut out_file = File::create(version_metadata_minified)
        .expect("failed to open download-metadata-minified.json");

    out_file
        .write_all(
            serde_json::to_string(&filtered_data)
                .expect("Failed to serialize JSON")
                .as_bytes(),
        )
        .expect("Failed to write minified JSON");

    // Cargo uses the modified times of the paths specified in
    // `rerun-if-changed`, so fetch the current file times and set them the same
    // on the output file.
    #[expect(clippy::disallowed_methods)]
    let meta =
        fs::metadata(version_metadata).expect("failed to read metadata for download-metadata.json");

    out_file
        .set_times(
            FileTimes::new()
                .set_accessed(meta.accessed().unwrap())
                .set_modified(meta.modified().unwrap()),
        )
        .expect("failed to write file times to download-metadata-minified.json");
}
