use std::env;
use std::io::Write;

use anyhow::Result;
use futures::future;
use http::header::AUTHORIZATION;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tempfile::NamedTempFile;
use tokio::net::TcpListener;

use uv_cache::Cache;
use uv_client::RegistryClientBuilder;

#[tokio::test]
async fn test_client_with_netrc_credentials() -> Result<()> {
    // Set up the TCP listener on a random available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Spawn the server loop in a background task
    tokio::spawn(async move {
        let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
            // Get User Agent Header and send it back in the response
            let auth = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(); // Empty Default
            future::ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(auth))))
        });
        // Start Server (not wrapped in loop {} since we want a single response server)
        // If you want server to accept multiple connections, wrap it in loop {}
        let (socket, _) = listener.accept().await.unwrap();
        let socket = TokioIo::new(socket);
        tokio::task::spawn(async move {
            http1::Builder::new()
                .serve_connection(socket, svc)
                .with_upgrades()
                .await
                .expect("Server Started");
        });
    });

    // Create a netrc file
    let mut netrc_file = NamedTempFile::new()?;
    env::set_var("NETRC", netrc_file.path());
    writeln!(netrc_file, "machine 127.0.0.1 login user password 1234")?;

    // Initialize uv-client
    let cache = Cache::temp()?;
    let client = RegistryClientBuilder::new(cache).build();

    // Send request to our dummy server
    let res = client
        .uncached_client()
        .get(format!("http://{addr}"))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Verify auth header
    assert_eq!(res.text().await?, "Basic dXNlcjoxMjM0");

    Ok(())
}
