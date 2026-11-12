use std::future::poll_fn;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use http::{Response, StatusCode, Version};
use rustls::ServerConfig;
use rustls::pki_types::PrivatePkcs8KeyDer;
use tempfile::NamedTempFile;
use tokio::io;
use tokio::net::TcpListener;
use tokio::time;
use tokio_rustls::TlsAcceptor;

use uv_client::{BaseClientBuilder, Certificates, Connectivity};
use uv_redacted::SafeUrl;

use crate::http_util::generate_self_signed_certs_with_ca;

/// A retry must leave an HTTP/2 connection that stops acknowledging keep-alive pings.
#[tokio::test]
async fn retry_on_new_connection_after_missed_ping() -> Result<()> {
    let (ca, server, _) = generate_self_signed_certs_with_ca()?;
    let mut ca_file = NamedTempFile::new()?;
    ca_file.write_all(ca.public.pem().as_bytes())?;
    let mut tls = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![server.public.der().clone()],
            PrivatePkcs8KeyDer::from(server.private.serialize_der()).into(),
        )?;
    tls.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let url = SafeUrl::parse_relaxed(&format!("https://{}/package", listener.local_addr()?))?;

    let io_timeout = Duration::from_secs(120);
    let client = BaseClientBuilder::new(
        Connectivity::Online,
        false,
        vec![],
        "http2-keep-alive".parse()?,
        io_timeout,
        io_timeout,
        1,
    )
    .custom_certificates(Certificates::from_file(ca_file.path())?)
    .no_retry_delay(true)
    .build()?;

    let serve = async {
        let (mut first_socket, _) = listener.accept().await?;
        let uri = {
            let tls = acceptor.accept(&mut first_socket).await?;
            let mut first = h2::server::handshake(tls).await?;
            let (request, _) = first
                .accept()
                .await
                .context("expected the first request")??;
            request.uri().clone()
        };

        // Keep the socket open without driving HTTP/2, so the peer cannot acknowledge pings.
        // Advance through keep-alive detection while the original read timeout is still pending.
        time::pause();
        time::sleep(io_timeout / 2).await;
        time::resume();
        let closed = time::timeout(
            Duration::from_secs(5),
            io::copy(&mut first_socket, &mut io::sink()),
        )
        .await
        .context("the client did not close the unresponsive connection")?;
        if let Err(err) = closed {
            assert_eq!(err.kind(), io::ErrorKind::ConnectionReset, "{err}");
        }

        let (second_socket, _) = listener.accept().await?;
        let mut second = h2::server::handshake(acceptor.accept(second_socket).await?).await?;
        let (retry, mut respond) = second.accept().await.context("expected a retry")??;
        assert_eq!(retry.uri(), &uri);
        respond.send_response(Response::new(()), true)?;
        poll_fn(|cx| second.poll_closed(cx)).await?;
        anyhow::Ok(())
    };
    let fetch = async move {
        let response = client.for_host(&url).get(url).send().await?;
        assert_eq!(response.version(), Version::HTTP_2);
        assert_eq!(response.status(), StatusCode::OK);
        anyhow::Ok(())
    };
    tokio::try_join!(serve, fetch)?;
    Ok(())
}
