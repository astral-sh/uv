use std::borrow::Cow;

use uv_configuration::{BuildHashChecking, BuildHashPolicy, HashCheckingMode, TargetTriple};
use uv_platform_tags::{Tags, TagsError, TagsOptions};
use uv_pypi_types::ResolverMarkerEnvironment;
use uv_python::{Interpreter, PythonVersion};

pub(crate) mod check;
pub(crate) mod compile;
pub(crate) mod freeze;
pub(crate) mod install;
pub(crate) mod latest;
pub(crate) mod list;
pub(crate) mod loggers;
pub(crate) mod operations;
pub(crate) mod show;
pub(crate) mod sync;
pub(crate) mod tree;
pub(crate) mod uninstall;

/// Configured hash-checking options, before reading requirements files.
pub(crate) struct PipHashOptions {
    runtime: Option<HashCheckingMode>,
    build: BuildHashChecking,
}

impl PipHashOptions {
    pub(crate) fn new(runtime: Option<HashCheckingMode>, build: BuildHashChecking) -> Self {
        Self { runtime, build }
    }

    /// Resolve verification and trust after incorporating requirements-file directives.
    fn resolve(self, require_hashes: bool) -> PipHashPolicies {
        let runtime = HashCheckingMode::from_requirements_txt(self.runtime, require_hashes);
        // Runtime hash requirements do not require build hashes, but enabling runtime checking
        // also enables verification of supplied build hashes.
        let build = self
            .build
            .resolve(runtime.map(|_| HashCheckingMode::Verify));
        PipHashPolicies { runtime, build }
    }
}

/// Effective policies for runtime and build dependencies.
struct PipHashPolicies {
    runtime: Option<HashCheckingMode>,
    build: BuildHashPolicy,
}

pub(crate) fn resolution_markers(
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    interpreter: &Interpreter,
) -> ResolverMarkerEnvironment {
    match (python_platform, python_version) {
        (Some(python_platform), Some(python_version)) => ResolverMarkerEnvironment::from(
            python_version.markers(python_platform.markers(interpreter.markers().clone())),
        ),
        (Some(python_platform), None) => {
            ResolverMarkerEnvironment::from(python_platform.markers(interpreter.markers().clone()))
        }
        (None, Some(python_version)) => {
            ResolverMarkerEnvironment::from(python_version.markers(interpreter.markers().clone()))
        }
        (None, None) => interpreter.to_resolver_marker_environment(),
    }
}

pub(crate) fn resolution_tags<'env>(
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    interpreter: &'env Interpreter,
) -> Result<Cow<'env, Tags>, TagsError> {
    if python_platform.is_none() && python_version.is_none() {
        return Ok(Cow::Borrowed(interpreter.tags()?));
    }

    let (platform, manylinux_compatible) = if let Some(python_platform) = python_platform {
        (
            python_platform.platform(),
            python_platform.manylinux_compatible(),
        )
    } else {
        (
            interpreter.platform().clone(),
            interpreter.manylinux_compatible(),
        )
    };

    let version_tuple = if let Some(python_version) = python_version {
        (python_version.major(), python_version.minor())
    } else {
        interpreter.python_tuple()
    };

    let tags = Tags::from_env(
        platform,
        version_tuple,
        interpreter.implementation_name(),
        interpreter.implementation_tuple(),
        TagsOptions {
            manylinux_compatible,
            gil_disabled: interpreter.gil_disabled(),
            debug_enabled: interpreter.debug_enabled(),
            is_cross: true,
        },
    )?;
    Ok(Cow::Owned(tags))
}
