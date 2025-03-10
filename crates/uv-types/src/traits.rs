use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rustc_hash::FxHashSet;

use uv_cache::Cache;
use uv_configuration::{BuildKind, BuildOptions, BuildOutput, ConfigSettings, SourceStrategy};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{
    CachedDist, DependencyMetadata, DistributionId, IndexCapabilities, IndexLocations,
    InstalledDist, IsBuildBackendError, Resolution, SourceDist,
};
use uv_git::GitResolver;
use uv_pep508::PackageName;
use uv_pypi_types::Requirement;
use uv_python::{Interpreter, PythonEnvironment};
use uv_workspace::WorkspaceCache;

///  Avoids cyclic crate dependencies between resolver, installer and builder.
///
/// To resolve the dependencies of a packages, we may need to build one or more source
/// distributions. To building a source distribution, we need to create a virtual environment from
/// the same base python as we use for the root resolution, resolve the build requirements
/// (potentially which nested source distributions, recursing a level deeper), installing
/// them and then build. The installer, the resolver and the source distribution builder are each in
/// their own crate. To avoid circular crate dependencies, this type dispatches between the three
/// crates with its three main methods ([`BuildContext::resolve`], [`BuildContext::install`] and
/// [`BuildContext::setup_build`]).
///
/// The overall main crate structure looks like this:
///
/// ```text
///                    ┌────────────────┐
///                    │       uv       │
///                    └───────▲────────┘
///                            │
///                            │
///                    ┌───────┴────────┐
///         ┌─────────►│  uv-dispatch   │◄─────────┐
///         │          └───────▲────────┘          │
///         │                  │                   │
///         │                  │                   │
/// ┌───────┴────────┐ ┌───────┴────────┐ ┌────────┴────────────────┐
/// │  uv-resolver   │ │  uv-installer  │ │    uv-build-frontend    │
/// └───────▲────────┘ └───────▲────────┘ └────────▲────────────────┘
///         │                  │                   │
///         └─────────────┐    │    ┌──────────────┘
///                    ┌──┴────┴────┴───┐
///                    │    uv-types    │
///                    └────────────────┘
/// ```
///
/// Put in a different way, the types here allow `uv-resolver` to depend on `uv-build` and
/// `uv-build-frontend` to depend on `uv-resolver` without having actual crate dependencies between
/// them.
pub trait BuildContext {
    type SourceDistBuilder: SourceBuildTrait;

    /// Return a reference to the interpreter.
    fn interpreter(&self) -> &Interpreter;

    /// Return a reference to the cache.
    fn cache(&self) -> &Cache;

    /// Return a reference to the Git resolver.
    fn git(&self) -> &GitResolver;

    /// Return a reference to the discovered registry capabilities.
    fn capabilities(&self) -> &IndexCapabilities;

    /// Return a reference to any pre-defined static metadata.
    fn dependency_metadata(&self) -> &DependencyMetadata;

    /// Whether source distribution building or pre-built wheels is disabled.
    ///
    /// This [`BuildContext::setup_build`] calls will fail if builds are disabled.
    /// This method exists to avoid fetching source distributions if we know we can't build them.
    fn build_options(&self) -> &BuildOptions;

    /// The [`ConfigSettings`] used to build distributions.
    fn config_settings(&self) -> &ConfigSettings;

    /// Whether to incorporate `tool.uv.sources` when resolving requirements.
    fn sources(&self) -> SourceStrategy;

    /// The index locations being searched.
    fn locations(&self) -> &IndexLocations;

    /// Workspace discovery caching.
    fn workspace_cache(&self) -> &WorkspaceCache;

    /// Resolve the given requirements into a ready-to-install set of package versions.
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
        build_stack: &'a BuildStack,
    ) -> impl Future<Output = Result<Resolution, impl IsBuildBackendError>> + 'a;

    /// Install the given set of package versions into the virtual environment. The environment must
    /// use the same base Python as [`BuildContext::interpreter`]
    fn install<'a>(
        &'a self,
        resolution: &'a Resolution,
        venv: &'a PythonEnvironment,
        build_stack: &'a BuildStack,
    ) -> impl Future<Output = Result<Vec<CachedDist>, impl IsBuildBackendError>> + 'a;

    /// Set up a source distribution build by installing the required dependencies. A wrapper for
    /// `uv_build::SourceBuild::setup`.
    ///
    /// For PEP 517 builds, this calls `get_requires_for_build_wheel`.
    ///
    /// `version_id` is for error reporting only.
    /// `dist` is for safety checks and may be null for editable builds.
    fn setup_build<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        install_path: &'a Path,
        version_id: Option<&'a str>,
        dist: Option<&'a SourceDist>,
        sources: SourceStrategy,
        build_kind: BuildKind,
        build_output: BuildOutput,
        build_stack: BuildStack,
    ) -> impl Future<Output = Result<Self::SourceDistBuilder, impl IsBuildBackendError>> + 'a;

    /// Build by calling directly into the uv build backend without PEP 517, if possible.
    ///
    /// Checks if the source tree uses uv as build backend. If not, it returns `Ok(None)`, otherwise
    /// it builds and returns the name of the built file.
    ///
    /// `version_id` is for error reporting only.
    fn direct_build<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        output_dir: &'a Path,
        build_kind: BuildKind,
        version_id: Option<&'a str>,
    ) -> impl Future<Output = Result<Option<DistFilename>, impl IsBuildBackendError>> + 'a;
}

/// A wrapper for `uv_build::SourceBuild` to avoid cyclical crate dependencies.
///
/// You can either call only `wheel()` to build the wheel directly, call only `metadata()` to get
/// the metadata without performing the actual or first call `metadata()` and then `wheel()`.
pub trait SourceBuildTrait {
    /// A wrapper for `uv_build::SourceBuild::get_metadata_without_build`.
    ///
    /// For PEP 517 builds, this calls `prepare_metadata_for_build_wheel`
    ///
    /// Returns the metadata directory if we're having a PEP 517 build and the
    /// `prepare_metadata_for_build_wheel` hook exists
    fn metadata(&mut self) -> impl Future<Output = Result<Option<PathBuf>, AnyErrorBuild>>;

    /// A wrapper for `uv_build::SourceBuild::build`.
    ///
    /// For PEP 517 builds, this calls `build_wheel`.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`. The filename is a
    /// string and not a `WheelFilename` because the on disk filename might not be normalized in the
    /// same way as uv would.
    fn wheel<'a>(
        &'a self,
        wheel_dir: &'a Path,
    ) -> impl Future<Output = Result<String, AnyErrorBuild>> + 'a;
}

/// A wrapper for [`uv_installer::SitePackages`]
pub trait InstalledPackagesProvider: Clone + Send + Sync + 'static {
    fn iter(&self) -> impl Iterator<Item = &InstalledDist>;
    fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist>;
}

/// An [`InstalledPackagesProvider`] with no packages in it.
#[derive(Clone)]
pub struct EmptyInstalledPackages;

impl InstalledPackagesProvider for EmptyInstalledPackages {
    fn get_packages(&self, _name: &PackageName) -> Vec<&InstalledDist> {
        Vec::new()
    }

    fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        std::iter::empty()
    }
}

/// [`anyhow::Error`]-like wrapper type for [`BuildDispatch`] method return values, that also makes
/// [`IsBuildBackendError`] work as [`thiserror`] `#[source]`.
///
/// The errors types have the same problem as [`BuildDispatch`] generally: The `uv-resolver`,
/// `uv-installer` and `uv-build-frontend` error types all reference each other:
/// Resolution and installation may need to build packages, while the build frontend needs to
/// resolve and install for the PEP 517 build environment.
///
/// Usually, [`anyhow::Error`] is opaque error type of choice. In this case though, we error type
/// that we can inspect on whether it's a build backend error with [`IsBuildBackendError`], and
/// [`anyhow::Error`] does not allow attaching more traits. The next choice would be
/// `Box<dyn std::error::Error + IsBuildFrontendError + Send + Sync + 'static>`, but [`thiserror`]
/// complains about the internal `AsDynError` not being implemented when being used as `#[source]`.
/// This struct is an otherwise transparent error wrapper that thiserror recognizes.
pub struct AnyErrorBuild(Box<dyn IsBuildBackendError>);

impl Debug for AnyErrorBuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for AnyErrorBuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl std::error::Error for AnyErrorBuild {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }

    #[allow(deprecated)]
    fn description(&self) -> &str {
        self.0.description()
    }

    #[allow(deprecated)]
    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.0.cause()
    }
}

impl<T: IsBuildBackendError> From<T> for AnyErrorBuild {
    fn from(err: T) -> Self {
        Self(Box::new(err))
    }
}

impl Deref for AnyErrorBuild {
    type Target = dyn IsBuildBackendError;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The stack of packages being built.
#[derive(Debug, Clone, Default)]
pub struct BuildStack(FxHashSet<DistributionId>);

impl BuildStack {
    /// Return an empty stack.
    pub fn empty() -> Self {
        Self(FxHashSet::default())
    }

    pub fn contains(&self, id: &DistributionId) -> bool {
        self.0.contains(id)
    }

    /// Push a package onto the stack.
    pub fn insert(&mut self, id: DistributionId) -> bool {
        self.0.insert(id)
    }
}
