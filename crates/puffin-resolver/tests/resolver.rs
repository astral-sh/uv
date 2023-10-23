#![cfg(feature = "pypi")]

//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;

use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use puffin_client::RegistryClientBuilder;
use puffin_resolver::{ResolutionMode, Resolver};

#[tokio::test]
async fn pylint() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("pylint==2.3.0").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_colorama() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black[colorama]<=23.9.1").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_python_310() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_310,
        &TAGS_310,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a constraint on `mypy-extensions`, to ensure that constraints are
/// respected.
#[tokio::test]
async fn black_mypy_extensions() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let constraints = vec![Requirement::from_str("mypy-extensions<1").unwrap()];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a constraint on `mypy-extensions[extra]`, to ensure that extras are
/// ignored when resolving constraints.
#[tokio::test]
async fn black_mypy_extensions_extra() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let constraints = vec![Requirement::from_str("mypy-extensions[extra]<1").unwrap()];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a redundant constraint on `flake8`, to ensure that constraints don't
/// introduce new dependencies.
#[tokio::test]
async fn black_flake8() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black<=23.9.1").unwrap()];
    let constraints = vec![Requirement::from_str("flake8<1").unwrap()];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::default(),
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_lowest() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black>21").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::Lowest,
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_lowest_direct() -> Result<()> {
    colored::control::set_override(false);

    let client = RegistryClientBuilder::default().build();

    let requirements = vec![Requirement::from_str("black>21").unwrap()];
    let constraints = vec![];
    let resolver = Resolver::new(
        requirements,
        constraints,
        ResolutionMode::LowestDirect,
        &MARKERS_311,
        &TAGS_311,
        &client,
    );
    let resolution = resolver.resolve().await?;

    insta::assert_display_snapshot!(resolution);

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
