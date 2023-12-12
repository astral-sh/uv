//! Avoid cyclic crate dependencies between resolver, installer and builder.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Result;

pub use once_map::OnceMap;
use pep508_rs::Requirement;
use puffin_cache::Cache;
use puffin_interpreter::{Interpreter, Virtualenv};

mod once_map;

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
///                    │puffin-cli      │
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
pub trait BuildContext {
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
    fn no_build(&self) -> bool {
        false
    }

    /// Resolve the given requirements into a ready-to-install set of package versions.
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Requirement>>> + Send + 'a>>;

    /// Install the given set of package versions into the virtual environment. The environment must
    /// use the same base python as [`BuildContext::base_python`]
    fn install<'a>(
        &'a self,
        requirements: &'a [Requirement],
        venv: &'a Virtualenv,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

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
    ) -> Pin<Box<dyn Future<Output = Result<Self::SourceDistBuilder>> + Send + 'a>>;
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
    fn metadata<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>>> + Send + 'a>>;

    /// A wrapper for `puffin_build::SourceBuild::build`.
    ///
    /// For PEP 517 builds, this calls `build_wheel`.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`.
    fn wheel<'a>(
        &'a self,
        wheel_dir: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}
