//! Avoid cyclic crate dependencies between resolver, installer and builder.

use std::fmt::{Display, Formatter};
use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::Result;

use distribution_types::{CachedDist, DistributionId, Resolution};
use once_map::OnceMap;
use pep508_rs::Requirement;
use puffin_cache::Cache;
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_normalize::PackageName;

/// Avoid cyclic crate dependencies between resolver, installer and builder.
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
///                    │puffin          │
///                    └───────▲────────┘
///                            │
///                            │
///                    ┌───────┴────────┐
///         ┌─────────►│puffin-dispatch │◄─────────┐
///         │          └───────▲────────┘          │
///         │                  │                   │
///         │                  │                   │
/// ┌───────┴────────┐ ┌───────┴────────┐ ┌────────┴───────┐
/// │puffin-resolver │ │puffin-installer│ │puffin-build    │
/// └───────▲────────┘ └───────▲────────┘ └────────▲───────┘
///         │                  │                   │
///         └─────────────┐    │    ┌──────────────┘
///                    ┌──┴────┴────┴───┐
///                    │puffin-traits   │
///                    └────────────────┘
/// ```
///
/// Put in a different way, this trait allows `puffin-resolver` to depend on `puffin-build` and
/// `puffin-build` to depend on `puffin-resolver` which having actual crate dependencies between
/// them.

// TODO(konstin): Proper error types
pub trait BuildContext: Sync {
    type SourceDistBuilder: SourceBuildTrait + Send + Sync;

    fn cache(&self) -> &Cache;

    /// All (potentially nested) source distribution builds use the same base python and can reuse
    /// it's metadata (e.g. wheel compatibility tags).
    fn interpreter(&self) -> &Interpreter;

    /// The system (or conda) python interpreter to create venvs.
    fn base_python(&self) -> &Path;

    /// Whether source distribution building is disabled. This [`BuildContext::setup_build`] calls
    /// will fail in this case. This method exists to avoid fetching source distributions if we know
    /// we can't build them
    fn no_build(&self) -> bool;

    /// Whether using pre-built wheels is disabled.
    fn no_binary(&self) -> &NoBinary;

    /// The strategy to use when building source distributions that lack a `pyproject.toml`.
    fn setup_py_strategy(&self) -> SetupPyStrategy;

    /// Resolve the given requirements into a ready-to-install set of package versions.
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> impl Future<Output = Result<Resolution>> + Send + 'a;

    /// Install the given set of package versions into the virtual environment. The environment must
    /// use the same base python as [`BuildContext::base_python`]
    fn install<'a>(
        &'a self,
        resolution: &'a Resolution,
        venv: &'a Virtualenv,
    ) -> impl Future<Output = Result<()>> + Send + 'a;

    /// Setup a source distribution build by installing the required dependencies. A wrapper for
    /// `puffin_build::SourceBuild::setup`.
    ///
    /// For PEP 517 builds, this calls `get_requires_for_build_wheel`.
    ///
    /// `package_id` is for error reporting only.
    fn setup_build<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        package_id: &'a str,
        build_kind: BuildKind,
    ) -> impl Future<Output = Result<Self::SourceDistBuilder>> + Send + 'a;
}

/// A wrapper for `puffin_build::SourceBuild` to avoid cyclical crate dependencies.
///
/// You can either call only `wheel()` to build the wheel directly, call only `metadata()` to get
/// the metadata without performing the actual or first call `metadata()` and then `wheel()`.
pub trait SourceBuildTrait {
    /// A wrapper for `puffin_build::SourceBuild::get_metadata_without_build`.
    ///
    /// For PEP 517 builds, this calls `prepare_metadata_for_build_wheel`
    ///
    /// Returns the metadata directory if we're having a PEP 517 build and the
    /// `prepare_metadata_for_build_wheel` hook exists
    fn metadata(&mut self) -> impl Future<Output = Result<Option<PathBuf>>> + Send;

    /// A wrapper for `puffin_build::SourceBuild::build`.
    ///
    /// For PEP 517 builds, this calls `build_wheel`.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`.
    fn wheel<'a>(&'a self, wheel_dir: &'a Path)
        -> impl Future<Output = Result<String>> + Send + 'a;
}

#[derive(Default)]
pub struct InFlight {
    /// The in-flight distribution downloads.
    pub downloads: OnceMap<DistributionId, Result<CachedDist, String>>,
}

/// The strategy to use when building source distributions that lack a `pyproject.toml`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SetupPyStrategy {
    /// Perform a PEP 517 build.
    #[default]
    Pep517,
    /// Perform a build by invoking `setuptools` directly.
    Setuptools,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildKind {
    /// A regular PEP 517 wheel build
    #[default]
    Wheel,
    /// A PEP 660 editable installation wheel build
    Editable,
}

impl Display for BuildKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildKind::Wheel => f.write_str("wheel"),
            BuildKind::Editable => f.write_str("editable"),
        }
    }
}

#[derive(Debug)]
pub enum NoBinary {
    /// Allow installation of any wheel.
    None,

    /// Do not allow installation from any wheels.
    All,

    /// Do not allow installation from the specific wheels.
    Packages(Vec<PackageName>),
}

impl NoBinary {
    /// Determine the binary installation strategy to use.
    pub fn from_args(no_binary: bool, no_binary_package: Vec<PackageName>) -> Self {
        if no_binary {
            Self::All
        } else if !no_binary_package.is_empty() {
            Self::Packages(no_binary_package)
        } else {
            Self::None
        }
    }
}
