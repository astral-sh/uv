use uv_command_support::Printer;
use uv_resolve_operations::ExtrasWithoutSourceError;

use uv_errors::{Hints, collect_hint};

/// Format an error chain with the default user-facing hints and output settings.
pub(crate) fn write_error_chain(err: &anyhow::Error, printer: Printer) -> std::fmt::Result {
    uv_errors::write_error_chain_with_options(
        err.as_ref(),
        &hints_for_error(err),
        uv_errors::ErrorOptions::default().with_stream(printer.stderr_important()),
    )
}

/// Walk an error chain and combine command-owned and shared workflow hints.
pub(crate) fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    for cause in err.chain() {
        hints.extend(uv_project_commands::error_hints(cause));
        hints.extend(uv_tool_commands::error_hints(cause));
        hints.extend(uv_pip_commands::error_hints(cause));
        hints.extend(uv_python_commands::error_hints(cause));
        hints.extend(uv_build_commands::error_hints(cause));
        collect_hint::<Box<uv_resolver::NoSolutionError>>(cause, &mut hints);
        collect_hint::<uv_resolver::NoSolutionError>(cause, &mut hints);
        collect_hint::<uv_resolver::ResolveError>(cause, &mut hints);
        collect_hint::<uv_lock::LockError>(cause, &mut hints);
        collect_hint::<uv_lock_operations::LockError>(cause, &mut hints);
        collect_hint::<uv_resolve_operations::Error>(cause, &mut hints);
        collect_hint::<uv_install_operations::Error>(cause, &mut hints);
        collect_hint::<ExtrasWithoutSourceError>(cause, &mut hints);
        collect_hint::<uv_environment_operations::EnvironmentError>(cause, &mut hints);
        collect_hint::<uv_python_context::PythonContextError>(cause, &mut hints);
        collect_hint::<uv_build_backend::Error>(cause, &mut hints);
        collect_hint::<uv_build_frontend::Error>(cause, &mut hints);
        collect_hint::<uv_python::Error>(cause, &mut hints);
        collect_hint::<uv_installer::IncompatibleWheelError>(cause, &mut hints);
        collect_hint::<uv_distribution::Error>(cause, &mut hints);
        collect_hint::<uv_python::BrokenLink>(cause, &mut hints);
        collect_hint::<uv_lock::PylockTomlError>(cause, &mut hints);
        collect_hint::<uv_requirements_txt::MakeEditableError>(cause, &mut hints);
        collect_hint::<uv_python::InterpreterError>(cause, &mut hints);
        collect_hint::<uv_workspace::pyproject::SourceError>(cause, &mut hints);
        collect_hint::<uv_distribution::LoweringError>(cause, &mut hints);
        collect_hint::<uv_virtualenv::Error>(cause, &mut hints);
        collect_hint::<uv_client::Error>(cause, &mut hints);
        #[cfg(not(feature = "self-update"))]
        collect_hint::<crate::ExternallyInstalledError>(cause, &mut hints);
    }
    hints
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use uv_lock_operations::LockError;
    use uv_settings::{LockedFlag, LockedSource};
    use uv_workspace::pyproject::{PyprojectTomlError, SourceError};

    use super::hints_for_error;

    #[test]
    fn collects_source_hints_through_pyproject_errors() {
        let err = anyhow::Error::new(PyprojectTomlError::Source(SourceError::OverlappingMarkers(
            "sys_platform == 'win32'".to_string(),
            "python_version == '3.12'".to_string(),
            "python_version != '3.12'".to_string(),
        )));

        let hints = hints_for_error(&err);
        assert_debug_snapshot!(hints.iter().collect::<Vec<_>>(), @r#"
        [
            "replace `python_version == '3.12'` with `python_version != '3.12'`",
        ]
        "#);
    }

    #[test]
    fn collects_lock_hints_through_context() {
        let error =
            LockError::LockFormat("uv.lock".into(), 3, LockedSource::Cli(LockedFlag::Check));

        // Command context retains the lockfile's regeneration hint.
        let error = anyhow::Error::new(error).context("Failed to check the lockfile");

        let hints = hints_for_error(&error);
        assert_debug_snapshot!(hints.iter().collect::<Vec<_>>(), @r#"
        [
            "To regenerate the lockfile, run `uv lock --refresh --preview-features lockfile-format-check`.",
        ]
        "#);
    }
}
