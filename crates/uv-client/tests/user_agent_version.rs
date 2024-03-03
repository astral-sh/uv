use anyhow::Result;
use futures::future;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::USER_AGENT;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_config::GlobalConfig;

#[tokio::test]
async fn test_user_agent_has_version() -> Result<()> {
    // Set up the TCP listener on a random available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Spawn the server loop in a background task
    let server_task = tokio::spawn(async move {
        let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
            // Get User Agent Header and send it back in the response
            let user_agent = req
                .headers()
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(); // Empty Default
            future::ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(user_agent))))
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

    // Initialize uv-client
    let cache = Cache::temp()?;
    let client = RegistryClientBuilder::new(cache).build();

    // Send request to our dummy server
    let res = client
        .cached_client()
        .uncached()
        .get(format!("http://{addr}"))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Check User Agent
    let version = GlobalConfig::settings().unwrap().version;
    let body = res.text().await?;

    // Verify body matches regex
    assert_eq!(body, format!("uv/{version}"));

    // Wait for the server task to complete, to be a good citizen.
    server_task.await?;

    Ok(())
}
