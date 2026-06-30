use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use version_ranges::Ranges;

use uv_distribution_types::{
    DerivationChain, DerivationStep, Dist, DistErrorKind, Name, RequestedDist,
};
use uv_errors::{Hint, Hints};
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
    /// Caller-provided hints to render after the error output.
    hints: Vec<String>,
    /// Whether system certificates are being used.
    system_certs: bool,
    /// The context to display to the user upon resolution failure.
    context: Option<&'static str>,
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

    /// Add a hint to display to the user upon resolution failure.
    #[must_use]
    pub(crate) fn with_hint(mut self, hint: String) -> Self {
        self.hints.push(hint);
        self
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
        let result = match err {
            pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err)) => {
                no_solution(&err, self.context);
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                requested_dist_error(kind, dist, &chain, err);
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dependencies(
                error,
                name,
                version,
                chain,
            )) => {
                dependencies_error(error, &name, &version, &chain);
                None
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Dist(kind, dist, err)) => {
                dist_error(kind, dist, &DerivationChain::default(), Arc::new(*err));
                None
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                dist_error(kind, dist, &chain, Arc::new(*err));
                None
            }
            pip::operations::Error::Requirements(err) => {
                if let Some(context) = self.context {
                    let err = miette::Report::msg(format!("{err}"))
                        .context(format!("Failed to resolve {context} requirement"));
                    anstream::eprint!("{err:?}");
                    None
                } else {
                    Some(pip::operations::Error::Requirements(err))
                }
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Client(err))
                if !self.system_certs && err.is_ssl() =>
            {
                system_certs_hint(err);
                None
            }
            err @ pip::operations::Error::OutdatedEnvironment(..) => {
                anstream::eprintln!("{}", err);
                None
            }
            err => Some(err),
        };

        // Render the caller-provided hints after the error output.
        if result.is_none() {
            let hints: Hints<'_> = self.hints.into_iter().collect();
            anstream::eprint!("{hints}");
        }

        result
    }
}

/// Render a distribution failure (read, download or build) with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
fn dist_error(
    kind: DistErrorKind,
    dist: Box<Dist>,
    chain: &DerivationChain,
    cause: Arc<uv_distribution::Error>,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{kind} `{dist}`")]
    #[diagnostic()]
    struct Diagnostic {
        kind: DistErrorKind,
        dist: Box<Dist>,
        #[source]
        cause: Arc<uv_distribution::Error>,
    }

    let hints = dist_hints(dist.name(), dist.version(), chain, cause.hints());
    let report = miette::Report::new(Diagnostic { kind, dist, cause });
    anstream::eprint!("{report:?}");
    anstream::eprint!("{hints}");
}

/// Render a requested distribution failure (read, download or build) with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
fn requested_dist_error(
    kind: DistErrorKind,
    dist: Box<RequestedDist>,
    chain: &DerivationChain,
    cause: Arc<uv_distribution::Error>,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{kind} `{dist}`")]
    #[diagnostic()]
    struct Diagnostic {
        kind: DistErrorKind,
        dist: Box<RequestedDist>,
        #[source]
        cause: Arc<uv_distribution::Error>,
    }

    let hints = dist_hints(dist.name(), dist.version(), chain, cause.hints());
    let report = miette::Report::new(Diagnostic { kind, dist, cause });
    anstream::eprint!("{report:?}");
    anstream::eprint!("{hints}");
}

/// Render an error in fetching a package's dependencies.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
fn dependencies_error(
    error: Box<uv_resolver::ResolveError>,
    name: &PackageName,
    version: &Version,
    chain: &DerivationChain,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to resolve dependencies for `{}` ({})", name.cyan(), format!("v{version}").cyan())]
    #[diagnostic()]
    struct Diagnostic {
        name: PackageName,
        version: Version,
        #[source]
        cause: Box<uv_resolver::ResolveError>,
    }

    let hints = dist_hints(name, Some(version), chain, error.hints());
    let report = miette::Report::new(Diagnostic {
        name: name.clone(),
        version: version.clone(),
        cause: error,
    });
    anstream::eprint!("{report:?}");
    anstream::eprint!("{hints}");
}

/// Render a [`uv_resolver::NoSolutionError`].
fn no_solution(err: &uv_resolver::NoSolutionError, context: Option<&'static str>) {
    let header = if let Some(context) = context {
        err.header().with_context(context)
    } else {
        err.header()
    };
    let report = miette::Report::msg(err.report().to_string()).context(header);
    anstream::eprint!("{report:?}");
    let hints = err.hints();
    anstream::eprint!("{hints}");
}

/// Render a TLS error with a hint to enable native TLS.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
fn system_certs_hint(err: uv_client::Error) {
    #[derive(Debug, miette::Diagnostic)]
    #[diagnostic()]
    struct Error {
        /// The underlying error.
        err: uv_client::Error,

        /// The help message to display.
        #[help]
        help: String,
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.err)
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.err.source()
        }
    }

    let report = miette::Report::new(Error {
        err,
        help: format!(
            "Consider enabling use of system TLS certificates with the `{}` command-line flag",
            "--system-certs".green()
        ),
    });
    anstream::eprint!("{report:?}");
}

/// Walk an error chain and collect hint strings from all known error types.
///
/// This is the central "hint for error" function. It walks the full error chain
/// (via `anyhow::Error::chain`) and tries to downcast each error to known types
/// that implement [`Hint`]. All hint rendering logic should be consolidated here.
pub(crate) fn hints_for_error(err: &anyhow::Error) -> Hints<'static> {
    let mut hints = Hints::none();
    for cause in err.chain() {
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
        collect_hint::<uv_build_backend::Error>(cause, &mut hints);
        collect_hint::<uv_build_frontend::Error>(cause, &mut hints);
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
