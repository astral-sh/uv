use std::error::Error;
use std::str::FromStr;
use std::sync::LazyLock;

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use version_ranges::Ranges;

use uv_distribution_types::{DerivationChain, DerivationStep};
use uv_errors::{Diagnostic, Hinted, Hints};
use uv_normalize::PackageName;
use uv_pep440::{Version, strip_local_version_sentinels};

use crate::commands::pip;
use crate::commands::pip::install::ExternallyManagedError;
use crate::commands::pip::operations::ExtrasWithoutSourceError;
use crate::commands::project::ProjectError;
use crate::commands::project::add::AddDependencyError;
use crate::commands::project::remove::DependencyNotFoundError;
use crate::commands::project::run::RecursionLimitError;
use crate::commands::project::version::MissingProjectVersionError;
use crate::commands::python::install::InvalidUpgradeRequestError;
use crate::commands::tool::common::NoExecutablesError;
use crate::commands::tool::run::{ToolRunScriptError, ToolRunUsageError};
use crate::printer::Printer;

static SUGGESTIONS: LazyLock<FxHashMap<PackageName, PackageName>> = LazyLock::new(|| {
    let suggestions: Vec<(String, String)> =
        serde_json::from_str(include_str!("suggestions.json")).unwrap();
    suggestions
        .iter()
        .map(|(k, v)| {
            (
                PackageName::from_str(k).unwrap(),
                PackageName::from_str(v).unwrap(),
            )
        })
        .collect()
});

/// Format an error chain with the default user-facing hints and output settings.
pub(crate) fn write_error_chain(err: &anyhow::Error, printer: Printer) -> std::fmt::Result {
    uv_errors::write_error_chain_with_options(
        err.as_ref(),
        &hints_for_error(err),
        uv_errors::ErrorOptions::default()
            .with_diagnostic(diagnostic_for_error)
            .with_stream(printer.stderr_important()),
    )
}

/// Resolve presentation data without changing an error or its source chain.
pub(crate) fn diagnostic_for_error<'a>(error: &'a (dyn Error + 'static)) -> Option<Diagnostic<'a>> {
    uv_settings::diagnostic_for_error(error)
        .or_else(|| uv_workspace::pyproject::diagnostic_for_error(error))
        .or_else(|| uv_pypi_types::diagnostic_for_error(error))
        .or_else(|| uv_publish::diagnostic_for_error(error))
}

/// Walk an error chain and collect hint strings from all known error types.
///
/// This is the central "hint for error" function. It walks the full error chain
/// (via `anyhow::Error::chain`) and tries to downcast each error to known types
/// that implement [`Hinted`]. All hint rendering logic should be consolidated here.
pub(crate) fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    for cause in err.chain() {
        collect_hint::<AddDependencyError>(cause, &mut hints);
        collect_hint::<ToolRunUsageError>(cause, &mut hints);
        collect_hint::<Box<uv_resolver::NoSolutionError>>(cause, &mut hints);
        collect_hint::<uv_resolver::NoSolutionError>(cause, &mut hints);
        collect_hint::<uv_resolver::ResolveError>(cause, &mut hints);
        collect_hint::<uv_resolver::LockError>(cause, &mut hints);
        collect_hint::<pip::operations::Error>(cause, &mut hints);
        collect_hint::<ToolRunScriptError>(cause, &mut hints);
        collect_hint::<RecursionLimitError>(cause, &mut hints);
        collect_hint::<DependencyNotFoundError>(cause, &mut hints);
        collect_hint::<ExtrasWithoutSourceError>(cause, &mut hints);
        collect_hint::<ProjectError>(cause, &mut hints);
        collect_hint::<NoExecutablesError>(cause, &mut hints);
        collect_hint::<ExternallyManagedError>(cause, &mut hints);
        collect_hint::<MissingProjectVersionError>(cause, &mut hints);
        collect_hint::<InvalidUpgradeRequestError>(cause, &mut hints);
        collect_hint::<crate::commands::build_frontend::Error>(cause, &mut hints);
        collect_hint::<uv_build_backend::Error>(cause, &mut hints);
        collect_hint::<uv_build_frontend::Error>(cause, &mut hints);
        collect_hint::<uv_python::Error>(cause, &mut hints);
        collect_hint::<uv_installer::IncompatibleWheelError>(cause, &mut hints);
        collect_hint::<uv_distribution::Error>(cause, &mut hints);
        collect_hint::<uv_python::BrokenLink>(cause, &mut hints);
        collect_hint::<uv_resolver::PylockTomlError>(cause, &mut hints);
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

/// If `cause` can be downcast to `T`, collect its hints.
fn collect_hint<T: Hinted + std::error::Error + 'static>(
    cause: &(dyn std::error::Error + 'static),
    hints: &mut Hints<'static>,
) {
    if let Some(inner) = cause.downcast_ref::<T>() {
        hints.extend(inner.hints());
    }
}

/// Format package context that should follow a distribution error as hints.
pub(crate) fn dist_hints(
    name: &PackageName,
    version: Option<&Version>,
    chain: &DerivationChain,
    cause_hints: Hints<'_>,
) -> Hints<'static> {
    let mut hints = Hints::none();
    if let Some(suggestion) = SUGGESTIONS.get(name) {
        hints.push(format!(
            "`{}` is often confused for `{}`. Did you mean to install `{}` instead?",
            name.cyan(),
            suggestion.cyan(),
            suggestion.cyan(),
        ));
    } else if !chain.is_empty() {
        hints.push(format_chain(name, version, chain));
    }
    hints.extend(cause_hints);
    hints.into_owned()
}

/// Format a [`DerivationChain`] as a human-readable error message.
fn format_chain(name: &PackageName, version: Option<&Version>, chain: &DerivationChain) -> String {
    /// Format a step in the [`DerivationChain`] as a human-readable error message.
    fn format_step(step: &DerivationStep, range: Option<Ranges<Version>>) -> String {
        if let Some(range) =
            range.filter(|range| *range != Ranges::empty() && *range != Ranges::full())
        {
            if let Some(extra) = &step.extra {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask[dotenv]>=1.0.0` (v1.2.3)
                    format!(
                        "`{}{}` ({})",
                        format!("{}[{}]", step.name, extra).cyan(),
                        range.cyan(),
                        format!("v{version}").cyan(),
                    )
                } else {
                    // Ex) `flask[dotenv]>=1.0.0`
                    format!(
                        "`{}{}`",
                        format!("{}[{}]", step.name, extra).cyan(),
                        range.cyan(),
                    )
                }
            } else if let Some(group) = &step.group {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask:dev>=1.0.0` (v1.2.3)
                    format!(
                        "`{}{}` ({})",
                        format!("{}:{}", step.name, group).cyan(),
                        range.cyan(),
                        format!("v{version}").cyan(),
                    )
                } else {
                    // Ex) `flask:dev>=1.0.0`
                    format!(
                        "`{}{}`",
                        format!("{}:{}", step.name, group).cyan(),
                        range.cyan(),
                    )
                }
            } else {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask>=1.0.0` (v1.2.3)
                    format!(
                        "`{}{}` ({})",
                        step.name.cyan(),
                        range.cyan(),
                        format!("v{version}").cyan(),
                    )
                } else {
                    // Ex) `flask>=1.0.0`
                    format!("`{}{}`", step.name.cyan(), range.cyan())
                }
            }
        } else {
            if let Some(extra) = &step.extra {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask[dotenv]` (v1.2.3)
                    format!(
                        "`{}` ({})",
                        format!("{}[{}]", step.name, extra).cyan(),
                        format!("v{version}").cyan(),
                    )
                } else {
                    // Ex) `flask[dotenv]`
                    format!("`{}`", format!("{}[{}]", step.name, extra).cyan())
                }
            } else if let Some(group) = &step.group {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask:dev` (v1.2.3)
                    format!(
                        "`{}` ({})",
                        format!("{}:{}", step.name, group).cyan(),
                        format!("v{version}").cyan(),
                    )
                } else {
                    // Ex) `flask:dev`
                    format!("`{}`", format!("{}:{}", step.name, group).cyan())
                }
            } else {
                if let Some(version) = step.version.as_ref() {
                    // Ex) `flask` (v1.2.3)
                    format!("`{}` ({})", step.name.cyan(), format!("v{version}").cyan())
                } else {
                    // Ex) `flask`
                    format!("`{}`", step.name.cyan())
                }
            }
        }
    }

    let mut message = if let Some(version) = version {
        format!(
            "`{}` ({}) was included because",
            name.cyan(),
            format!("v{version}").cyan()
        )
    } else {
        format!("`{}` was included because", name.cyan())
    };
    let mut range: Option<Ranges<Version>> = None;
    for (i, step) in chain.iter().enumerate() {
        if i > 0 {
            message = format!("{message} {} which depends on", format_step(step, range));
        } else {
            message = format!("{message} {} depends on", format_step(step, range));
        }
        range = Some(strip_local_version_sentinels(&step.range));
    }
    if let Some(range) = range.filter(|range| *range != Ranges::empty() && *range != Ranges::full())
    {
        message = format!("{message} `{}{}`", name.cyan(), range.cyan());
    } else {
        message = format!("{message} `{}`", name.cyan());
    }
    message
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use assert_fs::prelude::*;
    use insta::{assert_debug_snapshot, assert_snapshot};

    use uv_errors::{ErrorOptions, Hints, write_error_chain_with_options};
    use uv_fs::Simplified;
    use uv_settings::FilesystemOptions;
    use uv_workspace::pyproject::{PyProjectToml, PyprojectTomlError, SourceError};

    use super::{diagnostic_for_error, hints_for_error};

    #[test]
    fn settings_parse_error_retains_source() -> anyhow::Result<()> {
        let file = assert_fs::NamedTempFile::new("uv.toml")?;
        file.write_str(indoc::indoc! {r#"
            index-url = "https://user:first-secret@example.com/simple"
            preview-features = 123
            publish-url = "https://user:second-secret@example.com/legacy/"
        "#})?;
        let error = FilesystemOptions::from_file(file.path())
            .expect_err("invalid preview setting in test input");
        assert!(
            error
                .source()
                .expect("settings retain the original TOML cause")
                .is::<Box<toml::de::Error>>()
        );

        file.write_str("preview-features = []\n")?;
        let mut output = String::new();
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_diagnostic(diagnostic_for_error)
                .with_stream(&mut output),
        )?;
        let output = anstream::adapter::strip_str(&output);
        let display_path = regex::escape(&file.path().user_display().to_string());
        let filters = [(display_path.as_str(), "[CONFIG]")];
        insta::with_settings!({ filters => filters }, {
            assert_snapshot!(output, @"
            error: Failed to parse: `[CONFIG]`
              cause: invalid type: integer `123`, expected a boolean or a list of preview feature names
               --> [CONFIG]:2:20
                |
              2 | preview-features = 123
                |                    ^^^
            ");
        });
        Ok(())
    }

    #[test]
    fn pyproject_diagnostics_retain_parser_sources() {
        let error = uv_pypi_types::PyProjectToml::from_toml("123 - 456", "pyproject.toml")
            .expect_err("invalid TOML in test input");
        assert!(
            error
                .source()
                .expect("metadata retains the original syntax error")
                .is::<toml_edit::TomlError>()
        );
        assert!(diagnostic_for_error(&error).is_some());
        assert!(diagnostic_for_error(&Box::new(error)).is_some());

        let error =
            uv_pypi_types::PyProjectToml::from_toml("[project]\nname = 42\n", "pyproject.toml")
                .expect_err("invalid project name in test input");
        assert!(error.source().is_none());
        assert!(diagnostic_for_error(&error).is_some());

        let error = PyProjectToml::from_string("[project]\n".to_owned(), "pyproject.toml")
            .expect_err("missing project name in test input");
        assert!(error.source().is_none());
        assert!(diagnostic_for_error(&error).is_some());
        assert!(diagnostic_for_error(&Box::new(error)).is_some());
    }

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
}
