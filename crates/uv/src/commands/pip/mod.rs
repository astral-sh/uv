use std::borrow::Cow;

use pep508_rs::MarkerEnvironment;
use platform_tags::{Tags, TagsError};
use uv_configuration::TargetTriple;
use uv_python::{ImplementationName, Interpreter, PythonVersion};

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

/// Determine the tags, markers, and interpreter to use for resolution.
pub(crate) fn resolution_environment(
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python_implementation: Option<ImplementationName>,
    interpreter: &Interpreter,
) -> Result<(Cow<'_, Tags>, Cow<'_, MarkerEnvironment>), TagsError> {
    let implementation_name = match python_implementation {
        Some(python_implementation) => &*python_implementation.to_string(),
        None => interpreter.implementation_name(),
    };
    let tags = match (python_platform, python_version.as_ref()) {
        (Some(python_platform), Some(python_version)) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            (python_version.major(), python_version.minor()),
            implementation_name,
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (Some(python_platform), None) => Cow::Owned(Tags::from_env(
            &python_platform.platform(),
            interpreter.python_tuple(),
            implementation_name,
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, Some(python_version)) => Cow::Owned(Tags::from_env(
            interpreter.platform(),
            (python_version.major(), python_version.minor()),
            implementation_name,
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
        (None, None) => Cow::Owned(Tags::from_env(
            interpreter.platform(),
            interpreter.python_tuple(),
            implementation_name,
            interpreter.implementation_tuple(),
            interpreter.manylinux_compatible(),
            interpreter.gil_disabled(),
        )?),
    };

    // Apply the platform tags to the markers.
    let markers = match (python_platform, python_version) {
        (Some(python_platform), Some(python_version)) => {
            Cow::Owned(python_version.markers(&python_platform.markers(interpreter.markers())))
        }
        (Some(python_platform), None) => Cow::Owned(python_platform.markers(interpreter.markers())),
        (None, Some(python_version)) => Cow::Owned(python_version.markers(interpreter.markers())),
        (None, None) => Cow::Borrowed(interpreter.markers()),
    };

    Ok((tags, markers))
}
