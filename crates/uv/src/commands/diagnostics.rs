use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use std::str::FromStr;
use std::sync::LazyLock;
use uv_distribution_types::{Name, SourceDist};
use uv_normalize::PackageName;

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

/// Render a [`uv_resolver::ResolveError::FetchAndBuild`] with a help message.
pub(crate) fn fetch_and_build(sdist: Box<SourceDist>, cause: uv_distribution::Error) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to download and build `{sdist}`")]
    #[diagnostic()]
    struct Error {
        sdist: Box<SourceDist>,
        #[source]
        cause: uv_distribution::Error,
        #[help]
        help: Option<String>,
    }

    let report = miette::Report::new(Error {
        help: SUGGESTIONS.get(sdist.name()).map(|suggestion| {
            format!(
                "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                sdist.name().cyan(),
                suggestion.cyan(),
                suggestion.cyan(),
            )
        }),
        sdist,
        cause,
    });
    anstream::eprint!("{report:?}");
}

/// Render a [`uv_resolver::ResolveError::Build`] with a help message.
pub(crate) fn build(sdist: Box<SourceDist>, cause: uv_distribution::Error) {
    #[derive(Debug, miette::Diagnostic, thiserror::Error)]
    #[error("Failed to build `{sdist}`")]
    #[diagnostic()]
    struct Error {
        sdist: Box<SourceDist>,
        #[source]
        cause: uv_distribution::Error,
        #[help]
        help: Option<String>,
    }

    let report = miette::Report::new(Error {
        help: SUGGESTIONS.get(sdist.name()).map(|suggestion| {
            format!(
                "`{}` is often confused for `{}` Did you mean to install `{}` instead?",
                sdist.name().cyan(),
                suggestion.cyan(),
                suggestion.cyan(),
            )
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
