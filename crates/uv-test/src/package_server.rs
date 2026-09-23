//! Serve changing package contents and advertised hashes for integrity tests.

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use uv_normalize::PackageName;

/// A server for one package whose archive and index response can be replaced between commands.
pub struct PackageServer {
    server: MockServer,
    name: PackageName,
}

impl PackageServer {
    pub async fn new(name: &PackageName) -> Self {
        Self {
            server: MockServer::start().await,
            name: name.clone(),
        }
    }

    pub fn index_url(&self) -> String {
        format!("{}/simple", self.server.uri())
    }

    pub fn file_url(&self, filename: &str) -> String {
        format!("{}/{filename}", self.server.uri())
    }

    /// Access the mock server to add responses and request expectations.
    pub fn mock_server(&self) -> &MockServer {
        &self.server
    }

    /// Replace all responses with one archive and its index entry, keeping the server address.
    ///
    /// The advertised SHA-256 digest is independent of the archive's bytes. Pass `None` to omit it.
    pub async fn serve(&self, filename: &str, bytes: &[u8], advertised_sha256: Option<&str>) {
        self.serve_with(filename, bytes, advertised_sha256, json!({}))
            .await;
    }

    /// Replace all responses as in [`Self::serve`], with additional fields in the file entry.
    ///
    /// Supplied fields take precedence; nested objects are replaced rather than merged.
    pub async fn serve_with(
        &self,
        filename: &str,
        bytes: &[u8],
        advertised_sha256: Option<&str>,
        file_metadata: Value,
    ) {
        self.server.reset().await;
        let hashes = if let Some(sha256) = advertised_sha256 {
            json!({ "sha256": sha256 })
        } else {
            json!({})
        };
        let mut simple_index = json!({
            "meta": { "api-version": "1.0" },
            "name": self.name,
            "files": [{
                "filename": filename,
                "url": self.file_url(filename),
                "hashes": hashes,
                "upload-time": "2024-01-01T00:00:00Z",
            }],
        });
        for (key, value) in file_metadata
            .as_object()
            .expect("file metadata must be a JSON object")
        {
            simple_index["files"][0][key] = value.clone();
        }
        Mock::given(method("GET"))
            .and(path(format!("/simple/{}/", self.name)))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                simple_index.to_string(),
                "application/vnd.pypi.simple.v1+json",
            ))
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.to_vec()))
            .mount(&self.server)
            .await;
    }
}
