use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use version_ranges::Ranges;

use uv_distribution_types::{
    DerivationChain, DerivationStep, Dist, DistErrorKind, Name, RequestedDist,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::SentinelRange;

use crate::commands::ExitStatus;
use crate::commands::pip;

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
#[derive(Debug)]
pub(crate) struct OperationDiagnostic {
    /// The underlying operation error.
    pub(crate) err: pip::operations::Error,
    /// The hint to display to the user upon resolution failure.
    pub(crate) hint: Option<String>,
    /// The context to display to the user upon resolution failure.
    pub(crate) context: Option<&'static str>,
}

impl std::fmt::Display for OperationDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.err.fmt(f)
    }
}

impl std::error::Error for OperationDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.err.source()
    }
}

impl From<pip::operations::Error> for OperationDiagnostic {
    fn from(err: pip::operations::Error) -> Self {
        Self {
            err,
            hint: None,
            context: None,
        }
    }
}

impl OperationDiagnostic {
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

    /// Report the diagnostic and convert to an exit status.
    pub(crate) fn report(self, native_tls: bool) -> anyhow::Result<ExitStatus> {
        let err = self.err;
        let hint = self.hint;
        let context = self.context;
        match err {
            pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err)) => {
                if let Some(context) = context {
                    no_solution_context(&err, context);
                } else if let Some(hint) = hint {
                    no_solution_hint(err, hint);
                } else {
                    no_solution(&err);
                }
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                requested_dist_error(kind, dist, &chain, err, hint);
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Dependencies(
                error,
                name,
                version,
                chain,
            )) => {
                dependencies_error(error, &name, &version, &chain, hint.clone());
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Dist(kind, dist, err)) => {
                dist_error(kind, dist, &DerivationChain::default(), Arc::new(err), hint);
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Dist(
                kind,
                dist,
                chain,
                err,
            )) => {
                dist_error(kind, dist, &chain, Arc::new(err), hint);
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::Requirements(err) => {
                if let Some(context) = context {
                    let err = miette::Report::msg(format!("{err}"))
                        .context(format!("Failed to resolve {context} requirement"));
                    anstream::eprint!("{err:?}");
                    Ok(ExitStatus::Failure)
                } else {
                    Err(pip::operations::Error::Requirements(err).into())
                }
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Client(err))
                if !native_tls && err.is_ssl() =>
            {
                native_tls_hint(err);
                Ok(ExitStatus::Failure)
            }
            pip::operations::Error::OutdatedEnvironment => {
                anstream::eprintln!("{}", err);
                Ok(ExitStatus::Failure)
            }
            err => Err(err.into()),
        }
    }
}

/// Render a distribution failure (read, download or build) with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
pub(crate) fn dist_error(
    kind: DistErrorKind,
    dist: Box<Dist>,
    chain: &DerivationChain,
    cause: Arc<uv_distribution::Error>,
    help: Option<String>,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{kind} `{dist}`")]
    #[diagnostic()]
    struct Diagnostic {
        kind: DistErrorKind,
        dist: Box<Dist>,
        #[source]
        cause: Arc<uv_distribution::Error>,
        #[help]
        help: Option<String>,
    }

    let help = help.or_else(|| {
        SUGGESTIONS
            .get(dist.name())
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    dist.name().cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(dist.name(), dist.version(), chain))
                }
            })
    });
    let report = miette::Report::new(Diagnostic {
        kind,
        dist,
        cause,
        help,
    });
    anstream::eprint!("{report:?}");
}

/// Render a requested distribution failure (read, download or build) with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
pub(crate) fn requested_dist_error(
    kind: DistErrorKind,
    dist: Box<RequestedDist>,
    chain: &DerivationChain,
    cause: Arc<uv_distribution::Error>,
    help: Option<String>,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{kind} `{dist}`")]
    #[diagnostic()]
    struct Diagnostic {
        kind: DistErrorKind,
        dist: Box<RequestedDist>,
        #[source]
        cause: Arc<uv_distribution::Error>,
        #[help]
        help: Option<String>,
    }

    let help = help.or_else(|| {
        SUGGESTIONS
            .get(dist.name())
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    dist.name().cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(dist.name(), dist.version(), chain))
                }
            })
    });
    let report = miette::Report::new(Diagnostic {
        kind,
        dist,
        cause,
        help,
    });
    anstream::eprint!("{report:?}");
}

/// Render an error in fetching a package's dependencies.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
pub(crate) fn dependencies_error(
    error: Box<uv_resolver::ResolveError>,
    name: &PackageName,
    version: &Version,
    chain: &DerivationChain,
    help: Option<String>,
) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to resolve dependencies for `{}` ({})", name.cyan(), format!("v{version}").cyan())]
    #[diagnostic()]
    struct Diagnostic {
        name: PackageName,
        version: Version,
        #[source]
        cause: Box<uv_resolver::ResolveError>,
        #[help]
        help: Option<String>,
    }

    let help = help.or_else(|| {
        SUGGESTIONS
            .get(name)
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    name.cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(name, Some(version), chain))
                }
            })
    });
    let report = miette::Report::new(Diagnostic {
        name: name.clone(),
        version: version.clone(),
        cause: error,
        help,
    });
    anstream::eprint!("{report:?}");
}

/// Render a [`uv_resolver::NoSolutionError`].
pub(crate) fn no_solution(err: &uv_resolver::NoSolutionError) {
    let report = miette::Report::msg(format!("{err}")).context(err.header());
    anstream::eprint!("{report:?}");
}

/// Render a [`uv_resolver::NoSolutionError`] with dedicated context.
pub(crate) fn no_solution_context(err: &uv_resolver::NoSolutionError, context: &'static str) {
    let report = miette::Report::msg(format!("{err}")).context(err.header().with_context(context));
    anstream::eprint!("{report:?}");
}

/// Render a [`uv_resolver::NoSolutionError`] with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
pub(crate) fn no_solution_hint(err: Box<uv_resolver::NoSolutionError>, help: String) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{header}")]
    #[diagnostic()]
    struct Error {
        /// The header to render in the error message.
        header: uv_resolver::NoSolutionHeader,

        /// The underlying error.
        #[source]
        err: Box<uv_resolver::NoSolutionError>,

        /// The help message to display.
        #[help]
        help: String,
    }

    let header = err.header();
    let report = miette::Report::new(Error { header, err, help });
    anstream::eprint!("{report:?}");
}

/// Render a [`uv_resolver::NoSolutionError`] with a help message.
// https://github.com/rust-lang/rust/issues/147648
#[allow(unused_assignments)]
pub(crate) fn native_tls_hint(err: uv_client::Error) {
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
            "--native-tls".green()
        ),
    });
    anstream::eprint!("{report:?}");
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
