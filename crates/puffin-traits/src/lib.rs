//! Avoid cyclic crate dependencies between resolver, installer and builder.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use anyhow::Result;

use pep508_rs::Requirement;
use puffin_interpreter::{Interpreter, Virtualenv};

/// Avoid cyclic crate dependencies between resolver, installer and builder.
///
/// To resolve the dependencies of a packages, we may need to build one or more source
/// distributions. To building a source distribution, we need to create a virtual environment from
/// the same base python as we use for the root resolution, resolve the build requirements
/// (potentially which nested source distributions, recursing a level deeper), installing
/// them and then build. The installer, the resolver and the source distribution builder are each in
/// their own crate. To avoid circular crate dependencies, this type dispatches between the three
/// crates with its three main methods ([`BuildContext::resolve`], [`BuildContext::install`] and
/// [`BuildContext::build_source`]).
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
    // TODO(konstin): Add a cache abstraction
    fn cache(&self) -> &Path;

    /// All (potentially nested) source distribution builds use the same base python and can reuse
    /// it's metadata (e.g. wheel compatibility tags).
    fn interpreter(&self) -> &Interpreter;

    /// The system (or conda) python interpreter to create venvs.
    fn base_python(&self) -> &Path;

    /// Whether source distribution building is disabled. This [`BuildContext::build_source`] calls
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

    /// Build a source distribution into a wheel from an archive.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`.
    ///
    /// `package_id` is for error reporting only.
    fn build_source<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        wheel_dir: &'a Path,
        package_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}
