use std::str::FromStr;
use std::sync::LazyLock;

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use thiserror::Error;
use version_ranges::Ranges;

use uv_distribution_types::{DerivationChain, DerivationStep, Name};
use uv_errors::{Hint, Hints, write_error_chain};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::SentinelRange;

use crate::commands::pip;
use crate::commands::pip::install::ExternallyManagedError;
use crate::commands::pip::operations::ExtrasWithoutSourceError;
use crate::commands::project::ProjectError;
use crate::commands::project::add::AddDependencyError;
use crate::commands::project::remove::DependencyNotFoundError;
use crate::commands::project::run::RecursionLimitError;
use crate::commands::project::version::MissingProjectVersionError;
use crate::commands::tool::common::NoExecutablesError;
use crate::commands::tool::run::{ToolRunScriptError, ToolRunUsageError};

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

/// A requirements failure with command-specific resolution context.
#[derive(Debug, Error)]
#[error("Failed to resolve {context} requirement")]
struct RequirementsError {
    context: &'static str,
    #[source]
    cause: uv_requirements::Error,
}

/// Render an error using the standard error chain and its applicable hints.
pub(crate) fn render_error(err: &anyhow::Error) {
    let hints = hints_for_error(err);
    write_error_chain(err.as_ref(), hints).expect("writing to stderr should not fail");
}

/// Add requirement-resolution context to a user-facing failure.
pub(crate) fn requirements_error(
    context: &'static str,
    cause: uv_requirements::Error,
) -> anyhow::Error {
    anyhow::Error::new(RequirementsError { context, cause })
}

/// Walk an error chain and collect hint strings from all known error types.
///
/// This is the central "hint for error" function. It walks the full error chain
/// (via `anyhow::Error::chain`) and tries to downcast each error to known types
/// that implement [`Hint`]. All hint rendering logic should be consolidated here.
fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    let mut command_hints = Hints::none();
    for cause in err.chain() {
        collect_operation_hints(cause, &mut hints);
        collect_hint::<uv_client::Error>(cause, &mut hints);
        collect_hint::<AddDependencyError>(cause, &mut command_hints);
        collect_hint::<ToolRunUsageError>(cause, &mut command_hints);
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
        collect_hint::<crate::commands::build_frontend::Error>(cause, &mut hints);
        collect_hint::<uv_build_backend::Error>(cause, &mut hints);
        collect_hint::<uv_build_frontend::Error>(cause, &mut hints);
        collect_hint::<uv_types::AnyErrorBuild>(cause, &mut hints);
        collect_hint::<uv_python::Error>(cause, &mut hints);
        collect_hint::<uv_installer::IncompatibleWheelError>(cause, &mut hints);
        collect_hint::<uv_distribution::Error>(cause, &mut hints);
        collect_hint::<uv_python::BrokenLink>(cause, &mut hints);
        collect_hint::<uv_resolver::PylockTomlError>(cause, &mut hints);
        collect_hint::<uv_python::InterpreterError>(cause, &mut hints);
        collect_hint::<uv_workspace::pyproject::SourceError>(cause, &mut hints);
        collect_hint::<uv_distribution::LoweringError>(cause, &mut hints);
        collect_hint::<uv_virtualenv::Error>(cause, &mut hints);
        #[cfg(not(feature = "self-update"))]
        collect_hint::<crate::ExternallyInstalledError>(cause, &mut hints);
    }
    hints.extend(command_hints);
    hints
}

/// If `cause` can be downcast to `T`, collect its hints.
fn collect_hint<T: Hint + std::error::Error + 'static>(
    cause: &(dyn std::error::Error + 'static),
    hints: &mut Hints<'static>,
) {
    if let Some(inner) = cause.downcast_ref::<T>() {
        hints.extend(inner.hints());
    }
}

/// Collect hints that depend on operation metadata without replacing the underlying error type.
fn collect_operation_hints(cause: &(dyn std::error::Error + 'static), hints: &mut Hints<'static>) {
    if let Some(err) = cause.downcast_ref::<pip::operations::Error>() {
        match err {
            pip::operations::Error::Resolve(
                error @ uv_resolver::ResolveError::NoSolution { .. },
            ) => {
                hints.extend(error.hints());
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Client(error)) => {
                hints.extend(error.hints());
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dist(
                _,
                dist,
                chain,
                error,
            )) => {
                hints.extend(dist_hints(
                    dist.name(),
                    dist.version(),
                    chain,
                    error.hints(),
                ));
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dependencies(
                error,
                name,
                version,
                chain,
            )) => {
                hints.extend(dist_hints(name, Some(version), chain, error.hints()));
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Dist(_, dist, error)) => {
                hints.extend(dist_hints(
                    dist.name(),
                    dist.version(),
                    &DerivationChain::default(),
                    error.hints(),
                ));
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Dist(
                _,
                dist,
                chain,
                error,
            )) => {
                hints.extend(dist_hints(
                    dist.name(),
                    dist.version(),
                    chain,
                    error.hints(),
                ));
            }
            _ => {}
        }
    }

    if let Some(err) = cause.downcast_ref::<uv_resolver::ResolveError>() {
        match err {
            uv_resolver::ResolveError::Dist(_, dist, chain, error) => {
                hints.extend(dist_hints(
                    dist.name(),
                    dist.version(),
                    chain,
                    error.hints(),
                ));
            }
            uv_resolver::ResolveError::Dependencies(error, name, version, chain) => {
                hints.extend(dist_hints(name, Some(version), chain, error.hints()));
            }
            _ => {}
        }
    }

    if let Some(uv_requirements::Error::Dist(_, dist, error)) =
        cause.downcast_ref::<uv_requirements::Error>()
    {
        hints.extend(dist_hints(
            dist.name(),
            dist.version(),
            &DerivationChain::default(),
            error.hints(),
        ));
    }

    if let Some(uv_installer::PrepareError::Dist(_, dist, chain, error)) =
        cause.downcast_ref::<uv_installer::PrepareError>()
    {
        hints.extend(dist_hints(
            dist.name(),
            dist.version(),
            chain,
            error.hints(),
        ));
    }
}

/// Format package context that should follow a distribution error as hints.
fn dist_hints(
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
        range = Some(SentinelRange::from(&step.range).strip());
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
    use std::borrow::Cow;

    use uv_workspace::pyproject::{PyprojectTomlError, SourceError};

    use super::hints_for_error;

    #[test]
    fn collects_source_hints_through_pyproject_errors() {
        let err = anyhow::Error::new(PyprojectTomlError::Source(SourceError::OverlappingMarkers(
            "sys_platform == 'win32'".to_string(),
            "python_version == '3.12'".to_string(),
            "python_version != '3.12'".to_string(),
        )));

        let hints = hints_for_error(&err)
            .into_iter()
            .map(Cow::into_owned)
            .collect::<Vec<_>>();

        assert_eq!(
            hints,
            vec!["replace `python_version == '3.12'` with `python_version != '3.12'`".to_string()]
        );
    }
}
