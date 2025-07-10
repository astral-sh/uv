use std::fs;
use std::path::Path;

fn process_json(data: &serde_json::Value) -> serde_json::Value {
    let mut out_data = serde_json::Map::new();

    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            if let Some(variant) = value.get("variant") {
                // Exclude debug variants for now, we don't support them
                if variant == "debug" {
                    continue;
                }
            }

            out_data.insert(key.clone(), value.clone());
        }
    }

    serde_json::Value::Object(out_data)
}

fn main() {
    let version_metadata = "download-metadata.json";
    println!("cargo::rerun-if-changed={version_metadata}");
    let target = Path::new("src/download-metadata-minified.json");

    let json_data: serde_json::Value = serde_json::from_str(
        #[allow(clippy::disallowed_methods)]
        &fs::read_to_string(version_metadata).expect("Failed to read download-metadata.json"),
    )
    .expect("Failed to parse JSON");

    let filtered_data = process_json(&json_data);

    #[allow(clippy::disallowed_methods)]
    fs::write(
        target,
        serde_json::to_string(&filtered_data).expect("Failed to serialize JSON"),
    )
    .expect("Failed to write minified JSON");
}
