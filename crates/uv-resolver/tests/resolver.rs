#![cfg(feature = "pypi")]

//! Integration tests for the resolver. These tests rely on a live network connection, and hit
//! `PyPI` directly.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use distribution_types::{IndexLocations, Resolution, SourceDist};
use pep508_rs::{MarkerEnvironment, Requirement, StringVersion};
use platform_host::{Arch, Os, Platform};
use platform_tags::Tags;
use uv_cache::Cache;
use uv_client::{FlatIndex, RegistryClientBuilder};
use uv_interpreter::{Interpreter, Virtualenv};
use uv_resolver::{
    DisplayResolutionGraph, InMemoryIndex, Manifest, Options, OptionsBuilder, PreReleaseMode,
    ResolutionGraph, ResolutionMode, Resolver,
};
use uv_traits::{BuildContext, BuildKind, NoBinary, NoBuild, SetupPyStrategy, SourceBuildTrait};

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: Lazy<DateTime<Utc>> = Lazy::new(|| {
    DateTime::parse_from_rfc3339("2023-11-18T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
});

struct DummyContext {
    cache: Cache,
    interpreter: Interpreter,
    index_locations: IndexLocations,
}

impl DummyContext {
    fn new(cache: Cache, interpreter: Interpreter) -> Self {
        Self {
            cache,
            interpreter,
            index_locations: IndexLocations::default(),
        }
    }
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

    fn no_build(&self) -> &NoBuild {
        &NoBuild::None
    }

    fn no_binary(&self) -> &NoBinary {
        &NoBinary::None
    }

    fn setup_py_strategy(&self) -> SetupPyStrategy {
        SetupPyStrategy::default()
    }

    fn index_locations(&self) -> &IndexLocations {
        &self.index_locations
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
        _dist: Option<&'a SourceDist>,
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
    options: Options,
    markers: &'static MarkerEnvironment,
    tags: &Tags,
) -> Result<ResolutionGraph> {
    let client = RegistryClientBuilder::new(Cache::temp()?).build();
    let flat_index = FlatIndex::default();
    let index = InMemoryIndex::default();
    let interpreter = Interpreter::artificial(
        Platform::current()?,
        markers.clone(),
        PathBuf::from("/dev/null"),
        PathBuf::from("/dev/null"),
        PathBuf::from("/dev/null"),
        PathBuf::from("/dev/null"),
    );
    let build_context = DummyContext::new(Cache::temp()?, interpreter.clone());
    let resolver = Resolver::new(
        manifest,
        options,
        markers,
        &interpreter,
        tags,
        &client,
        &flat_index,
        &index,
        &build_context,
    );
    Ok(resolver.resolve().await?)
}

macro_rules! assert_snapshot {
    ($value:expr, @$snapshot:literal) => {
        let snapshot = anstream::adapter::strip_str(&format!("{}", $value)).to_string();
        insta::assert_snapshot!(&snapshot, @$snapshot)
    };
}

#[tokio::test]
async fn black() -> Result<()> {
    let manifest = Manifest::simple(vec![Requirement::from_str("black<=23.9.1").unwrap()]);
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![
        Requirement::from_str("black[colorama]<=23.9.1").unwrap()
    ]);
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
    black==23.9.1
    click==8.1.7
        # via black
    colorama==0.4.6
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

/// Resolve Black with an invalid extra. The resolver should ignore the extra.
#[tokio::test]
async fn black_tensorboard() -> Result<()> {
    let manifest = Manifest::simple(vec![
        Requirement::from_str("black[tensorboard]<=23.9.1").unwrap()
    ]);
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![Requirement::from_str("black<=23.9.1").unwrap()]);
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_310, &TAGS_310).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("mypy-extensions<0.4.4").unwrap()],
        vec![],
        vec![],
        None,
        vec![],
    );
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("mypy-extensions[extra]<0.4.4").unwrap()],
        vec![],
        vec![],
        None,
        vec![],
    );
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![Requirement::from_str("flake8<1").unwrap()],
        vec![],
        vec![],
        None,
        vec![],
    );
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![Requirement::from_str("black>21").unwrap()]);
    let options = OptionsBuilder::new()
        .resolution_mode(ResolutionMode::Lowest)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![Requirement::from_str("black>21").unwrap()]);
    let options = OptionsBuilder::new()
        .resolution_mode(ResolutionMode::LowestDirect)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        vec![Requirement::from_str("black==23.9.0").unwrap()],
        None,
        vec![],
    );
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::new(
        vec![Requirement::from_str("black<=23.9.1").unwrap()],
        vec![],
        vec![],
        vec![Requirement::from_str("black==23.9.2").unwrap()],
        None,
        vec![],
    );
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![Requirement::from_str("black<=20.0").unwrap()]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::Disallow)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let err = resolve(manifest, options, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    assert_snapshot!(err, @r###"
    Because only black>20.0 is available and you require black<=20.0, we can conclude that the requirements are unsatisfiable.

    hint: Pre-releases are available for black in the requested range (e.g., 19.10b0), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    Ok(())
}

#[tokio::test]
async fn black_allow_prerelease_if_necessary() -> Result<()> {
    let manifest = Manifest::simple(vec![Requirement::from_str("black<=20.0").unwrap()]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::IfNecessary)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let err = resolve(manifest, options, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    assert_snapshot!(err, @r###"
    Because only black>20.0 is available and you require black<=20.0, we can conclude that the requirements are unsatisfiable.

    hint: Pre-releases are available for black in the requested range (e.g., 19.10b0), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    Ok(())
}

#[tokio::test]
async fn pylint_disallow_prerelease() -> Result<()> {
    let manifest = Manifest::simple(vec![Requirement::from_str("pylint==2.3.0").unwrap()]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::Disallow)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![Requirement::from_str("pylint==2.3.0").unwrap()]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::Allow)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![
        Requirement::from_str("pylint==2.3.0").unwrap(),
        Requirement::from_str("isort>=5.0.0").unwrap(),
    ]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::Explicit)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
    let manifest = Manifest::simple(vec![
        Requirement::from_str("pylint==2.3.0").unwrap(),
        Requirement::from_str("isort>=5.0.0b").unwrap(),
    ]);
    let options = OptionsBuilder::new()
        .prerelease_mode(PreReleaseMode::Explicit)
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let resolution = resolve(manifest, options, &MARKERS_311, &TAGS_311).await?;

    assert_snapshot!(DisplayResolutionGraph::from(&resolution), @r###"
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
/// fail with a pre-release-centric hint.
#[tokio::test]
async fn msgraph_sdk() -> Result<()> {
    let manifest = Manifest::simple(vec![Requirement::from_str("msgraph-sdk==1.0.0").unwrap()]);
    let options = OptionsBuilder::new()
        .exclude_newer(Some(*EXCLUDE_NEWER))
        .build();

    let err = resolve(manifest, options, &MARKERS_311, &TAGS_311)
        .await
        .unwrap_err();

    assert_snapshot!(err, @r###"
    Because only msgraph-core<1.0.0a2 is available and msgraph-sdk==1.0.0 depends on msgraph-core>=1.0.0a2, we can conclude that msgraph-sdk==1.0.0 cannot be used.
    And because you require msgraph-sdk==1.0.0, we can conclude that the requirements are unsatisfiable.

    hint: msgraph-core was requested with a pre-release marker (e.g., msgraph-core>=1.0.0a2), but pre-releases weren't enabled (try: `--prerelease=allow`)
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
        "cpython",
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
        "cpython",
        (3, 10),
    )
    .unwrap()
});
