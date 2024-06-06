use rustc_hash::FxHashMap;
use same_file::is_same_file;
use tracing::debug;
use url::Url;

use cache_key::CanonicalUrl;
use distribution_types::Verbatim;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use pypi_types::{
    ParsedArchiveUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl, RequirementSource, VerbatimParsedUrl,
};
use uv_git::{GitResolver, GitUrl};
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest, ResolveError};

/// A map of package names to their associated, required URLs.
#[derive(Debug, Default)]
pub(crate) struct Urls(FxHashMap<PackageName, VerbatimParsedUrl>);

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
        git: &GitResolver,
        dependencies: DependencyMode,
    ) -> Result<Self, ResolveError> {
        let mut urls: FxHashMap<PackageName, VerbatimParsedUrl> = FxHashMap::default();

        // Add all direct requirements and constraints. If there are any conflicts, return an error.
        for requirement in manifest.requirements(markers, dependencies) {
            match &requirement.source {
                RequirementSource::Registry { .. } => {}
                RequirementSource::Url {
                    subdirectory,
                    location,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Archive(ParsedArchiveUrl {
                            url: location.clone(),
                            subdirectory: subdirectory.clone(),
                        }),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous.verbatim, &url.verbatim) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                    }
                }
                RequirementSource::Path {
                    path,
                    editable,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Path(ParsedPathUrl {
                            url: url.to_url(),
                            path: path.clone(),
                            editable: *editable,
                        }),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if let VerbatimParsedUrl {
                            parsed_url: ParsedUrl::Path(previous_path),
                            ..
                        } = &previous
                        {
                            // On Windows, we can have two versions of the same path, e.g.
                            // `C:\Users\KONSTA~1` and `C:\Users\Konstantin`.
                            if is_same_file(path, &previous_path.path).unwrap_or(false) {
                                continue;
                            }
                        }

                        if !is_equal(&previous.verbatim, &url.verbatim) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                    }
                }
                RequirementSource::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory,
                    url,
                } => {
                    let mut git_url = GitUrl::new(repository.clone(), reference.clone());
                    if let Some(precise) = precise {
                        git_url = git_url.with_precise(*precise);
                    }
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Git(ParsedGitUrl {
                            url: git_url,
                            subdirectory: subdirectory.clone(),
                        }),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous.verbatim, &url.verbatim) {
                            if is_same_reference(&previous.verbatim, &url.verbatim, git) {
                                debug!(
                                    "Allowing {} as a variant of {}",
                                    &url.verbatim, previous.verbatim
                                );
                            } else {
                                return Err(ResolveError::ConflictingUrlsDirect(
                                    requirement.name.clone(),
                                    previous.verbatim.verbatim().to_string(),
                                    url.verbatim.verbatim().to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(Self(urls))
    }

    /// Return the [`VerbatimUrl`] associated with the given package name, if any.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&VerbatimParsedUrl> {
        self.0.get(package)
    }

    /// Returns `true` if the provided URL is compatible with the given "allowed" URL.
    pub(crate) fn is_allowed(
        expected: &VerbatimUrl,
        provided: &VerbatimUrl,
        git: &GitResolver,
    ) -> bool {
        #[allow(clippy::if_same_then_else)]
        if is_equal(expected, provided) {
            // If the URLs are canonically equivalent, they're compatible.
            true
        } else if is_same_reference(expected, provided, git) {
            // If the URLs refer to the same commit, they're compatible.
            true
        } else {
            // Otherwise, they're incompatible.
            false
        }
    }
}

/// Returns `true` if the [`VerbatimUrl`] is compatible with the previous [`VerbatimUrl`].
///
/// Accepts URLs that map to the same [`CanonicalUrl`].
fn is_equal(previous: &VerbatimUrl, url: &VerbatimUrl) -> bool {
    CanonicalUrl::new(previous.raw()) == CanonicalUrl::new(url.raw())
}

/// Returns `true` if the URLs refer to the same Git commit.
///
/// For example, the previous URL could be a branch or tag, while the current URL would be a
/// precise commit hash.
fn is_same_reference<'a>(a: &'a Url, b: &'a Url, git: &'a GitResolver) -> bool {
    // Convert `a` to a Git URL, if possible.
    let Ok(a_git) = ParsedGitUrl::try_from(Url::from(CanonicalUrl::new(a))) else {
        return false;
    };

    // Convert `b` to a Git URL, if possible.
    let Ok(b_git) = ParsedGitUrl::try_from(Url::from(CanonicalUrl::new(b))) else {
        return false;
    };

    // The URLs must refer to the same subdirectory, if any.
    if a_git.subdirectory != b_git.subdirectory {
        return false;
    }

    // The Git URLs must refer to the same commit.
    git.same_ref(&a_git.url, &b_git.url)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use url::Url;

    use pep508_rs::VerbatimUrl;
    use uv_git::{GitResolver, GitSha, GitUrl, RepositoryReference};

    use crate::resolver::urls::{is_equal, is_same_reference};

    #[test]
    fn url_compatibility() -> Result<(), url::ParseError> {
        // Same repository, same tag.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(is_equal(&previous, &url));

        // Same repository, different tags.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.1")?;
        assert!(!is_equal(&previous, &url));

        // Same repository (with and without `.git`), same tag.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(is_equal(&previous, &url));

        // Same repository, no tag on the previous URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(!is_equal(&previous, &url));

        // Same repository, tag on the previous URL, no tag on the overriding URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        assert!(!is_equal(&previous, &url));

        Ok(())
    }

    #[test]
    fn same_reference() -> anyhow::Result<()> {
        let empty = GitResolver::default();

        // Same repository, same tag.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main")?;
        assert!(is_same_reference(&a, &b, &empty));

        // Same repository, same tag, same subdirectory.
        let a = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        assert!(is_same_reference(&a, &b, &empty));

        // Different repositories, same tag.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyOtherProject.git@main")?;
        assert!(!is_same_reference(&a, &b, &empty));

        // Same repository, different tags.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyProject.git@v1.0")?;
        assert!(!is_same_reference(&a, &b, &empty));

        // Same repository, same tag, different subdirectory.
        let a = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=other_dir")?;
        assert!(!is_same_reference(&a, &b, &empty));

        // Same repository, different tags, but same precise commit.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse(
            "git+https://example.com/MyProject.git@164a8735b081663fede48c5041667b194da15d25",
        )?;
        let resolved_refs = GitResolver::default();
        resolved_refs.insert(
            RepositoryReference::from(&GitUrl::try_from(Url::parse(
                "https://example.com/MyProject@main",
            )?)?),
            GitSha::from_str("164a8735b081663fede48c5041667b194da15d25")?,
        );
        assert!(is_same_reference(&a, &b, &resolved_refs));

        // Same repository, different tags, different precise commit.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse(
            "git+https://example.com/MyProject.git@164a8735b081663fede48c5041667b194da15d25",
        )?;
        let resolved_refs = GitResolver::default();
        resolved_refs.insert(
            RepositoryReference::from(&GitUrl::try_from(Url::parse(
                "https://example.com/MyProject@main",
            )?)?),
            GitSha::from_str("f2c9e88f3ec9526bbcec68d150b176d96a750aba")?,
        );
        assert!(!is_same_reference(&a, &b, &resolved_refs));

        Ok(())
    }
}
