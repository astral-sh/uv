use std::path::PathBuf;
use std::{env, fs};

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
        env::var("CARGO_MANIFEST_DIR").unwrap(),
        "download-metadata.json".into(),
    ]);

    let version_metadata_minified = PathBuf::from_iter([
        env::var("OUT_DIR").unwrap(),
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
        &fs::read_to_string(version_metadata).expect("Failed to read download-metadata.json"),
    )
    .expect("Failed to parse JSON");

    let filtered_data = process_json(&json_data);

    #[allow(clippy::disallowed_methods)]
    fs::write(
        version_metadata_minified,
        serde_json::to_string(&filtered_data).expect("Failed to serialize JSON"),
    )
    .expect("Failed to write minified JSON");
}
