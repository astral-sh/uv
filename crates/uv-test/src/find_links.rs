//! A local HTTP server that serves a directory of files as a PEP 503 flat link page.
//!
//! Useful for testing `--find-links` with an HTTP URL.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use std::thread;

use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// A running HTTP server that serves files from a directory as a flat links page.
pub struct FindLinksServer {
    url: String,
    _shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    _thread: Option<thread::JoinHandle<()>>,
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

        let (url_tx, url_rx) = std::sync::mpsc::channel::<String>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for FindLinksServer");

            rt.block_on(async move {
                let server = MockServer::start().await;
                let server_uri = server.uri();

                let files_clone = Arc::clone(&files);
                let filenames_clone = Arc::clone(&filenames);

                Mock::given(any())
                    .respond_with(move |req: &Request| {
                        let path = req.url.path();

                        // Root: flat HTML links page.
                        if path == "/" {
                            let links: String = filenames_clone
                                .iter()
                                .map(|f| format!("<a href=\"{server_uri}/{f}\">{f}</a>"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let html =
                                format!("<!DOCTYPE html>\n<html><body>\n{links}\n</body></html>");
                            return ResponseTemplate::new(200).set_body_raw(html, "text/html");
                        }

                        // File download.
                        let filename = path.trim_start_matches('/');
                        if let Some(bytes) = files_clone.get(filename) {
                            let content_type = if Path::new(filename)
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
                            {
                                "application/zip"
                            } else {
                                "application/gzip"
                            };
                            return ResponseTemplate::new(200)
                                .set_body_raw(bytes.to_vec(), content_type);
                        }

                        ResponseTemplate::new(404)
                    })
                    .mount(&server)
                    .await;

                url_tx.send(server.uri()).ok();
                let _ = shutdown_rx.await;
            });
        });

        let url = url_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("FindLinksServer: timed out waiting for server to start");

        Self {
            url,
            _shutdown: Some(shutdown_tx),
            _thread: Some(handle),
        }
    }

    /// The base URL of the server (for use with `--find-links`).
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for FindLinksServer {
    fn drop(&mut self) {
        drop(self._shutdown.take());
        if let Some(handle) = self._thread.take() {
            handle.join().ok();
        }
    }
}
