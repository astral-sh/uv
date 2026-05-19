//! A local HTTP server that serves a directory of files as a PEP 503 flat link page.
//!
//! Useful for testing `--find-links` with an HTTP URL.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use wiremock::{Request, ResponseTemplate};

use crate::http_server::{HttpServer, content_type_for_filename};

/// A running HTTP server that serves files from a directory as a flat links page.
pub struct FindLinksServer {
    server: HttpServer,
}

impl FindLinksServer {
    /// Start a server that serves all files in the given directory.
    pub fn new(directory: &Path) -> Self {
        let mut files: HashMap<String, Arc<[u8]>> = HashMap::new();
        let mut filenames: Vec<String> = Vec::new();

        for entry in fs_err::read_dir(directory).expect("failed to read find-links directory") {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(filename) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            let bytes = fs_err::read(&path).expect("failed to read file");
            files.insert(filename.clone(), bytes.into());
            filenames.push(filename);
        }
        filenames.sort();

        let files = Arc::new(files);
        let filenames = Arc::new(filenames);
        let server = HttpServer::start(move |request, server_uri| {
            handle_request(request, server_uri, &files, &filenames)
        });

        Self { server }
    }

    /// The base URL of the server (for use with `--find-links`).
    pub fn url(&self) -> &str {
        self.server.url()
    }
}

fn handle_request(
    request: &Request,
    server_uri: &str,
    files: &HashMap<String, Arc<[u8]>>,
    filenames: &[String],
) -> ResponseTemplate {
    let path = request.url.path();

    if path == "/" {
        let links = filenames
            .iter()
            .map(|filename| format!("<a href=\"{server_uri}/{filename}\">{filename}</a>"))
            .collect::<Vec<_>>()
            .join("\n");
        let html = format!("<!DOCTYPE html>\n<html><body>\n{links}\n</body></html>");
        return ResponseTemplate::new(200).set_body_raw(html, "text/html");
    }

    let filename = path.trim_start_matches('/');
    if let Some(bytes) = files.get(filename) {
        return ResponseTemplate::new(200)
            .set_body_raw(bytes.to_vec(), content_type_for_filename(filename));
    }

    ResponseTemplate::new(404)
}
