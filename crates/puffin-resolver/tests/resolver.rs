//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;

use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_resolver::{ResolveFlags, Resolver};

#[tokio::test]
async fn black() -> Result<()> {
    let client = PypiClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let resolver = Resolver::new(requirements, &MARKERS_311, &TAGS_311, &client);
    let resolution = resolver.resolve(ResolveFlags::default()).await?;

    assert_eq!(
        format!("{resolution}"),
        [
            "black==23.9.1",
            "click==8.1.7",
            "mypy-extensions==1.0.0",
            "packaging==23.2",
            "pathspec==0.11.2",
            "platformdirs==3.11.0"
        ]
        .join("\n")
    );

    Ok(())
}

#[tokio::test]
async fn black_colorama() -> Result<()> {
    let client = PypiClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black[colorama]<=23.9.1").unwrap()];
    let resolver = Resolver::new(requirements, &MARKERS_311, &TAGS_311, &client);
    let resolution = resolver.resolve(ResolveFlags::default()).await?;

    assert_eq!(
        format!("{resolution}"),
        [
            "black==23.9.1",
            "click==8.1.7",
            "colorama==0.4.6",
            "mypy-extensions==1.0.0",
            "packaging==23.2",
            "pathspec==0.11.2",
            "platformdirs==3.11.0"
        ]
        .join("\n")
    );

    Ok(())
}

#[tokio::test]
async fn black_python_310() -> Result<()> {
    let client = PypiClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let resolver = Resolver::new(requirements, &MARKERS_310, &TAGS_310, &client);
    let resolution = resolver.resolve(ResolveFlags::default()).await?;

    assert_eq!(
        format!("{resolution}"),
        [
            "black==23.9.1",
            "click==8.1.7",
            "mypy-extensions==1.0.0",
            "packaging==23.2",
            "pathspec==0.11.2",
            "platformdirs==3.11.0",
            "tomli==2.0.1",
            "typing-extensions==4.8.0"
        ]
        .join("\n")
    );

    Ok(())
}

#[tokio::test]
async fn htmldate() -> Result<()> {
    let client = PypiClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("htmldate<=1.5.0").unwrap()];
    let resolver = Resolver::new(requirements, &MARKERS_311, &TAGS_311, &client);
    let resolution = resolver.resolve(ResolveFlags::default()).await?;

    // Resolves to `htmldate==1.4.3` (rather than `htmldate==1.5.2`) because `htmldate==1.5.2` has
    // a dependency on `lxml` versions that don't provide universal wheels.
    assert_eq!(
        format!("{resolution}"),
        [
            "charset-normalizer==3.3.0",
            "dateparser==1.1.8",
            "htmldate==1.4.3",
            "lxml==4.9.3",
            "python-dateutil==2.8.2",
            "pytz==2023.3.post1",
            "regex==2023.10.3",
            "six==1.16.0",
            "tzlocal==5.1",
            "urllib3==2.0.6"
        ]
        .join("\n")
    );

    Ok(())
}

static MARKERS_311: Lazy<MarkerEnvironment> = Lazy::new(|| {
    MarkerEnvironment {
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
    }
});

static TAGS_311: Lazy<Tags> = Lazy::new(|| {
    Tags::from_env(
        &Platform::new(
            Os::Macos {
                major: 21,
                minor: 6,
            },
            Arch::Aarch64,
        ),
        (3, 11),
    )
    .unwrap()
});

static MARKERS_310: Lazy<MarkerEnvironment> = Lazy::new(|| {
    MarkerEnvironment {
        implementation_name: "cpython".to_string(),
        implementation_version: StringVersion::from_str("3.10.5").unwrap(),
        os_name: "posix".to_string(),
        platform_machine: "arm64".to_string(),
        platform_python_implementation: "CPython".to_string(),
        platform_release: "21.6.0".to_string(),
        platform_system: "Darwin".to_string(),
        platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000".to_string(),
        python_full_version: StringVersion::from_str("3.10.5").unwrap(),
        python_version: StringVersion::from_str("3.10").unwrap(),
        sys_platform: "darwin".to_string(),
    }
});

static TAGS_310: Lazy<Tags> = Lazy::new(|| {
    Tags::from_env(
        &Platform::new(
            Os::Macos {
                major: 21,
                minor: 6,
            },
            Arch::Aarch64,
        ),
        (3, 10),
    )
    .unwrap()
});
