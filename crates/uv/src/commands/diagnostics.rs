use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use thiserror::Error;
use version_ranges::Ranges;

use uv_distribution_types::{
    DerivationChain, DerivationStep, Dist, DistErrorKind, Name, RequestedDist,
};
use uv_errors::{ErrorOptions, Hint, Hints, write_error_chain_with_options};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::SentinelRange;

use crate::commands::pip;
use crate::commands::pip::install::ExternallyManagedError;
use crate::commands::pip::operations::ExtrasWithoutSourceError;
use crate::commands::project::ProjectError;
use crate::commands::project::remove::DependencyNotFoundError;
use crate::commands::project::run::RecursionLimitError;
use crate::commands::project::version::MissingProjectVersionError;
use crate::commands::tool::common::NoExecutablesError;
use crate::commands::tool::run::ToolRunScriptError;

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

/// A rich reporter for operational diagnostics, i.e., errors that occur during resolution and
/// installation.
#[derive(Debug, Default)]
pub(crate) struct OperationDiagnostic {
    /// A caller-provided hint to render after the error output.
    hint: Option<String>,
    /// Whether system certificates are being used.
    pub(crate) system_certs: bool,
    /// The context to display to the user upon resolution failure.
    pub(crate) context: Option<&'static str>,
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
        hint: Option<String>,
    },
    #[error("{kind} `{dist}`")]
    RequestedDist {
        kind: DistErrorKind,
        dist: Box<RequestedDist>,
        chain: DerivationChain,
        #[source]
        cause: Arc<uv_distribution::Error>,
        hint: Option<String>,
    },
    #[error("Failed to resolve dependencies for `{name}` (v{version})")]
    Dependencies {
        name: PackageName,
        version: Version,
        chain: DerivationChain,
        #[source]
        cause: Box<uv_resolver::ResolveError>,
        hint: Option<String>,
    },
    #[error("{header}")]
    NoSolution {
        header: uv_resolver::NoSolutionHeader,
        #[source]
        cause: Box<uv_resolver::NoSolutionError>,
        hint: Option<String>,
    },
    #[error("Failed to resolve {context} requirement")]
    Requirements {
        context: &'static str,
        #[source]
        cause: uv_requirements::Error,
        hint: Option<String>,
    },
}

impl Hint for OperationError {
    fn hints(&self) -> Hints<'_> {
        let (mut hints, extra_hint) = match self {
            Self::Dist {
                dist,
                chain,
                cause,
                hint,
                ..
            } => (
                dist_hints(dist.name(), dist.version(), chain, cause.hints()),
                hint,
            ),
            Self::RequestedDist {
                dist,
                chain,
                cause,
                hint,
                ..
            } => (
                dist_hints(dist.name(), dist.version(), chain, cause.hints()),
                hint,
            ),
            Self::Dependencies {
                name,
                version,
                chain,
                cause,
                hint,
            } => (dist_hints(name, Some(version), chain, cause.hints()), hint),
            Self::NoSolution { cause, hint, .. } => (cause.hints().into_owned(), hint),
            Self::Requirements { hint, .. } => (Hints::none(), hint),
        };
        if let Some(extra_hint) = extra_hint {
            hints.push(extra_hint.clone());
        }
        hints
    }
}

#[derive(Debug)]
struct SystemCertsError {
    cause: uv_client::Error,
    hint: Option<String>,
}

impl std::fmt::Display for SystemCertsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl std::error::Error for SystemCertsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.source()
    }
}

impl Hint for SystemCertsError {
    fn hints(&self) -> Hints<'_> {
        let mut hints = Hints::from(format!(
            "Consider enabling use of system TLS certificates with the `{}` command-line flag",
            "--system-certs".green()
        ));
        if let Some(hint) = &self.hint {
            hints.push(hint.clone());
        }
        hints
    }
}

fn render_handled_error(err: &anyhow::Error) {
    let hints = hints_for_error(err);
    write_error_chain_with_options(err.as_ref(), ErrorOptions::default().with_hints(hints))
        .expect("writing to stderr should not fail");
}

impl OperationDiagnostic {
    /// Create an [`OperationDiagnostic`] with the given system certificates setting.
    #[must_use]
    pub(crate) fn with_system_certs(system_certs: bool) -> Self {
        Self {
            system_certs,
            ..Default::default()
        }
    }

    /// Set the hint to display to the user upon resolution failure.
    #[must_use]
    pub(crate) fn with_hint(self, hint: String) -> Self {
        Self {
            hint: Some(hint),
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

    /// Attempt to report an error with rich diagnostic context.
    ///
    /// Returns `Some` if the error was not handled.
    pub(crate) fn report(self, err: pip::operations::Error) -> Option<pip::operations::Error> {
        let Self {
            hint,
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
                render_handled_error(&anyhow::Error::new(OperationError::NoSolution {
                    header,
                    cause: err,
                    hint,
                }));
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                render_handled_error(&anyhow::Error::new(OperationError::RequestedDist {
                    kind,
                    dist,
                    chain,
                    cause: err,
                    hint,
                }));
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dependencies(
                error,
                name,
                version,
                chain,
            )) => {
                render_handled_error(&anyhow::Error::new(OperationError::Dependencies {
                    name,
                    version,
                    chain,
                    cause: error,
                    hint,
                }));
                None
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Dist(kind, dist, err)) => {
                render_handled_error(&anyhow::Error::new(OperationError::Dist {
                    kind,
                    dist,
                    chain: DerivationChain::default(),
                    cause: Arc::new(*err),
                    hint,
                }));
                None
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                render_handled_error(&anyhow::Error::new(OperationError::Dist {
                    kind,
                    dist,
                    chain,
                    cause: Arc::new(*err),
                    hint,
                }));
                None
            }
            pip::operations::Error::Requirements(err) => {
                if let Some(context) = context {
                    render_handled_error(&anyhow::Error::new(OperationError::Requirements {
                        context,
                        cause: err,
                        hint,
                    }));
                    None
                } else {
                    Some(pip::operations::Error::Requirements(err))
                }
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Client(err))
                if !system_certs && err.is_ssl() =>
            {
                render_handled_error(&anyhow::Error::new(SystemCertsError { cause: err, hint }));
                None
            }
            err @ pip::operations::Error::OutdatedEnvironment(..) => {
                anstream::eprintln!("{}", err);
                if let Some(hint) = hint {
                    anstream::eprint!("{}", Hints::from(hint));
                }
                None
            }
            err => Some(err),
        }
    }
}

/// Walk an error chain and collect hint strings from all known error types.
///
/// This is the central "hint for error" function. It walks the full error chain
/// (via `anyhow::Error::chain`) and tries to downcast each error to known types
/// that implement [`Hint`]. All hint rendering logic should be consolidated here.
pub(crate) fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    for cause in err.chain() {
        collect_hint::<OperationError>(cause, &mut hints);
        collect_hint::<SystemCertsError>(cause, &mut hints);
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
