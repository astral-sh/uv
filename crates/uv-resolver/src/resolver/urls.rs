use rustc_hash::FxHashMap;
use tracing::debug;

use distribution_types::{
    ParsedArchiveUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl, RequirementSource, Verbatim,
    VerbatimParsedUrl,
};
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use uv_distribution::is_same_reference;
use uv_git::GitUrl;
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest, ResolveError};

/// A map of package names to their associated, required URLs.
#[derive(Debug, Default)]
pub(crate) struct Urls(FxHashMap<PackageName, VerbatimParsedUrl>);

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
        dependencies: DependencyMode,
    ) -> Result<Self, ResolveError> {
        let mut urls: FxHashMap<PackageName, VerbatimParsedUrl> = FxHashMap::default();

        // Add the editables themselves to the list of required URLs.
        for editable in &manifest.editables {
            let editable_url = VerbatimParsedUrl {
                parsed_url: ParsedUrl::Path(ParsedPathUrl {
                    url: editable.built.url.to_url(),
                    path: editable.built.path.clone(),
                    editable: true,
                }),
                verbatim: editable.built.url.clone(),
            };
            if let Some(previous) =
                urls.insert(editable.metadata.name.clone(), editable_url.clone())
            {
                if !is_equal(&previous.verbatim, &editable_url.verbatim) {
                    if is_same_reference(&previous.verbatim, &editable_url.verbatim) {
                        debug!(
                            "Allowing {} as a variant of {}",
                            editable_url.verbatim, previous.verbatim
                        );
                    } else {
                        return Err(ResolveError::ConflictingUrlsDirect(
                            editable.metadata.name.clone(),
                            previous.verbatim.verbatim().to_string(),
                            editable_url.verbatim.verbatim().to_string(),
                        ));
                    }
                }
            }
        }

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
                            editable: (*editable).unwrap_or_default(),
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
                            if is_same_reference(&previous.verbatim, &url.verbatim) {
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
    pub(crate) fn is_allowed(expected: &VerbatimUrl, provided: &VerbatimUrl) -> bool {
        #[allow(clippy::if_same_then_else)]
        if is_equal(expected, provided) {
            // If the URLs are canonically equivalent, they're compatible.
            true
        } else if is_same_reference(expected, provided) {
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
    cache_key::CanonicalUrl::new(previous.raw()) == cache_key::CanonicalUrl::new(url.raw())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
