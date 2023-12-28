#![cfg(feature = "pypi")]

//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::path::{Path, PathBuf};
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

    async fn resolve<'a>(&'a self, _requirements: &'a [Requirement]) -> Result<Resolution> {
        panic!("The test should not need to build source distributions")
    }

    async fn install<'a>(
        &'a self,
        _resolution: &'a Resolution,
        _venv: &'a Virtualenv,
    ) -> Result<()> {
        panic!("The test should not need to build source distributions")
    }

    async fn setup_build<'a>(
        &'a self,
        _source: &'a Path,
        _subdirectory: Option<&'a Path>,
        _package_id: &'a str,
        _build_kind: BuildKind,
    ) -> Result<Self::SourceDistBuilder> {
        Ok(DummyBuilder)
    }
}

struct DummyBuilder;

impl SourceBuildTrait for DummyBuilder {
    async fn metadata(&mut self) -> Result<Option<PathBuf>> {
        panic!("The test should not need to build source distributions")
    }

    async fn wheel<'a>(&'a self, _wheel_dir: &'a Path) -> Result<String> {
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
async fn black_allow_prerelease_if_necessary() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![Requirement::from_str("black<=20.0").unwrap()]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::IfNecessary,
        Some(*EXCLUDE_NEWER),
    );

    let err = resolve(manifest, options, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    insta::assert_display_snapshot!(err, @"Because there is no version of black available matching <=20.0 and root depends on black<=20.0, version solving failed.");

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
        Requirement::from_str("isort>=5.0.0").unwrap(),
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::Explicit,
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
async fn pylint_allow_explicit_prerelease_with_marker() -> Result<()> {
    colored::control::set_override(false);

    let manifest = Manifest::simple(vec![
        Requirement::from_str("pylint==2.3.0").unwrap(),
        Requirement::from_str("isort>=5.0.0b").unwrap(),
    ]);
    let options = ResolutionOptions::new(
        ResolutionMode::default(),
        PreReleaseMode::Explicit,
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
