use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use thiserror::Error;
use version_ranges::Ranges;

use uv_distribution_types::{
    DerivationChain, DerivationStep, Dist, DistErrorKind, Name, RequestedDist,
};
use uv_errors::{Hint, Hints, write_error_chain};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::SentinelRange;

use crate::commands::ExitStatus;
use crate::commands::pip;
use crate::commands::pip::install::ExternallyManagedError;
use crate::commands::pip::operations::ExtrasWithoutSourceError;
use crate::commands::project::ProjectError;
use crate::commands::project::remove::DependencyNotFoundError;
use crate::commands::project::run::RecursionLimitError;
use crate::commands::project::version::MissingProjectVersionError;
use crate::commands::tool::common::NoExecutablesError;
use crate::commands::tool::run::{ToolRunCommand, ToolRunScriptError};

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

/// Context used to convert recognized operation failures into user-facing errors.
#[derive(Debug, Default)]
pub(crate) struct OperationErrorContext {
    /// Caller-provided context used to generate a hint after the error output.
    hint_context: Option<OperationHintContext>,
    /// Whether system certificates are being used.
    system_certs: bool,
    /// The context to display to the user upon resolution failure.
    context: Option<&'static str>,
}

#[derive(Debug)]
enum OperationHintContext {
    AddFrozen,
    UvxRun {
        arguments: String,
    },
    ToolVerbose {
        verbose_flag: String,
        target: String,
        invocation_source: ToolRunCommand,
    },
}

impl Hint for OperationHintContext {
    fn hints(&self) -> Hints<'_> {
        Hints::from(match self {
            Self::AddFrozen => format!(
                "If you want to add the package regardless of the failed resolution, provide the `{}` flag to skip locking and syncing",
                "--frozen".green()
            ),
            Self::UvxRun { arguments } => format!(
                "`{}` invokes the `{}` package. Did you mean `{}`?",
                format!("uvx run {arguments}").green(),
                "run".cyan(),
                format!("uvx {arguments}").green()
            ),
            Self::ToolVerbose {
                verbose_flag,
                target,
                invocation_source,
            } => format!(
                "You provided `{}` to `{}`. Did you mean to provide it to `{}`? e.g., `{}`",
                verbose_flag.cyan(),
                target.cyan(),
                invocation_source.to_string().cyan(),
                format!("{invocation_source} {verbose_flag} {target}").green()
            ),
        })
    }
}

#[derive(Debug, Error)]
enum OperationError {
    #[error("{kind} `{dist}`")]
    Dist {
        kind: DistErrorKind,
        dist: Box<Dist>,
        chain: DerivationChain,
        #[source]
        cause: Arc<uv_distribution::Error>,
        hint_context: Option<OperationHintContext>,
    },
    #[error("{kind} `{dist}`")]
    RequestedDist {
        kind: DistErrorKind,
        dist: Box<RequestedDist>,
        chain: DerivationChain,
        #[source]
        cause: Arc<uv_distribution::Error>,
        hint_context: Option<OperationHintContext>,
    },
    #[error("Failed to resolve dependencies for `{name}` (v{version})")]
    Dependencies {
        name: PackageName,
        version: Version,
        chain: DerivationChain,
        #[source]
        cause: Box<uv_resolver::ResolveError>,
        hint_context: Option<OperationHintContext>,
    },
    #[error("{header}")]
    NoSolution {
        header: uv_resolver::NoSolutionHeader,
        #[source]
        cause: Box<uv_resolver::NoSolutionError>,
        hint_context: Option<OperationHintContext>,
    },
    #[error("Failed to resolve {context} requirement")]
    Requirements {
        context: &'static str,
        #[source]
        cause: uv_requirements::Error,
        hint_context: Option<OperationHintContext>,
    },
    #[error("{cause}")]
    SystemCerts {
        #[source]
        cause: uv_client::Error,
        hint_context: Option<OperationHintContext>,
    },
    #[error("{cause}")]
    OutdatedEnvironment {
        cause: pip::operations::Error,
        hint_context: Option<OperationHintContext>,
    },
}

impl Hint for OperationError {
    fn hints(&self) -> Hints<'_> {
        let (mut hints, hint_context) = match self {
            Self::Dist {
                dist,
                chain,
                cause,
                hint_context,
                ..
            } => (
                dist_hints(dist.name(), dist.version(), chain, cause.hints()),
                hint_context,
            ),
            Self::RequestedDist {
                dist,
                chain,
                cause,
                hint_context,
                ..
            } => (
                dist_hints(dist.name(), dist.version(), chain, cause.hints()),
                hint_context,
            ),
            Self::Dependencies {
                name,
                version,
                chain,
                cause,
                hint_context,
            } => (
                dist_hints(name, Some(version), chain, cause.hints()),
                hint_context,
            ),
            Self::NoSolution {
                cause,
                hint_context,
                ..
            } => (cause.hints().into_owned(), hint_context),
            Self::Requirements { hint_context, .. } => (Hints::none(), hint_context),
            Self::SystemCerts { hint_context, .. } => (
                Hints::from(format!(
                    "Consider enabling use of system TLS certificates with the `{}` command-line flag",
                    "--system-certs".green()
                )),
                hint_context,
            ),
            Self::OutdatedEnvironment { hint_context, .. } => (Hints::none(), hint_context),
        };
        if let Some(hint_context) = hint_context {
            hints.extend(hint_context.hints());
        }
        hints
    }
}

trait ErrorExitStatus {
    fn exit_status(&self) -> ExitStatus;
}

impl ErrorExitStatus for OperationError {
    fn exit_status(&self) -> ExitStatus {
        ExitStatus::Failure
    }
}

/// Render an error using the standard error chain and its applicable hints.
pub(crate) fn render_error(err: &anyhow::Error) {
    let hints = hints_for_error(err);
    write_error_chain(err.as_ref(), hints).expect("writing to stderr should not fail");
}

/// Determine the process status for a propagated error, defaulting to unexpected failure.
pub(crate) fn exit_status_for_error(err: &anyhow::Error) -> ExitStatus {
    for cause in err.chain() {
        if let Some(exit_status) = get_exit_status::<OperationError>(cause) {
            return exit_status;
        }
    }
    ExitStatus::Error
}

/// Add requirement-resolution context to a user-facing failure.
pub(crate) fn requirements_error(
    context: &'static str,
    cause: uv_requirements::Error,
) -> anyhow::Error {
    anyhow::Error::new(OperationError::Requirements {
        context,
        cause,
        hint_context: None,
    })
}

fn get_exit_status<T: ErrorExitStatus + std::error::Error + 'static>(
    cause: &(dyn std::error::Error + 'static),
) -> Option<ExitStatus> {
    cause.downcast_ref::<T>().map(ErrorExitStatus::exit_status)
}

impl OperationErrorContext {
    /// Create an [`OperationErrorContext`] with the given system certificates setting.
    #[must_use]
    pub(crate) fn with_system_certs(system_certs: bool) -> Self {
        Self {
            system_certs,
            ..Default::default()
        }
    }

    /// Include a hint about skipping lock and sync operations after an `add` failure.
    #[must_use]
    pub(crate) fn with_add_frozen_hint(self) -> Self {
        Self {
            hint_context: Some(OperationHintContext::AddFrozen),
            ..self
        }
    }

    /// Include a hint for a likely mistaken `uvx run` invocation.
    #[must_use]
    pub(crate) fn with_uvx_run_hint(self, arguments: String) -> Self {
        Self {
            hint_context: Some(OperationHintContext::UvxRun { arguments }),
            ..self
        }
    }

    /// Include a hint for a verbose flag passed to a tool rather than to uv.
    #[must_use]
    pub(crate) fn with_tool_verbose_hint(
        self,
        verbose_flag: String,
        target: String,
        invocation_source: ToolRunCommand,
    ) -> Self {
        Self {
            hint_context: Some(OperationHintContext::ToolVerbose {
                verbose_flag,
                target,
                invocation_source,
            }),
            ..self
        }
    }

    /// Set the context to display to the user upon resolution failure.
    #[must_use]
    pub(crate) fn with_context(self, context: &'static str) -> Self {
        Self {
            context: Some(context),
            ..self
        }
    }

    /// Convert recognized operation failures into errors with user-facing context.
    pub(crate) fn into_error(self, err: pip::operations::Error) -> anyhow::Error {
        let Self {
            hint_context,
            system_certs,
            context,
        } = self;
        match err {
            pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err)) => {
                let header = if let Some(context) = context {
                    err.header().with_context(context)
                } else {
                    err.header()
                };
                anyhow::Error::new(OperationError::NoSolution {
                    header,
                    cause: err,
                    hint_context,
                })
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => anyhow::Error::new(OperationError::RequestedDist {
                kind,
                dist,
                chain,
                cause: err,
                hint_context,
            }),
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dependencies(
                error,
                name,
                version,
                chain,
            )) => anyhow::Error::new(OperationError::Dependencies {
                name,
                version,
                chain,
                cause: error,
                hint_context,
            }),
            pip::operations::Error::Requirements(uv_requirements::Error::Dist(kind, dist, err)) => {
                anyhow::Error::new(OperationError::Dist {
                    kind,
                    dist,
                    chain: DerivationChain::default(),
                    cause: Arc::new(*err),
                    hint_context,
                })
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => anyhow::Error::new(OperationError::Dist {
                kind,
                dist,
                chain,
                cause: Arc::new(*err),
                hint_context,
            }),
            pip::operations::Error::Requirements(err) => {
                if let Some(context) = context {
                    anyhow::Error::new(OperationError::Requirements {
                        context,
                        cause: err,
                        hint_context,
                    })
                } else {
                    anyhow::Error::new(pip::operations::Error::Requirements(err))
                }
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Client(err))
                if !system_certs && err.is_ssl() =>
            {
                anyhow::Error::new(OperationError::SystemCerts {
                    cause: err,
                    hint_context,
                })
            }
            err @ pip::operations::Error::OutdatedEnvironment(..) => {
                anyhow::Error::new(OperationError::OutdatedEnvironment {
                    cause: err,
                    hint_context,
                })
            }
            err => anyhow::Error::new(err),
        }
    }
}

/// Walk an error chain and collect hint strings from all known error types.
///
/// This is the central "hint for error" function. It walks the full error chain
/// (via `anyhow::Error::chain`) and tries to downcast each error to known types
/// that implement [`Hint`]. All hint rendering logic should be consolidated here.
fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    for cause in err.chain() {
        collect_hint::<OperationError>(cause, &mut hints);
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
