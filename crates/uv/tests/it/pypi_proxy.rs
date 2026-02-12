//! A local mock of the `pypi-proxy.fly.dev` service for testing authenticated index access.
//!
//! The fly.dev proxy is an nginx reverse-proxy in front of PyPI that adds HTTP Basic Auth.
//! This module replicates its behavior with wiremock so tests don't depend on an external service.
//!
//! ## Routes
//!
//! | Path prefix                         | Auth?          | Behavior                                               |
//! |-------------------------------------|----------------|--------------------------------------------------------|
//! | `/simple/{pkg}/`                    | No             | Simple API JSON, file URLs → `/files/…`                |
//! | `/relative/simple/{pkg}/`           | No             | Simple API JSON, file URLs are relative (`../../../files/…`) |
//! | `/files/…`                          | No             | 302 redirect → `files.pythonhosted.org`                |
//! | `/basic-auth/simple/{pkg}/`         | `public:heron` | Simple API JSON, file URLs → `/basic-auth/files/…`     |
//! | `/basic-auth/relative/simple/{pkg}/`| `public:heron` | Simple API JSON, file URLs are relative                |
//! | `/basic-auth/files/…`              | `public:heron` | 302 redirect → `files.pythonhosted.org`                |
//! | `/basic-auth-heron/simple/{pkg}/`   | `public:heron` | Same as basic-auth but separate location               |
//! | `/basic-auth-heron/files/…`        | `public:heron` | 302 redirect → `files.pythonhosted.org`                |
//! | `/basic-auth-eagle/simple/{pkg}/`   | `public:eagle` | Same, different password                               |
//! | `/basic-auth-eagle/files/…`        | `public:eagle` | 302 redirect → `files.pythonhosted.org`                |

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::json;

/// Package metadata needed to build Simple API responses.
struct PackageEntry {
    filename: &'static str,
    url: &'static str,
    sha256: &'static str,
    requires_python: Option<&'static str>,
    size: u64,
    upload_time: &'static str,
}

/// All packages we serve. Keyed by normalized package name.
fn package_database() -> HashMap<&'static str, Vec<PackageEntry>> {
    let mut db = HashMap::new();

    db.insert(
        "iniconfig",
        vec![
            PackageEntry {
                filename: "iniconfig-2.0.0-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
                sha256: "b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374",
                requires_python: Some(">=3.7"),
                size: 5892,
                upload_time: "2023-01-07T11:08:09.864Z",
            },
            PackageEntry {
                filename: "iniconfig-2.0.0.tar.gz",
                url: "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
                sha256: "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3",
                requires_python: Some(">=3.7"),
                size: 4646,
                upload_time: "2023-01-07T11:08:11.254Z",
            },
        ],
    );

    db.insert(
        "anyio",
        vec![
            PackageEntry {
                filename: "anyio-4.3.0-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl",
                sha256: "048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8",
                requires_python: Some(">=3.8"),
                size: 85_584,
                upload_time: "2024-02-19T08:36:26.842Z",
            },
            PackageEntry {
                filename: "anyio-4.3.0.tar.gz",
                url: "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz",
                sha256: "f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6",
                requires_python: Some(">=3.8"),
                size: 159_642,
                upload_time: "2024-02-19T08:36:28.641Z",
            },
        ],
    );

    db.insert(
        "sniffio",
        vec![
            PackageEntry {
                filename: "sniffio-1.3.1-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl",
                sha256: "2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2",
                requires_python: Some(">=3.7"),
                size: 10235,
                upload_time: "2024-02-25T23:20:01.196Z",
            },
            PackageEntry {
                filename: "sniffio-1.3.1.tar.gz",
                url: "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz",
                sha256: "f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc",
                requires_python: Some(">=3.7"),
                size: 20372,
                upload_time: "2024-02-25T23:20:04.057Z",
            },
        ],
    );

    db.insert(
        "idna",
        vec![
            PackageEntry {
                filename: "idna-3.6-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl",
                sha256: "c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f",
                requires_python: Some(">=3.5"),
                size: 61567,
                upload_time: "2023-11-25T15:40:52.604Z",
            },
            PackageEntry {
                filename: "idna-3.6.tar.gz",
                url: "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz",
                sha256: "9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca",
                requires_python: Some(">=3.5"),
                size: 175_426,
                upload_time: "2023-11-25T15:40:54.902Z",
            },
        ],
    );

    db.insert(
        "executable-application",
        vec![
            PackageEntry {
                filename: "executable_application-0.3.0-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl",
                sha256: "ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6",
                requires_python: Some(">=3.8"),
                size: 1719,
                upload_time: "2025-01-17T23:21:22.716Z",
            },
            PackageEntry {
                filename: "executable_application-0.3.0.tar.gz",
                url: "https://files.pythonhosted.org/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz",
                sha256: "0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261",
                requires_python: Some(">=3.8"),
                size: 914,
                upload_time: "2025-01-17T23:21:24.559Z",
            },
        ],
    );

    db.insert(
        "typing-extensions",
        vec![
            PackageEntry {
                filename: "typing_extensions-4.10.0-py3-none-any.whl",
                url: "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl",
                sha256: "69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475",
                requires_python: Some(">=3.8"),
                size: 33926,
                upload_time: "2024-02-25T22:12:47.72Z",
            },
            PackageEntry {
                filename: "typing_extensions-4.10.0.tar.gz",
                url: "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz",
                sha256: "b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb",
                requires_python: Some(">=3.8"),
                size: 77558,
                upload_time: "2024-02-25T22:12:49.693Z",
            },
        ],
    );

    db
}

/// Build the JSON Simple API response for a package, rewriting file URLs.
///
/// - `file_url_prefix`: the base URL for files (e.g., `http://127.0.0.1:PORT/basic-auth/files`)
fn build_simple_api_response(
    package_name: &str,
    entries: &[PackageEntry],
    file_url_prefix: &str,
) -> serde_json::Value {
    let files: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            // Rewrite "https://files.pythonhosted.org/..." to "{file_url_prefix}/..."
            let rewritten_url = entry.url.replace(
                "https://files.pythonhosted.org/",
                &format!("{file_url_prefix}/"),
            );
            let mut file_obj = json!({
                "filename": entry.filename,
                "url": rewritten_url,
                "hashes": {
                    "sha256": entry.sha256
                },
                "size": entry.size,
                "upload-time": entry.upload_time,
            });
            if let Some(rp) = entry.requires_python {
                file_obj["requires-python"] = json!(rp);
            }
            file_obj
        })
        .collect();

    json!({
        "meta": { "api-version": "1.1" },
        "name": package_name,
        "files": files,
    })
}

/// Build the JSON Simple API response for a package with relative file URLs.
///
/// File URLs are relative paths like `../../../files/packages/...`
fn build_simple_api_response_relative(
    package_name: &str,
    entries: &[PackageEntry],
) -> serde_json::Value {
    let files: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            let rewritten_url = entry
                .url
                .replace("https://files.pythonhosted.org/", "../../../files/");
            let mut file_obj = json!({
                "filename": entry.filename,
                "url": rewritten_url,
                "hashes": {
                    "sha256": entry.sha256
                },
                "size": entry.size,
                "upload-time": entry.upload_time,
            });
            if let Some(rp) = entry.requires_python {
                file_obj["requires-python"] = json!(rp);
            }
            file_obj
        })
        .collect();

    json!({
        "meta": { "api-version": "1.1" },
        "name": package_name,
        "files": files,
    })
}

/// A running mock PyPI proxy server. Returned by [`start`].
pub(crate) struct PypiProxy {
    server: wiremock::MockServer,
}

impl PypiProxy {
    /// The base URI of the mock server (e.g., `http://127.0.0.1:PORT`).
    pub(crate) fn uri(&self) -> String {
        self.server.uri()
    }

    /// The host portion of the mock server address (e.g., `127.0.0.1`).
    pub(crate) fn host(&self) -> String {
        let url = url::Url::parse(&self.server.uri()).expect("valid URL");
        url.host_str().expect("has host").to_string()
    }

    /// The host:port portion of the mock server address (e.g., `127.0.0.1:PORT`).
    pub(crate) fn host_port(&self) -> String {
        self.server
            .uri()
            .strip_prefix("http://")
            .expect("wiremock server URI should start with http://")
            .to_string()
    }

    /// Build a URL with embedded credentials: `http://user:pass@host:port{path}`.
    pub(crate) fn authenticated_url(&self, username: &str, password: &str, path: &str) -> String {
        format!("http://{username}:{password}@{}{path}", self.host_port())
    }

    /// Build a base URI with embedded credentials: `http://user:pass@host:port` (no path).
    pub(crate) fn authenticated_uri(&self, username: &str, password: &str) -> String {
        format!("http://{username}:{password}@{}", self.host_port())
    }

    /// Build a URL with only a username: `http://user@host:port{path}`.
    pub(crate) fn username_url(&self, username: &str, path: &str) -> String {
        format!("http://{username}@{}{path}", self.host_port())
    }

    /// Build an unauthenticated URL: `http://host:port{path}`.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.uri())
    }
}

/// Start a mock PyPI proxy that replicates `pypi-proxy.fly.dev`.
///
/// The server handles:
/// - `/simple/{pkg}/` — unauthenticated Simple API
/// - `/basic-auth/simple/{pkg}/` — authenticated Simple API (public:heron)
/// - `/basic-auth-heron/simple/{pkg}/` — authenticated Simple API (public:heron)
/// - `/basic-auth-eagle/simple/{pkg}/` — authenticated Simple API (public:eagle)
/// - `/relative/simple/{pkg}/` — unauthenticated Simple API with relative file links
/// - `/basic-auth/relative/simple/{pkg}/` — authenticated Simple API with relative file links
/// - `/files/…` — unauthenticated file redirect to `files.pythonhosted.org`
/// - `/basic-auth/files/…` — authenticated file redirect (public:heron)
/// - `/basic-auth-heron/files/…` — authenticated file redirect (public:heron)
/// - `/basic-auth-eagle/files/…` — authenticated file redirect (public:eagle)
pub(crate) async fn start() -> PypiProxy {
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    let server = MockServer::start().await;
    let db = package_database();
    let server_uri = server.uri();

    // We use a single `respond_with` closure that inspects the request path and auth header
    // to decide what to do. This is simpler than mounting dozens of individual mocks.
    Mock::given(wiremock::matchers::any())
        .respond_with(move |req: &Request| {
            let path = req.url.path();

            // Check basic-auth credentials from the Authorization header.
            let auth = req
                .headers
                .get(&http::header::AUTHORIZATION)
                .and_then(parse_basic_auth);

            // Route: /basic-auth/files/...
            if let Some(rest) = path.strip_prefix("/basic-auth/files/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "heron")
                {
                    let target = format!("https://files.pythonhosted.org/{rest}");
                    return ResponseTemplate::new(302).insert_header("Location", target);
                }
                return unauthorized_response();
            }

            // Route: /basic-auth-heron/files/...
            if let Some(rest) = path.strip_prefix("/basic-auth-heron/files/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "heron")
                {
                    let target = format!("https://files.pythonhosted.org/{rest}");
                    return ResponseTemplate::new(302).insert_header("Location", target);
                }
                return unauthorized_response();
            }

            // Route: /basic-auth-eagle/files/...
            if let Some(rest) = path.strip_prefix("/basic-auth-eagle/files/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "eagle")
                {
                    let target = format!("https://files.pythonhosted.org/{rest}");
                    return ResponseTemplate::new(302).insert_header("Location", target);
                }
                return unauthorized_response();
            }

            // Route: /files/...  (unauthenticated)
            if let Some(rest) = path.strip_prefix("/files/") {
                let target = format!("https://files.pythonhosted.org/{rest}");
                return ResponseTemplate::new(302).insert_header("Location", target);
            }

            // Route: /basic-auth/relative/simple/{pkg}/
            if let Some(pkg) = extract_package_name(path, "/basic-auth/relative/simple/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "heron")
                {
                    if let Some(entries) = db.get(pkg) {
                        let body = build_simple_api_response_relative(pkg, entries);
                        return simple_api_response(&body);
                    }
                    return ResponseTemplate::new(404);
                }
                return unauthorized_response();
            }

            // Route: /basic-auth/simple/{pkg}/
            if let Some(pkg) = extract_package_name(path, "/basic-auth/simple/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "heron")
                {
                    if let Some(entries) = db.get(pkg) {
                        let file_prefix = format!("{server_uri}/basic-auth/files");
                        let body = build_simple_api_response(pkg, entries, &file_prefix);
                        return simple_api_response(&body);
                    }
                    return ResponseTemplate::new(404);
                }
                return unauthorized_response();
            }

            // Route: /basic-auth-heron/simple/{pkg}/
            if let Some(pkg) = extract_package_name(path, "/basic-auth-heron/simple/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "heron")
                {
                    if let Some(entries) = db.get(pkg) {
                        let file_prefix = format!("{server_uri}/basic-auth-heron/files");
                        let body = build_simple_api_response(pkg, entries, &file_prefix);
                        return simple_api_response(&body);
                    }
                    return ResponseTemplate::new(404);
                }
                return unauthorized_response();
            }

            // Route: /basic-auth-eagle/simple/{pkg}/
            if let Some(pkg) = extract_package_name(path, "/basic-auth-eagle/simple/") {
                if auth
                    .as_ref()
                    .is_some_and(|(u, p)| u == "public" && p == "eagle")
                {
                    if let Some(entries) = db.get(pkg) {
                        let file_prefix = format!("{server_uri}/basic-auth-eagle/files");
                        let body = build_simple_api_response(pkg, entries, &file_prefix);
                        return simple_api_response(&body);
                    }
                    return ResponseTemplate::new(404);
                }
                return unauthorized_response();
            }

            // Route: /relative/simple/{pkg}/  (unauthenticated, relative links)
            if let Some(pkg) = extract_package_name(path, "/relative/simple/") {
                if let Some(entries) = db.get(pkg) {
                    let body = build_simple_api_response_relative(pkg, entries);
                    return simple_api_response(&body);
                }
                return ResponseTemplate::new(404);
            }

            // Route: /simple/{pkg}/  (unauthenticated)
            // Unlike authenticated routes, file URLs point directly to files.pythonhosted.org
            // (matching the behavior of the original fly.dev proxy).
            if let Some(pkg) = extract_package_name(path, "/simple/") {
                if let Some(entries) = db.get(pkg) {
                    let file_prefix = "https://files.pythonhosted.org";
                    let body = build_simple_api_response(pkg, entries, file_prefix);
                    return simple_api_response(&body);
                }
                return ResponseTemplate::new(404);
            }

            ResponseTemplate::new(404)
        })
        .mount(&server)
        .await;

    PypiProxy { server }
}

/// Extract the package name from a path like `/prefix/{package}/`.
fn extract_package_name<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?;
    let pkg = rest.strip_suffix('/')?;
    // Only match single-segment names (no nested paths).
    if pkg.contains('/') {
        return None;
    }
    Some(pkg)
}

/// Parse a `Basic <base64>` Authorization header into (username, password).
fn parse_basic_auth(value: &wiremock::http::HeaderValue) -> Option<(String, String)> {
    use base64::Engine;

    let s = value.as_bytes();
    let s = std::str::from_utf8(s).ok()?;
    let encoded = s.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

fn unauthorized_response() -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(401)
        .insert_header("WWW-Authenticate", r#"Basic realm="authenticated""#)
}

fn simple_api_response(body: &serde_json::Value) -> wiremock::ResponseTemplate {
    // Compute a deterministic ETag from the body content.
    let body_str = body.to_string();
    let mut hasher = DefaultHasher::new();
    body_str.hash(&mut hasher);
    let etag = format!("\"{}\"", hasher.finish());

    // Mirror the cache headers that PyPI returns so our mock behaves like the
    // real index for HTTP caching purposes (Cache-Control, ETag).
    wiremock::ResponseTemplate::new(200)
        .insert_header("Cache-Control", "max-age=600, public")
        .insert_header("ETag", etag)
        .set_body_raw(body_str, "application/vnd.pypi.simple.v1+json")
}
