use std::str::FromStr;

use anyhow::Result;
use insta::{assert_json_snapshot, assert_snapshot, with_settings};
use url::Url;

use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_client::{BaseClientBuilder, LineHaul};
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder};
use uv_platform_tags::{Arch, Os, Platform};
use uv_redacted::DisplaySafeUrl;
use uv_version::version;

use crate::http_util::start_http_user_agent_server;

#[tokio::test]
async fn test_user_agent_has_version() -> Result<()> {
    // Initialize dummy http server
    let (server_task, addr) = start_http_user_agent_server().await?;

    // Initialize uv-client
    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build();

    // Send request to our dummy server
    let url = DisplaySafeUrl::from_str(&format!("http://{addr}"))?;
    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Check User Agent
    let body = res.text().await?;

    let (uv_version, uv_linehaul) = body
        .split_once(' ')
        .expect("Failed to split User-Agent header");

    // Deserializing Linehaul
    let linehaul: LineHaul = serde_json::from_str(uv_linehaul)?;

    // Assert linehaul user agent
    let filters = vec![(version(), "[VERSION]")];
    with_settings!({
        filters => filters
    }, {
        // Assert uv version
        assert_snapshot!(uv_version, @"uv/[VERSION]");
        // Assert linehaul json
        assert_json_snapshot!(&linehaul.installer, @r#"
        {
          "name": "uv",
          "version": "[VERSION]",
          "subcommand": null
        }
        "#);
    });

    // Wait for the server task to complete, to be a good citizen.
    let _ = server_task.await?;

    Ok(())
}

#[tokio::test]
async fn test_user_agent_has_subcommand() -> Result<()> {
    // Initialize dummy http server
    let (server_task, addr) = start_http_user_agent_server().await?;

    // Initialize uv-client
    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default().subcommand(vec!["foo".to_owned(), "bar".to_owned()]),
        cache,
    )
    .build();

    // Send request to our dummy server
    let url = DisplaySafeUrl::from_str(&format!("http://{addr}"))?;
    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Check User Agent
    let body = res.text().await?;

    let (uv_version, uv_linehaul) = body
        .split_once(' ')
        .expect("Failed to split User-Agent header");

    // Deserializing Linehaul
    let linehaul: LineHaul = serde_json::from_str(uv_linehaul)?;

    // Assert linehaul user agent
    let filters = vec![(version(), "[VERSION]")];
    with_settings!({
        filters => filters
    }, {
        // Assert uv version
        assert_snapshot!(uv_version, @"uv/[VERSION]");
        // Assert linehaul json
        assert_json_snapshot!(&linehaul.installer, @r#"
        {
          "name": "uv",
          "version": "[VERSION]",
          "subcommand": [
            "foo",
            "bar"
          ]
        }
        "#);
    });

    // Wait for the server task to complete, to be a good citizen.
    let _ = server_task.await?;

    Ok(())
}

#[tokio::test]
async fn test_user_agent_has_linehaul() -> Result<()> {
    // Initialize dummy http server
    let (server_task, addr) = start_http_user_agent_server().await?;

    // Add some representative markers for an Ubuntu CI runner
    let markers = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: "3.12.2",
        os_name: "posix",
        platform_machine: "x86_64",
        platform_python_implementation: "CPython",
        platform_release: "6.5.0-1016-azure",
        platform_system: "Linux",
        platform_version: "#16~22.04.1-Ubuntu SMP Fri Feb 16 15:42:02 UTC 2024",
        python_full_version: "3.12.2",
        python_version: "3.12",
        sys_platform: "linux",
    })?;

    // Initialize uv-client
    let cache = Cache::temp()?.init().await?;
    let mut builder =
        RegistryClientBuilder::new(BaseClientBuilder::default(), cache).markers(&markers);

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
    let url = DisplaySafeUrl::from_str(&format!("http://{addr}"))?;
    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await?;

    // Check the HTTP status
    assert!(res.status().is_success());

    // Check User Agent
    let body = res.text().await?;

    // Wait for the server task to complete, to be a good citizen.
    let _ = server_task.await?;

    // Unpack User-Agent with linehaul
    let (uv_version, uv_linehaul) = body
        .split_once(' ')
        .expect("Failed to split User-Agent header");

    // Deserializing Linehaul
    let linehaul: LineHaul = serde_json::from_str(uv_linehaul)?;

    // Assert linehaul user agent
    let filters = vec![(version(), "[VERSION]")];
    with_settings!({
        filters => filters
    }, {
        // Assert uv version
        assert_snapshot!(uv_version, @"uv/[VERSION]");
        // Assert linehaul json
        assert_json_snapshot!(&linehaul, {
            ".distro" => "[distro]",
            ".ci" => "[ci]"
        }, @r#"
        {
          "installer": {
            "name": "uv",
            "version": "[VERSION]",
            "subcommand": null
          },
          "python": "3.12.2",
          "implementation": {
            "name": "CPython",
            "version": "3.12.2"
          },
          "distro": "[distro]",
          "system": {
            "name": "Linux",
            "release": "6.5.0-1016-azure"
          },
          "cpu": "x86_64",
          "openssl_version": null,
          "setuptools_version": null,
          "rustc_version": null,
          "ci": "[ci]"
        }
        "#);
    });

    // Assert distro
    if cfg!(windows) {
        assert_json_snapshot!(&linehaul.distro, @"null");
    } else if cfg!(target_os = "linux") {
        assert_json_snapshot!(&linehaul.distro, {
            ".id" => "[distro.id]",
            ".name" => "[distro.name]",
            ".version" => "[distro.version]"
            // We mock the libc version already
        }, @r###"
            {
              "name": "[distro.name]",
              "version": "[distro.version]",
              "id": "[distro.id]",
              "libc": {
                "lib": "glibc",
                "version": "2.38"
              }
            }"###
        );
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
        assert_json_snapshot!(&linehaul.distro, @r###"
            {
              "name": "macOS",
              "version": "14.4",
              "id": null,
              "libc": null
            }"###
        );
    }

    Ok(())
}
