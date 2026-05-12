use std::borrow::Cow;

use uv_configuration::TargetTriple;
use uv_platform_tags::{Tags, TagsError};
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

pub(crate) fn resolution_markers(
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    interpreter: &Interpreter,
) -> ResolverMarkerEnvironment {
    match (python_platform, python_version) {
        (Some(python_platform), Some(python_version)) => ResolverMarkerEnvironment::from(
            python_version.markers(&python_platform.markers(interpreter.markers())),
        ),
        (Some(python_platform), None) => {
            ResolverMarkerEnvironment::from(python_platform.markers(interpreter.markers()))
        }
        (None, Some(python_version)) => {
            ResolverMarkerEnvironment::from(python_version.markers(interpreter.markers()))
        }
        (None, None) => interpreter.resolver_marker_environment(),
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
            &python_platform.platform(),
            python_platform.manylinux_compatible(),
        )
    } else {
        (interpreter.platform(), interpreter.manylinux_compatible())
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
        manylinux_compatible,
        interpreter.gil_disabled(),
        true,
    )?;
    Ok(Cow::Owned(tags))
}
