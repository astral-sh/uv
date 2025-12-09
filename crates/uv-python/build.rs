#[allow(clippy::disallowed_types)]
use std::fs::{File, FileTimes};
use std::io::Write;
use std::path::PathBuf;
use std::{env, fs};

use uv_static::EnvVars;

fn process_json(data: &serde_json::Value) -> serde_json::Value {
    let mut out_data = serde_json::Map::new();

    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            out_data.insert(key.clone(), value.clone());
        }
    }

    serde_json::Value::Object(out_data)
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

    let json_data: serde_json::Value = serde_json::from_str(
        #[allow(clippy::disallowed_methods)]
        &fs::read_to_string(&version_metadata).expect("Failed to read download-metadata.json"),
    )
    .expect("Failed to parse JSON");

    let filtered_data = process_json(&json_data);

    #[allow(clippy::disallowed_types)]
    let mut out_file = File::create(version_metadata_minified)
        .expect("failed to open download-metadata-minified.json");

    #[allow(clippy::disallowed_methods)]
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
    #[allow(clippy::disallowed_methods)]
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
