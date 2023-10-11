use std::str::FromStr;

use anyhow::Result;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use puffin_client::{PypiClientBuilder, SimpleJson};
use puffin_resolver::{ResolveFlags, Resolver};

#[tokio::test]
async fn setuptools() -> Result<()> {
    // Setup standard environment.
    let markers = MarkerEnvironment {
        implementation_name: "cpython".to_string(),
        implementation_version: StringVersion::from_str("3.11.5").unwrap(),
        os_name: "posix".to_string(),
        platform_machine: "arm64".to_string(),
        platform_python_implementation: "CPython".to_string(),
        platform_release: "21.6.0".to_string(),
        platform_system: "Darwin".to_string(),
        platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000".to_string(),
        python_full_version: StringVersion::from_str("3.11.5").unwrap(),
        python_version: StringVersion::from_str("3.11").unwrap(),
        sys_platform: "darwin".to_string(),
    };
    let platform = Platform::new(
        Os::Macos {
            major: 21,
            minor: 6,
        },
        Arch::Aarch64,
    );
    let tags = Tags::from_env(&platform, (3, 11))?;

    // Setup the mock server.
    let server = MockServer::start().await;
    setup(&server).await?;
    let client = PypiClientBuilder::default()
        .registry(server.uri().parse().unwrap())
        .proxy(server.uri().parse().unwrap())
        .build();

    // Resolve the requirements.
    let resolver = Resolver::new(&markers, &tags, &client);
    let resolution = resolver
        .resolve(
            [Requirement::from_str("setuptools").unwrap()].iter(),
            ResolveFlags::default(),
        )
        .await?;

    assert_eq!(format!("{resolution}"), "setuptools==68.2.2");

    Ok(())
}

#[tokio::test]
async fn scipy() -> Result<()> {
    // Setup standard environment.
    let markers = MarkerEnvironment {
        implementation_name: "cpython".to_string(),
        implementation_version: StringVersion::from_str("3.11.5").unwrap(),
        os_name: "posix".to_string(),
        platform_machine: "arm64".to_string(),
        platform_python_implementation: "CPython".to_string(),
        platform_release: "21.6.0".to_string(),
        platform_system: "Darwin".to_string(),
        platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000".to_string(),
        python_full_version: StringVersion::from_str("3.11.5").unwrap(),
        python_version: StringVersion::from_str("3.11").unwrap(),
        sys_platform: "darwin".to_string(),
    };
    let platform = Platform::new(
        Os::Macos {
            major: 21,
            minor: 6,
        },
        Arch::Aarch64,
    );
    let tags = Tags::from_env(&platform, (3, 11))?;

    // Setup the mock server.
    let server = MockServer::start().await;
    setup(&server).await?;
    let client = PypiClientBuilder::default()
        .registry(server.uri().parse().unwrap())
        .proxy(server.uri().parse().unwrap())
        .build();

    // Resolve the requirements.
    let resolver = Resolver::new(&markers, &tags, &client);
    let resolution = resolver
        .resolve(
            [Requirement::from_str("scipy<1.11.3").unwrap()].iter(),
            ResolveFlags::default(),
        )
        .await?;

    assert_eq!(format!("{resolution}"), "scipy==1.11.2\nnumpy==1.25.2");

    Ok(())
}

/// Setup the mock package registry.
async fn setup(server: &MockServer) -> Result<()> {
    /// Rewrite `https://files.pythonhosted.org` URLs to the mock server.
    fn rewrite_urls(simple_json: &mut SimpleJson, server: &MockServer) {
        for file in &mut simple_json.files {
            file.url = file
                .url
                .replace("https://files.pythonhosted.org", server.uri().as_str());
        }
    }

    /// Mock a package.
    macro_rules! mock_package {
        ($server:expr, $package:literal) => {
            let mut simple_json = serde_json::from_str::<SimpleJson>(include_str!(concat!(
                "../tests/data/packages/",
                $package,
                ".json"
            )))?;
            rewrite_urls(&mut simple_json, $server);
            Mock::given(method("GET"))
                .and(path(concat!("/simple/", $package, "/")))
                .respond_with(ResponseTemplate::new(200).set_body_json(&simple_json))
                .mount($server)
                .await;
        };
    }

    mock_package!(server, "numpy");
    mock_package!(server, "scipy");
    mock_package!(server, "setuptools");

    /// Mock a wheel file.
    macro_rules! mock_wheel {
        ($server:expr, $sha:literal, $wheel:literal) => {
            Mock::given(method("GET"))
                .and(path(concat!("/packages/", $sha, "/", $wheel, ".metadata")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(
                    include_bytes!(concat!("../tests/data/wheels/", $wheel, ".metadata")).to_vec(),
                ))
                .mount($server)
                .await;
        };
    }

    mock_wheel!(
        server,
        "bb/26/7945080113158354380a12ce26873dd6c1ebd88d47f5bc24e2c5bb38c16a",
        "setuptools-68.2.2-py3-none-any.whl"
    );
    mock_wheel!(
        server,
        "2a/12/62804d63514ecd9d2ecb73497c3e38094f9139bc60b0353b653253d106bb",
        "scipy-1.11.2-cp311-cp311-macosx_12_0_arm64.whl"
    );
    mock_wheel!(
        server,
        "86/a1/b8ef999c32f26a97b5f714887e21f96c12ae99a38583a0a96e65283ac0a1",
        "numpy-1.25.2-cp311-cp311-macosx_11_0_arm64.whl"
    );

    Ok(())
}
