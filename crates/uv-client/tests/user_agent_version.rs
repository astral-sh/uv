use anyhow::Result;
use futures::future;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::USER_AGENT;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use insta::{assert_json_snapshot, assert_snapshot, with_settings};
use pep508_rs::{MarkerEnvironment, StringVersion};
use platform_tags::{Arch, Os, Platform};
use tokio::net::TcpListener;
use uv_cache::Cache;
use uv_client::LineHaul;
use uv_client::RegistryClientBuilder;
use uv_version::version;

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
    let body = res.text().await?;

    // Verify body matches regex
    assert_eq!(body, format!("uv/{}", version()));

    // Wait for the server task to complete, to be a good citizen.
    server_task.await?;

    Ok(())
}

#[tokio::test]
async fn test_user_agent_has_linehaul() -> Result<()> {
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

    // Add some representative markers for an Ubuntu CI runner
    let markers = MarkerEnvironment {
        implementation_name: "cpython".to_string(),
        implementation_version: StringVersion {
            string: "3.12.2".to_string(),
            version: "3.12.2".parse()?,
        },
        os_name: "posix".to_string(),
        platform_machine: "x86_64".to_string(),
        platform_python_implementation: "CPython".to_string(),
        platform_release: "6.5.0-1016-azure".to_string(),
        platform_system: "Linux".to_string(),
        platform_version: "#16~22.04.1-Ubuntu SMP Fri Feb 16 15:42:02 UTC 2024".to_string(),
        python_full_version: StringVersion {
            string: "3.12.2".to_string(),
            version: "3.12.2".parse()?,
        },
        python_version: StringVersion {
            string: "3.12".to_string(),
            version: "3.12".parse()?,
        },
        sys_platform: "linux".to_string(),
    };

    // Initialize uv-client
    let cache = Cache::temp()?;
    let mut builder = RegistryClientBuilder::new(cache).markers(&markers);

    let linux = Platform::new(
        Os::Manylinux {
            major: 2,
            minor: 38,
        },
        Arch::X86_64,
    );
    let macos = Platform::new(
        Os::Macos {
            major: 14,
            minor: 4,
        },
        Arch::Aarch64,
    );
    if cfg!(target_os = "linux") {
        builder = builder.platform(&linux);
    } else if cfg!(target_os = "macos") {
        builder = builder.platform(&macos);
    }
    let client = builder.build();

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
    let body = res.text().await?;

    // Wait for the server task to complete, to be a good citizen.
    server_task.await?;

    // Unpack User-Agent with linehaul
    let (uv_version, uv_linehaul) = body
        .split_once(' ')
        .expect("Failed to split User-Agent header.");

    // Deserializing Linehaul
    let linehaul: LineHaul = serde_json::from_str(uv_linehaul)?;

    // Assert linehaul user agent
    let filters = vec![(version(), "[VERSION]")];
    with_settings!({
        filters => filters
    }, {
        // Assert uv version
        assert_snapshot!("uv_linehaul_version", uv_version);
        // Assert linehaul json
        assert_json_snapshot!("uv_linehaul_json", &linehaul, {
            ".distro" => "[distro]",
            ".ci" => "[ci]"
        });
    });

    // Assert distro
    if cfg!(windows) {
        assert_json_snapshot!("uv_linehaul_distro_windows", &linehaul.distro);
    } else if cfg!(target_os = "linux") {
        assert_json_snapshot!("uv_linehaul_distro_linux", &linehaul.distro, {
            ".id" => "[distro.id]",
            ".name" => "[distro.name]",
            ".version" => "[distro.version]"
            // We mock the libc version already
        });
        // Check dynamic values
        let distro_info = linehaul
            .distro
            .expect("got no distro, but expected one in linehaul");
        // Gather distribution info from /etc/os-release.
        let release_info = sys_info::linux_os_release()
            .expect("got no os release info, but expected one in linux");
        assert_eq!(distro_info.id, release_info.version_codename);
        assert_eq!(distro_info.name, release_info.name);
        assert_eq!(distro_info.version, release_info.version_id);
    } else if cfg!(target_os = "macos") {
        // We mock the macOS distro
        assert_json_snapshot!("uv_linehaul_distro_macos", &linehaul.distro);
    }

    Ok(())
}
