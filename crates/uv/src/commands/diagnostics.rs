use std::str::FromStr;
use std::sync::LazyLock;

use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use version_ranges::Ranges;

use uv_distribution_types::{BuiltDist, DerivationChain, DerivationStep, Name, SourceDist};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::SentinelRange;

use crate::commands::pip;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// Static map of common package name typos or misconfigurations to their correct package names.
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
    /// The hint to display to the user upon resolution failure.
    pub(crate) hint: Option<String>,
    /// The context to display to the user upon resolution failure.
    pub(crate) context: Option<&'static str>,
}

impl OperationDiagnostic {
    /// Set the hint to display to the user upon resolution failure.
    #[must_use]
    pub(crate) fn with_hint(hint: String) -> Self {
        Self {
            hint: Some(hint),
            ..Default::default()
        }
    }

    /// Set the context to display to the user upon resolution failure.
    #[must_use]
    pub(crate) fn with_context(context: &'static str) -> Self {
        Self {
            context: Some(context),
            ..Default::default()
        }
    }

    /// Attempt to report an error with rich diagnostic context.
    ///
    /// Returns `Some` if the error was not handled.
    pub(crate) fn report(self, err: pip::operations::Error) -> Option<pip::operations::Error> {
        match err {
            pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err)) => {
                if let Some(context) = self.context {
                    no_solution_context(&err, context);
                } else if let Some(hint) = self.hint {
                    // TODO(charlie): The `hint` should be shown on all diagnostics, not just
                    // `NoSolutionError`.
                    no_solution_hint(err, hint);
                } else {
                    no_solution(&err);
                }
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::DownloadAndBuild(
                dist,
                chain,
                err,
            )) => {
                download_and_build(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Download(
                dist,
                chain,
                err,
            )) => {
                download(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Resolve(uv_resolver::ResolveError::Build(dist, chain, err)) => {
                build(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Requirements(uv_requirements::Error::DownloadAndBuild(
                dist,
                chain,
                err,
            )) => {
                download_and_build(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Download(
                dist,
                chain,
                err,
            )) => {
                download(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Requirements(uv_requirements::Error::Build(
                dist,
                chain,
                err,
            )) => {
                build(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::DownloadAndBuild(
                dist,
                chain,
                err,
            )) => {
                download_and_build(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Download(
                dist,
                chain,
                err,
            )) => {
                download(dist, &chain, Box::new(err));
                None
            }
            pip::operations::Error::Prepare(uv_installer::PrepareError::Build(
                dist,
                chain,
                err,
            )) => {
                build(dist, &chain, Box::new(err));
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
            err => Some(err),
        }
    }
}

/// Render a remote source distribution build failure with a help message.
pub(crate) fn download_and_build(sdist: Box<SourceDist>, chain: &DerivationChain, cause: Error) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to download and build `{sdist}`")]
    #[diagnostic()]
    struct Diagnostic {
        sdist: Box<SourceDist>,
        #[source]
        cause: Error,
        #[help]
        help: Option<String>,
    }

    let report = miette::Report::new(Diagnostic {
        help: SUGGESTIONS
            .get(sdist.name())
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    sdist.name().cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(sdist.name(), sdist.version(), chain))
                }
            }),
        sdist,
        cause,
    });
    anstream::eprint!("{report:?}");
}

/// Render a remote binary distribution download failure with a help message.
pub(crate) fn download(wheel: Box<BuiltDist>, chain: &DerivationChain, cause: Error) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to download `{wheel}`")]
    #[diagnostic()]
    struct Diagnostic {
        wheel: Box<BuiltDist>,
        #[source]
        cause: Error,
        #[help]
        help: Option<String>,
    }

    let report = miette::Report::new(Diagnostic {
        help: SUGGESTIONS
            .get(wheel.name())
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    wheel.name().cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(wheel.name(), Some(wheel.version()), chain))
                }
            }),
        wheel,
        cause,
    });
    anstream::eprint!("{report:?}");
}

/// Render a local source distribution build failure with a help message.
pub(crate) fn build(sdist: Box<SourceDist>, chain: &DerivationChain, cause: Error) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to build `{sdist}`")]
    #[diagnostic()]
    struct Diagnostic {
        sdist: Box<SourceDist>,
        #[source]
        cause: Error,
        #[help]
        help: Option<String>,
    }

    let report = miette::Report::new(Diagnostic {
        help: SUGGESTIONS
            .get(sdist.name())
            .map(|suggestion| {
                format!(
                    "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                    sdist.name().cyan(),
                    suggestion.cyan(),
                    suggestion.cyan(),
                )
            })
            .or_else(|| {
                if chain.is_empty() {
                    None
                } else {
                    Some(format_chain(sdist.name(), sdist.version(), chain))
                }
            }),
        sdist,
        cause,
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
pub(crate) fn no_solution_hint(err: uv_resolver::NoSolutionError, help: String) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("{header}")]
    #[diagnostic()]
    struct Error {
        /// The header to render in the error message.
        header: uv_resolver::NoSolutionHeader,

        /// The underlying error.
        #[source]
        err: uv_resolver::NoSolutionError,

        /// The help message to display.
        #[help]
        help: String,
    }

    let header = err.header();
    let report = miette::Report::new(Error { header, err, help });
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
                // Ex) `flask[dotenv]>=1.0.0` (v1.2.3)`
                format!(
                    "`{}{}` ({})",
                    format!("{}[{}]", step.name, extra).cyan(),
                    range.cyan(),
                    format!("v{}", step.version).cyan(),
                )
            } else if let Some(group) = &step.group {
                // Ex) `flask:dev>=1.0.0` (v1.2.3)`
                format!(
                    "`{}{}` ({})",
                    format!("{}:{}", step.name, group).cyan(),
                    range.cyan(),
                    format!("v{}", step.version).cyan(),
                )
            } else {
                // Ex) `flask>=1.0.0` (v1.2.3)`
                format!(
                    "`{}{}` ({})",
                    step.name.cyan(),
                    range.cyan(),
                    format!("v{}", step.version).cyan(),
                )
            }
        } else {
            if let Some(extra) = &step.extra {
                // Ex) `flask[dotenv]` (v1.2.3)`
                format!(
                    "`{}` ({})",
                    format!("{}[{}]", step.name, extra).cyan(),
                    format!("v{}", step.version).cyan(),
                )
            } else if let Some(group) = &step.group {
                // Ex) `flask:dev` (v1.2.3)`
                format!(
                    "`{}` ({})",
                    format!("{}:{}", step.name, group).cyan(),
                    format!("v{}", step.version).cyan(),
                )
            } else {
                // Ex) `flask` (v1.2.3)`
                format!(
                    "`{}` ({})",
                    step.name.cyan(),
                    format!("v{}", step.version).cyan()
                )
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
