#![cfg(feature = "pypi")]

//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;

use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use distribution_types::Resolution;
use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClientBuilder;
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_resolver::{
    Manifest, PreReleaseMode, ResolutionGraph, ResolutionMode, ResolutionOptions, Resolver,
};
use puffin_traits::{BuildContext, BuildKind, SourceBuildTrait};

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: Lazy<DateTime<Utc>> = Lazy::new(|| {
    DateTime::parse_from_rfc3339("2023-11-18T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
});

struct DummyContext {
    cache: Cache,
    interpreter: Interpreter,
}

impl BuildContext for DummyContext {
    type SourceDistBuilder = DummyBuilder;

    fn cache(&self) -> &Cache {
        &self.cache
    }

    fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    fn base_python(&self) -> &Path {
        panic!("The test should not need to build source distributions")
    }

    fn resolve<'a>(
        &'a self,
        _requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = Result<Resolution>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }

    fn install<'a>(
        &'a self,
        _resolution: &'a Resolution,
        _venv: &'a Virtualenv,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }

    fn setup_build<'a>(
        &'a self,
        _source: &'a Path,
        _subdirectory: Option<&'a Path>,
        _package_id: &'a str,
        _build_kind: BuildKind,
    ) -> Pin<Box<dyn Future<Output = Result<Self::SourceDistBuilder>> + Send + 'a>> {
        Box::pin(async { Ok(DummyBuilder) })
    }
}

struct DummyBuilder;

impl SourceBuildTrait for DummyBuilder {
    fn metadata<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }

    fn wheel<'a>(
        &'a self,
        _wheel_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        panic!("The test should not need to build source distributions")
    }
}

async fn resolve(
    manifest: Manifest,
    options: ResolutionOptions,
    markers: &'static MarkerEnvironment,
    tags: &Tags,
) -> Result<ResolutionGraph> {
    let client = RegistryClientBuilder::new(Cache::temp()?).build();
    let build_context = DummyContext {
        cache: Cache::temp()?,
        interpreter: Interpreter::artificial(
            Platform::current()?,
            markers.clone(),
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
            PathBuf::from("/dev/null"),
        ),
    };
    let resolver = Resolver::new(manifest, options, markers, tags, &client, &build_context);
    Ok(resolver.resolve().await?)
}

#[tokio::test]
async fn black() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black<=23.9.1").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_colorama() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![
        Requirement::from_str("black[colorama]<=23.9.1").unwrap()
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    colorama==0.4.6
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

/// Resolve Black with an invalid extra. The resolver should ignore the extra.
#[tokio::test]
async fn black_tensorboard() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![
        Requirement::from_str("black[tensorboard]<=23.9.1").unwrap()
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_python_310() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black<=23.9.1").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_310, &TAGS_310).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    tomli==2.0.1
        # via black
    typing-extensions==4.8.0
        # via black
    "###);

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
        vec![],
        None,
        vec![],
    );
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==0.4.3
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

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
        vec![],
        None,
        vec![],
    );
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==0.4.3
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

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
        vec![],
        None,
        vec![],
    );
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_lowest() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black>21").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::Lowest,
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==22.1.0
    click==8.0.0
        # via black
    mypy-extensions==0.4.3
        # via black
    pathspec==0.9.0
        # via black
    platformdirs==2.0.0
        # via black
    tomli==1.1.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_lowest_direct() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black>21").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::LowestDirect,
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==22.1.0
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    tomli==2.0.1
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_respect_preference() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        vec![Requirement::from_str("black==23.9.0").unwrap()],
        None,
        vec![],
    );
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.0
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_ignore_preference() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        vec![Requirement::from_str("black==23.9.2").unwrap()],
        None,
        vec![],
    );
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    black==23.9.1
    click==8.1.7
        # via black
    mypy-extensions==1.0.0
        # via black
    packaging==23.2
        # via black
    pathspec==0.11.2
        # via black
    platformdirs==4.0.0
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn black_disallow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black<=20.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::Disallow,
        Some(*EXCLUDE_NEWER),
    );

    let err = resolve(manifest, options, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    insta::assert_display_snapshot!(err, @"Because there is no version of black available matching <=20.0 and root depends on black<=20.0, version solving failed.");

    Ok(())
}

#[tokio::test]
async fn black_allow_prerelease_if_requested() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black<=20.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::IfNecessary,
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    appdirs==1.4.4
        # via black
    attrs==23.1.0
        # via black
    black==19.10b0
    click==8.1.7
        # via black
    pathspec==0.11.2
        # via black
    regex==2023.10.3
        # via black
    toml==0.10.2
        # via black
    typed-ast==1.5.5
        # via black
    "###);

    Ok(())
}

#[tokio::test]
async fn pylint_disallow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("pylint==2.3.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::Disallow,
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    astroid==3.0.1
        # via pylint
    isort==5.12.0
        # via pylint
    mccabe==0.7.0
        # via pylint
    pylint==2.3.0
    "###);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_prerelease() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("pylint==2.3.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::Allow,
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    astroid==3.0.1
        # via pylint
    isort==6.0.0b2
        # via pylint
    mccabe==0.7.0
        # via pylint
    pylint==2.3.0
    "###);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_explicit_prerelease_without_marker() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![
        Requirement::from_str("pylint==2.3.0").unwrap(),
        Requirement::from_str("isort>5.13.2").unwrap(),
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::IfNecessary,
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    astroid==3.0.1
        # via pylint
    isort==6.0.0b2
        # via pylint
    mccabe==0.7.0
        # via pylint
    pylint==2.3.0
    "###);

    Ok(())
}

#[tokio::test]
async fn pylint_allow_explicit_prerelease_with_marker() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![
        Requirement::from_str("pylint==2.3.0").unwrap(),
        Requirement::from_str("isort>=6.0.0b").unwrap(),
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::IfNecessary,
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    astroid==3.0.1
        # via pylint
    isort==6.0.0b2
        # via pylint
    mccabe==0.7.0
        # via pylint
    pylint==2.3.0
    "###);

    Ok(())
}

/// Resolve `msgraph-sdk==1.0.0`, which depends on `msgraph-core>=1.0.0a2`. The resolver should
/// select the most recent pre-release version, since there's no newer stable release for
/// `msgraph-core`.
#[tokio::test]
async fn msgraph_sdk() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("msgraph-sdk==1.0.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    aiohttp==3.9.0
        # via microsoft-kiota-authentication-azure
    aiosignal==1.3.1
        # via aiohttp
    anyio==4.0.0
        # via httpx
    attrs==23.1.0
        # via aiohttp
    azure-core==1.29.5
        # via
        #   azure-identity
        #   microsoft-kiota-authentication-azure
    azure-identity==1.15.0
        # via msgraph-sdk
    certifi==2023.11.17
        # via
        #   httpcore
        #   httpx
        #   requests
    cffi==1.16.0
        # via cryptography
    charset-normalizer==3.3.2
        # via requests
    cryptography==41.0.5
        # via
        #   azure-identity
        #   msal
    deprecated==1.2.14
        # via opentelemetry-api
    frozenlist==1.4.0
        # via
        #   aiohttp
        #   aiosignal
    h11==0.14.0
        # via httpcore
    h2==4.1.0
    hpack==4.0.0
        # via h2
    httpcore==1.0.2
        # via httpx
    httpx==0.25.1
        # via
        #   microsoft-kiota-http
        #   msgraph-core
    hyperframe==6.0.1
        # via h2
    idna==3.4
        # via
        #   anyio
        #   httpx
        #   requests
        #   yarl
    importlib-metadata==6.8.0
        # via opentelemetry-api
    microsoft-kiota-abstractions==1.0.0
        # via
        #   microsoft-kiota-authentication-azure
        #   microsoft-kiota-http
        #   microsoft-kiota-serialization-json
        #   microsoft-kiota-serialization-text
        #   msgraph-core
        #   msgraph-sdk
    microsoft-kiota-authentication-azure==1.0.0
        # via msgraph-sdk
    microsoft-kiota-http==1.0.0
        # via
        #   msgraph-core
        #   msgraph-sdk
    microsoft-kiota-serialization-json==1.0.0
        # via msgraph-sdk
    microsoft-kiota-serialization-text==1.0.0
        # via msgraph-sdk
    msal==1.25.0
        # via
        #   azure-identity
        #   msal-extensions
    msal-extensions==1.0.0
        # via azure-identity
    msgraph-core==1.0.0a4
        # via msgraph-sdk
    msgraph-sdk==1.0.0
    multidict==6.0.4
        # via
        #   aiohttp
        #   yarl
    opentelemetry-api==1.21.0
        # via
        #   microsoft-kiota-abstractions
        #   microsoft-kiota-authentication-azure
        #   microsoft-kiota-http
        #   opentelemetry-sdk
    opentelemetry-sdk==1.21.0
        # via
        #   microsoft-kiota-abstractions
        #   microsoft-kiota-authentication-azure
        #   microsoft-kiota-http
    opentelemetry-semantic-conventions==0.42b0
        # via opentelemetry-sdk
    pendulum==2.1.2
        # via microsoft-kiota-serialization-json
    portalocker==2.8.2
        # via msal-extensions
    pycparser==2.21
        # via cffi
    pyjwt==2.8.0
        # via msal
    python-dateutil==2.8.2
        # via
        #   microsoft-kiota-serialization-text
        #   pendulum
    pytzdata==2020.1
        # via pendulum
    requests==2.31.0
        # via
        #   azure-core
        #   msal
    six==1.16.0
        # via
        #   azure-core
        #   python-dateutil
    sniffio==1.3.0
        # via
        #   anyio
        #   httpx
    std-uritemplate==0.0.46
        # via microsoft-kiota-abstractions
    typing-extensions==4.8.0
        # via
        #   azure-core
        #   opentelemetry-sdk
    urllib3==2.1.0
        # via requests
    wrapt==1.16.0
        # via deprecated
    yarl==1.9.2
        # via aiohttp
    zipp==3.17.0
        # via importlib-metadata
    "###);

    Ok(())
}

/// Resolve `msgraph-core>=0.2.2`. All later releases are pre-releases, so the resolver should
/// select the latest non-pre-release version.
#[tokio::test]
async fn msgraph_core() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("msgraph-core>=0.2.2").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::default(),
        Some(*EXCLUDE_NEWER),
    );

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    insta::assert_display_snapshot!(resolution, @r###"
    msgraph-core==0.2.2
    "###);

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
