use anyhow::Result;
use futures::future;
use hyper::header::USER_AGENT;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Request, Response};
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
    tokio::spawn(async move {
        let svc = service_fn(move |req: Request<Body>| {
            // Get User Agent Header and send it back in the response
            let user_agent = req
                .headers()
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(); // Empty Default
            future::ok::<_, hyper::Error>(Response::new(Body::from(user_agent)))
        });
        // Start Hyper Server
        let (socket, _) = listener.accept().await.unwrap();
        Http::new()
            .http1_keep_alive(false)
            .serve_connection(socket, svc)
            .with_upgrades()
            .await
            .expect("Server Started");
    });

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

    // Check User Agent
    let body = res.text().await?;

    // Verify body matches regex
    assert_eq!(body, format!("uv/{}", version()));

    Ok(())
}

#[tokio::test]
async fn test_user_agent_has_linehaul() -> Result<()> {
    // Set up the TCP listener on a random available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Spawn the server loop in a background task
    tokio::spawn(async move {
        let svc = service_fn(move |req: Request<Body>| {
            // Get User Agent Header and send it back in the response
            let user_agent = req
                .headers()
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_default(); // Empty Default
            future::ok::<_, hyper::Error>(Response::new(Body::from(user_agent)))
        });
        // Start Hyper Server
        let (socket, _) = listener.accept().await.unwrap();
        Http::new()
            .http1_keep_alive(false)
            .serve_connection(socket, svc)
            .with_upgrades()
            .await
            .expect("Server Started");
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
        .uncached_client()
        .get(format!("http://{addr}"))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Check User Agent
    let body = res.text().await?;

    // Unpack User-Agent with linehaul
    let (uv_version, uv_linehaul) = body
        .split_once(' ')
        .expect("Failed to split User-Agent header.");

    // Deserializing Linehaul
    let linehaul: LineHaul = serde_json::from_str(uv_linehaul)?;

    // Assert uv version
    assert_eq!(uv_version, format!("uv/{}", version()));

    // Assert linehaul
    let installer_info = linehaul.installer.unwrap();
    let system_info = linehaul.system.unwrap();
    let impl_info = linehaul.implementation.unwrap();

    assert_eq!(installer_info.name.unwrap(), "uv".to_string());
    assert_eq!(installer_info.version.unwrap(), version());

    assert_eq!(system_info.name.unwrap(), markers.platform_system);
    assert_eq!(system_info.release.unwrap(), markers.platform_release);

    assert_eq!(
        impl_info.name.unwrap(),
        markers.platform_python_implementation
    );
    assert_eq!(
        impl_info.version.unwrap(),
        markers.python_full_version.version.to_string()
    );

    assert_eq!(
        linehaul.python.unwrap(),
        markers.python_full_version.version.to_string()
    );
    assert_eq!(linehaul.cpu.unwrap(), markers.platform_machine);

    assert_eq!(linehaul.openssl_version, None);
    assert_eq!(linehaul.setuptools_version, None);
    assert_eq!(linehaul.rustc_version, None);

    if cfg!(windows) {
        assert_eq!(linehaul.distro, None);
    } else if cfg!(target_os = "linux") {
        // Using `os_info` to confirm our values are as expected in Linux
        let info = os_info::get();
        let Some(distro_info) = linehaul.distro else {
            panic!("got no distro, but expected one in linehaul")
        };
        assert_eq!(distro_info.id.as_deref(), info.codename());
        if let Some(ref name) = distro_info.name {
            assert_eq!(name, &info.os_type().to_string());
        }
        if let Some(ref version) = distro_info.version {
            assert_eq!(version, &info.version().to_string());
        }
        assert!(distro_info.libc.is_some());
    } else if cfg!(target_os = "macos") {
        // We mock the macOS version
        let distro_info = linehaul.distro.unwrap();
        assert_eq!(distro_info.id, None);
        assert_eq!(distro_info.name.unwrap(), "macOS");
        assert_eq!(distro_info.version, Some("14.4".to_string()));
        assert_eq!(distro_info.libc, None);
    }

    Ok(())
}
