//! A per-test wiremock server that serves packse scenario packages.
//!
//! Each [`PackseServer`] reads a single scenario TOML file and serves:
//! - PEP 691 Simple API at `/simple/{package}/`
//! - Distribution downloads at `/files/{filename}`
//!
//! Vendored build dependencies are exposed through the same `/simple/*` and
//! `/files/*` routes as scenario packages.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use uv_distribution_filename::WheelFilename;
use uv_normalize::PackageName;

use super::scenario::Scenario;
use super::wheel::{generate_sdist, generate_wheel, sha256_hex};
use super::{scenarios_dir, vendor_dir};

/// Information about a single distribution file (metadata for the Simple API).
struct DistInfo {
    filename: String,
    sha256: String,
    requires_python: Option<String>,
    yanked: bool,
}

/// All distributions for a given package name, across versions.
struct PackageEntry {
    dists: Vec<DistInfo>,
}

/// The complete pre-indexed database for a server instance.
struct ServerIndex {
    /// Simple API: normalized package name → list of distribution metadata.
    packages: HashMap<PackageName, PackageEntry>,
    /// File downloads: filename → bytes.
    files: HashMap<String, Arc<[u8]>>,
}

/// A running mock PyPI server for a single packse scenario.
///
/// The server runs on a background thread with its own single-threaded tokio runtime.
/// When [`PackseServer`] is dropped, the background thread and server are shut down.
pub struct PackseServer {
    url: String,
    /// Signals the background thread to stop.
    _shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    /// The background thread handle.
    _thread: Option<thread::JoinHandle<()>>,
}

impl PackseServer {
    /// Load a scenario from a TOML path (relative to the vendored scenarios directory)
    /// and start a mock server for it.
    pub fn new(scenario_path: &str) -> Self {
        let full_path = scenarios_dir().join(scenario_path);
        let scenario = Scenario::from_path(&full_path);
        Self::from_scenario(&scenario)
    }

    /// Start a mock server with no packages (only vendored build dependencies).
    ///
    /// Useful as a dummy index that will 404 for any non-vendored package lookup.
    pub fn empty() -> Self {
        let scenario = Scenario {
            name: String::new(),
            description: None,
            packages: std::collections::BTreeMap::new(),
            root: super::scenario::RootPackage {
                requires_python: None,
                requires: Vec::new(),
            },
            expected: super::scenario::Expected {
                satisfiable: true,
                packages: std::collections::BTreeMap::new(),
                explanation: None,
            },
            environment: super::scenario::Environment::default(),
            resolver_options: super::scenario::ResolverOptions::default(),
        };
        Self::from_scenario(&scenario)
    }

    /// Start a mock server for the given scenario.
    pub fn from_scenario(scenario: &Scenario) -> Self {
        let vendor_path = vendor_dir();

        // Build the full index eagerly (on the calling thread).
        let index = Arc::new(build_server_index(scenario, &vendor_path));

        // Channel: background thread sends us the URL once the server is ready.
        let (url_tx, url_rx) = std::sync::mpsc::channel::<String>();
        // Channel: we send shutdown signal to the background thread.
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for PackseServer");

            rt.block_on(async move {
                let server = MockServer::start().await;
                let server_uri = server.uri();

                Mock::given(any())
                    .respond_with(move |req: &Request| handle_request(req, &server_uri, &index))
                    .mount(&server)
                    .await;

                // Tell the main thread the URL.
                url_tx.send(server.uri()).ok();

                // Keep the server alive until shutdown is signaled.
                let _ = shutdown_rx.await;
            });
        });

        let url = url_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("PackseServer: timed out waiting for server to start");

        Self {
            url,
            _shutdown: Some(shutdown_tx),
            _thread: Some(handle),
        }
    }

    /// The Simple API index URL (e.g., `http://127.0.0.1:PORT/simple/`).
    pub fn index_url(&self) -> String {
        format!("{}/simple/", self.url)
    }

    /// The base URI of the mock server.
    pub fn uri(&self) -> String {
        self.url.clone()
    }
}

impl Drop for PackseServer {
    fn drop(&mut self) {
        // Signal the background thread to stop.
        drop(self._shutdown.take());
        // Wait for it to finish.
        if let Some(handle) = self._thread.take() {
            handle.join().ok();
        }
    }
}

/// Build the complete [`ServerIndex`] from a scenario and vendored wheels.
fn build_server_index(scenario: &Scenario, vendor_path: &Path) -> ServerIndex {
    let mut packages = HashMap::new();
    let mut files: HashMap<String, Arc<[u8]>> = HashMap::new();

    // Index scenario packages.
    for (pkg_name, package) in &scenario.packages {
        let package_name =
            PackageName::from_str(pkg_name).expect("invalid package name in scenario");
        let mut dists = Vec::new();

        for (version, meta) in &package.versions {
            // Generate wheel(s)
            if meta.wheel {
                let tags = if meta.wheel_tags.is_empty() {
                    vec!["py3-none-any".to_string()]
                } else {
                    meta.wheel_tags.clone()
                };

                for tag in &tags {
                    let (filename, bytes) = generate_wheel(
                        pkg_name,
                        version,
                        &meta.requires,
                        &meta.extras,
                        meta.requires_python.as_deref(),
                        tag,
                    );
                    let sha256 = sha256_hex(&bytes);
                    let bytes: Arc<[u8]> = bytes.into();
                    files.insert(filename.clone(), Arc::clone(&bytes));
                    dists.push(DistInfo {
                        filename,
                        sha256,
                        requires_python: meta.requires_python.clone(),
                        yanked: meta.yanked,
                    });
                }
            }

            // Generate sdist
            if meta.sdist {
                let (filename, bytes) = generate_sdist(
                    pkg_name,
                    version,
                    &meta.requires,
                    &meta.extras,
                    meta.requires_python.as_deref(),
                );
                let sha256 = sha256_hex(&bytes);
                let bytes: Arc<[u8]> = bytes.into();
                files.insert(filename.clone(), Arc::clone(&bytes));
                dists.push(DistInfo {
                    filename,
                    sha256,
                    requires_python: meta.requires_python.clone(),
                    yanked: meta.yanked,
                });
            }
        }

        packages.insert(package_name, PackageEntry { dists });
    }

    // Index vendored build dependencies (wheels only).
    let entries = fs_err::read_dir(vendor_path).expect("failed to read vendor directory");
    for entry in entries {
        let entry = entry.expect("failed to read vendor directory entry");
        let path = entry.path();
        let Some(filename) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };

        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            continue;
        }

        let wheel_filename =
            WheelFilename::from_str(&filename).expect("invalid vendor wheel filename");
        let bytes = fs_err::read(&path).expect("failed to read vendor wheel");
        let sha256 = sha256_hex(&bytes);
        let bytes: Arc<[u8]> = bytes.into();

        files.insert(filename.clone(), Arc::clone(&bytes));
        packages
            .entry(wheel_filename.name)
            .or_insert_with(|| PackageEntry { dists: Vec::new() })
            .dists
            .push(DistInfo {
                filename,
                sha256,
                requires_python: None,
                yanked: false,
            });
    }

    ServerIndex { packages, files }
}

/// Handle an incoming request.
fn handle_request(req: &Request, server_uri: &str, index: &ServerIndex) -> ResponseTemplate {
    let path = req.url.path();

    // Simple API: /simple/{package}/
    if let Some(pkg) = extract_package_name(path) {
        let Ok(package_name) = PackageName::from_str(pkg) else {
            return ResponseTemplate::new(404);
        };

        if let Some(entry) = index.packages.get(&package_name) {
            return build_simple_api_response(pkg, entry, server_uri);
        }
        return ResponseTemplate::new(404);
    }

    // File download: /files/{filename}
    if let Some(filename) = path.strip_prefix("/files/") {
        if let Some(bytes) = index.files.get(filename) {
            return ResponseTemplate::new(200)
                .set_body_raw(bytes.to_vec(), content_type_for_filename(filename));
        }
        return ResponseTemplate::new(404);
    }

    ResponseTemplate::new(404)
}

/// Build PEP 691 JSON response for a package.
fn build_simple_api_response(
    package_name: &str,
    entry: &PackageEntry,
    server_uri: &str,
) -> ResponseTemplate {
    let files: Vec<serde_json::Value> = entry
        .dists
        .iter()
        .map(|dist| {
            let url = format!("{server_uri}/files/{}", dist.filename);
            let mut file_obj = json!({
                "filename": dist.filename,
                "url": url,
                "hashes": {
                    "sha256": dist.sha256,
                },
            });
            if let Some(rp) = &dist.requires_python {
                file_obj["requires-python"] = json!(rp);
            }
            if dist.yanked {
                file_obj["yanked"] = json!(true);
            }
            file_obj
        })
        .collect();

    let body = json!({
        "meta": { "api-version": "1.1" },
        "name": package_name,
        "files": files,
    });

    let body_str = body.to_string();
    ResponseTemplate::new(200)
        .insert_header("Content-Type", "application/vnd.pypi.simple.v1+json")
        .set_body_raw(body_str, "application/vnd.pypi.simple.v1+json")
}

fn content_type_for_filename(filename: &str) -> &'static str {
    if Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
    {
        "application/zip"
    } else {
        "application/gzip"
    }
}

/// Extract the package name from `/simple/{package}` or `/simple/{package}/`.
fn extract_package_name(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/simple/")?;
    let pkg = rest.strip_suffix('/').unwrap_or(rest);
    if pkg.is_empty() || pkg.contains('/') {
        return None;
    }
    Some(pkg)
}

#[cfg(test)]
mod tests {
    use super::extract_package_name;

    #[test]
    fn extract_package_name_accepts_with_or_without_trailing_slash() {
        assert_eq!(extract_package_name("/simple/foo/"), Some("foo"));
        assert_eq!(extract_package_name("/simple/foo"), Some("foo"));
    }

    #[test]
    fn extract_package_name_rejects_invalid_paths() {
        assert_eq!(extract_package_name("/simple/"), None);
        assert_eq!(extract_package_name("/simple"), None);
        assert_eq!(extract_package_name("/simple/foo/bar"), None);
    }
}
