use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::Result;

use distribution_types::{IndexLocations, InstalledDist, Resolution, SourceDist, UvRequirement};
use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_configuration::{BuildKind, NoBinary, NoBuild, SetupPyStrategy};
use uv_interpreter::{Interpreter, PythonEnvironment};

use crate::BuildIsolation;

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
/// ┌───────┴────────┐ ┌───────┴────────┐ ┌────────┴───────┐
/// │  uv-resolver   │ │  uv-installer  │ │    uv-build    │
/// └───────▲────────┘ └───────▲────────┘ └────────▲───────┘
///         │                  │                   │
///         └─────────────┐    │    ┌──────────────┘
///                    ┌──┴────┴────┴───┐
///                    │    uv-types   │
///                    └────────────────┘
/// ```
///
/// Put in a different way, the types here allow `uv-resolver` to depend on `uv-build` and
/// `uv-build` to depend on `uv-resolver` which having actual crate dependencies between
/// them.
pub trait BuildContext: Sync {
    type SourceDistBuilder: SourceBuildTrait + Send + Sync;

    /// Return a reference to the cache.
    fn cache(&self) -> &Cache;

    /// All (potentially nested) source distribution builds use the same base python and can reuse
    /// it's metadata (e.g. wheel compatibility tags).
    fn interpreter(&self) -> &Interpreter;

    /// Whether to enforce build isolation when building source distributions.
    fn build_isolation(&self) -> BuildIsolation;

    /// Whether source distribution building is disabled. This [`BuildContext::setup_build`] calls
    /// will fail in this case. This method exists to avoid fetching source distributions if we know
    /// we can't build them
    fn no_build(&self) -> &NoBuild;

    /// Whether using pre-built wheels is disabled.
    fn no_binary(&self) -> &NoBinary;

    /// The index locations being searched.
    fn index_locations(&self) -> &IndexLocations;

    /// The strategy to use when building source distributions that lack a `pyproject.toml`.
    fn setup_py_strategy(&self) -> SetupPyStrategy;

    /// Resolve the given requirements into a ready-to-install set of package versions.
    fn resolve<'a>(
        &'a self,
        requirements: &'a [UvRequirement],
    ) -> impl Future<Output = Result<Resolution>> + Send + 'a;

    /// Install the given set of package versions into the virtual environment. The environment must
    /// use the same base Python as [`BuildContext::interpreter`]
    fn install<'a>(
        &'a self,
        resolution: &'a Resolution,
        venv: &'a PythonEnvironment,
    ) -> impl Future<Output = Result<()>> + Send + 'a;

    /// Setup a source distribution build by installing the required dependencies. A wrapper for
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
        version_id: &'a str,
        dist: Option<&'a SourceDist>,
        build_kind: BuildKind,
    ) -> impl Future<Output = Result<Self::SourceDistBuilder>> + Send + 'a;
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
    fn metadata(&mut self) -> impl Future<Output = Result<Option<PathBuf>>> + Send;

    /// A wrapper for `uv_build::SourceBuild::build`.
    ///
    /// For PEP 517 builds, this calls `build_wheel`.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`.
    fn wheel<'a>(&'a self, wheel_dir: &'a Path)
        -> impl Future<Output = Result<String>> + Send + 'a;
}

/// A wrapper for [`uv_installer::SitePackages`]
pub trait InstalledPackagesProvider {
    fn iter(&self) -> impl Iterator<Item = &InstalledDist>;
    fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist>;
}

/// An [`InstalledPackagesProvider`] with no packages in it.
pub struct EmptyInstalledPackages;

impl InstalledPackagesProvider for EmptyInstalledPackages {
    fn get_packages(&self, _name: &pep508_rs::PackageName) -> Vec<&InstalledDist> {
        Vec::new()
    }

    fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        std::iter::empty()
    }
}
