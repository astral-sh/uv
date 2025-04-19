use std::env;
use std::fs;
use std::path::Path;

fn process_json(data: &serde_json::Value, os_filter: &str, arch_filter: &str) -> serde_json::Value {
    let mut out_data = serde_json::Map::new();

    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            if let Some(variant) = value.get("variant") {
                // Exclude debug variants for now, we don't support them
                if variant == "debug" {
                    continue;
                }
            }

            if let Some(os) = value.get("os") {
                if os != os_filter {
                    continue;
                }
            }

            if let Some(arch) = value.get("arch").and_then(|a| a.get("family")) {
                if arch != arch_filter {
                    // Windows ARM64 runs emulated x86_64 binaries transparently,
                    // so we need to include the x86_64 variant for Windows ARM64.
                    if !(os_filter == "windows" && arch_filter == "aarch64" && arch == "x86_64") {
                        continue;
                    }
                }
            }

            out_data.insert(key.clone(), value.clone());
        }
    }

    serde_json::Value::Object(out_data)
}

fn main() {
    let version_metadata = Path::new("download-metadata.json");
    let target = Path::new("src/download-metadata-minified.json");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "macos" => "darwin".to_string(),
        other => other.to_string(),
    };

    let json_data: serde_json::Value = serde_json::from_str(
        #[allow(clippy::disallowed_methods)]
        &fs::read_to_string(version_metadata).expect("Failed to read download-metadata.json"),
    )
    .expect("Failed to parse JSON");

    let filtered_data = process_json(&json_data, &target_os, &target_arch);

    if filtered_data.as_object().unwrap().is_empty() {
        match target_os.as_str() {
            "windows" | "linux" | "darwin" => {
                panic!("No matching objects found for {target_os:?}, {target_arch:?}.");
            }
            _ => {
                // we only support managed python for the oss above
            }
        }
    }

    #[allow(clippy::disallowed_methods)]
    fs::write(
        target,
        serde_json::to_string(&filtered_data).expect("Failed to serialize JSON"),
    )
    .expect("Failed to write minified JSON");
}
