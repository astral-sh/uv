#![cfg(feature = "pypi")]

//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;
use tempfile::tempdir;

use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use puffin_client::RegistryClientBuilder;
use puffin_interpreter::{InterpreterInfo, Virtualenv};
use puffin_resolver::{Graph, Manifest, PreReleaseMode, ResolutionMode, Resolver};
use puffin_traits::BuildContext;

struct DummyContext {
    interpreter_info: InterpreterInfo,
}

impl BuildContext for DummyContext {
    fn cache(&self) -> &Path {
        panic!("The test should not need to build source distributions")
    }

    fn interpreter_info(&self) -> &InterpreterInfo {
        &self.interpreter_info
    }

    fn base_python(&self) -> &Path {
        panic!("The test should not need to build source distributions")
    }

    fn resolve<'a>(
        &'a self,
        _requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Requirement>>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }

    fn install<'a>(
        &'a self,
        _requirements: &'a [Requirement],
        _venv: &'a Virtualenv,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }

    fn build_source<'a>(
        &'a self,
        _sdist: &'a Path,
        _subdirectory: Option<&'a Path>,
        _wheel_dir: &'a Path,
        _package_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }
}

async fn resolve(
    manifest: Manifest,
    markers: &'static MarkerEnvironment,
    tags: &Tags,
) -> Result<Graph> {
    let temp_dir = tempdir()?;
    let client = RegistryClientBuilder::new(temp_dir.path()).build();
    let build_context = DummyContext {
        interpreter_info: InterpreterInfo::artificial(
            Platform::current()?,
            markers.clone(),
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
        ),
    };
    let resolver = Resolver::new(manifest, markers, tags, &client, &build_context);
    Ok(resolver.resolve().await?)
}

#[tokio::test]
async fn black() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_colorama() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black[colorama]<=23.9.1").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_python_310() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_310, &TAGS_310).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a constraint on `mypy-extensions`, to ensure that constraints are
/// respected.
#[tokio::test]
async fn black_mypy_extensions() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("mypy-extensions<0.4.4").unwrap()],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a constraint on `mypy-extensions[extra]`, to ensure that extras are
/// ignored when resolving constraints.
#[tokio::test]
async fn black_mypy_extensions_extra() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("mypy-extensions[extra]<0.4.4").unwrap()],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

/// Resolve `black` with a redundant constraint on `flake8`, to ensure that constraints don't
/// introduce new dependencies.
#[tokio::test]
async fn black_flake8() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("flake8<1").unwrap()],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_lowest() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black>21").unwrap()],
        vec![],
        vec![],
        ResolutionMode::Lowest,
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_lowest_direct() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black>21").unwrap()],
        vec![],
        vec![],
        ResolutionMode::LowestDirect,
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_respect_preference() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![Requirement::from_str("black==23.9.0").unwrap()],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_ignore_preference() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![Requirement::from_str("black==23.9.2").unwrap()],
        ResolutionMode::default(),
        PreReleaseMode::default(),
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn black_disallow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=20.0").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::Disallow,
        None,
        None,
    );

    let err = resolve(manifest, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    insta::assert_display_snapshot!(err);

    Ok(())
}

#[tokio::test]
async fn black_allow_prerelease_if_necessary() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=20.0").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::IfNecessary,
        None,
        None,
    );

    let err = resolve(manifest, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    insta::assert_display_snapshot!(err);

    Ok(())
}

#[tokio::test]
async fn pylint_disallow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("pylint==2.3.0").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::Disallow,
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("pylint==2.3.0").unwrap()],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::Allow,
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_explicit_prerelease_without_marker() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![
            Requirement::from_str("pylint==2.3.0").unwrap(),
            Requirement::from_str("isort>=5.0.0").unwrap(),
        ],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::Explicit,
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_explicit_prerelease_with_marker() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![
            Requirement::from_str("pylint==2.3.0").unwrap(),
            Requirement::from_str("isort>=5.0.0b").unwrap(),
        ],
        vec![],
        vec![],
        ResolutionMode::default(),
        PreReleaseMode::Explicit,
        None,
        None,
    );

    let resolution = resolve(manifest, &MARKERS_311, &TAGS_311).await?;

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
