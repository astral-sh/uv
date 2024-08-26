use std::borrow::Cow;

use platform_tags::{Tags, TagsError};
use pypi_types::ResolverMarkerEnvironment;
use uv_configuration::TargetTriple;
use uv_python::{Interpreter, PythonVersion};

pub(crate) mod check;
pub(crate) mod compile;
pub(crate) mod freeze;
pub(crate) mod install;
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
        (None, None) => interpreter.resolver_markers(),
    }
}

pub(crate) fn resolution_tags<'env>(
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    interpreter: &'env Interpreter,
) -> Result<Cow<'env, Tags>, TagsError> {
    Ok(match (python_platform, python_version.as_ref()) {
        (Some(python_platform), Some(python_version)) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (Some(python_platform), None) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            interpreter.python_tuple(),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, Some(python_version)) => Cow::Owned(Tags::from_env(
            interpreter.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, None) => Cow::Borrowed(interpreter.tags()?),
    })
}

/// Determine the tags, markers, and interpreter to use for resolution.
pub(crate) fn resolution_environment(
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    interpreter: &Interpreter,
) -> Result<(Cow<'_, Tags>, ResolverMarkerEnvironment), TagsError> {
    let tags = match (python_platform, python_version.as_ref()) {
        (Some(python_platform), Some(python_version)) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (Some(python_platform), None) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            interpreter.python_tuple(),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, Some(python_version)) => Cow::Owned(Tags::from_env(
            interpreter.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, None) => Cow::Borrowed(interpreter.tags()?),
    };

    // Apply the platform tags to the markers.
    let markers = match (python_platform, python_version) {
        (Some(python_platform), Some(python_version)) => ResolverMarkerEnvironment::from(
            python_version.markers(&python_platform.markers(interpreter.markers())),
        ),
        (Some(python_platform), None) => {
            ResolverMarkerEnvironment::from(python_platform.markers(interpreter.markers()))
        }
        (None, Some(python_version)) => {
            ResolverMarkerEnvironment::from(python_version.markers(interpreter.markers()))
        }
        (None, None) => interpreter.resolver_markers(),
    };

    Ok((tags, markers))
}
