use std::path::Path;
use std::thread;
use std::time::Duration;

use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Background wiremock server shared by the local test indexes in this crate.
pub(crate) struct HttpServer {
    url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl HttpServer {
    pub(crate) fn start(
        handler: impl Fn(&Request, &str) -> ResponseTemplate + Send + Sync + 'static,
    ) -> Self {
        let (url_tx, url_rx) = std::sync::mpsc::channel::<String>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let thread = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for local HTTP test server");

            runtime.block_on(async move {
                let server = MockServer::start().await;
                let server_uri = server.uri();

                Mock::given(any())
                    .respond_with(move |request: &Request| handler(request, &server_uri))
                    .mount(&server)
                    .await;

                url_tx.send(server.uri()).ok();
                let _ = shutdown_rx.await;
            });
        });

        let url = url_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("timed out waiting for local HTTP test server to start");

        Self {
            url,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}

pub(crate) fn content_type_for_filename(filename: &str) -> &'static str {
    if Path::new(filename)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("whl"))
    {
        "application/zip"
    } else {
        "application/gzip"
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}
