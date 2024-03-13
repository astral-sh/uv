use rustc_hash::FxHashMap;
use tracing::debug;

use distribution_types::Verbatim;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use uv_normalize::PackageName;

use crate::{Manifest, ResolveError};

#[derive(Debug, Default)]
pub(crate) struct Urls {
    /// A map of package names to their associated, required URLs.
    required: FxHashMap<PackageName, VerbatimUrl>,
    /// A map from required URL to URL that is assumed to be a less precise variant.
    allowed: FxHashMap<VerbatimUrl, VerbatimUrl>,
}

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut required: FxHashMap<PackageName, VerbatimUrl> = FxHashMap::default();
        let mut allowed: FxHashMap<VerbatimUrl, VerbatimUrl> = FxHashMap::default();

        // Add all direct requirements and constraints. If there are any conflicts, return an error.
        for requirement in manifest
            .requirements
            .iter()
            .chain(manifest.constraints.iter())
        {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                if let Some(previous) = required.insert(requirement.name.clone(), url.clone()) {
                    if is_equal(&previous, url) {
                        continue;
                    }

                    if is_precise(&previous, url) {
                        debug!("Assuming {url} is a precise variant of {previous}");
                        allowed.insert(url.clone(), previous);
                        continue;
                    }

                    return Err(ResolveError::ConflictingUrlsDirect(
                        requirement.name.clone(),
                        previous.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }
            }
        }

        // Add any editable requirements. If there are any conflicts, return an error.
        for (editable, metadata) in &manifest.editables {
            if let Some(previous) = required.insert(metadata.name.clone(), editable.url.clone()) {
                if !is_equal(&previous, &editable.url) {
                    if is_precise(&previous, &editable.url) {
                        debug!(
                            "Assuming {} is a precise variant of {previous}",
                            editable.url
                        );
                        allowed.insert(editable.url.clone(), previous);
                    } else {
                        return Err(ResolveError::ConflictingUrlsDirect(
                            metadata.name.clone(),
                            previous.verbatim().to_string(),
                            editable.verbatim().to_string(),
                        ));
                    }
                }
            }

            for requirement in &metadata.requires_dist {
                if !requirement.evaluate_markers(markers, &editable.extras) {
                    continue;
                }

                if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                    if let Some(previous) = required.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous, url) {
                            if is_precise(&previous, url) {
                                debug!("Assuming {url} is a precise variant of {previous}");
                                allowed.insert(url.clone(), previous);
                            } else {
                                return Err(ResolveError::ConflictingUrlsDirect(
                                    requirement.name.clone(),
                                    previous.verbatim().to_string(),
                                    url.verbatim().to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Add any overrides. Conflicts here are fine, as the overrides are meant to be
        // authoritative.
        for requirement in &manifest.overrides {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                required.insert(requirement.name.clone(), url.clone());
            }
        }

        Ok(Self { required, allowed })
    }

    /// Return the [`VerbatimUrl`] associated with the given package name, if any.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&VerbatimUrl> {
        self.required.get(package)
    }

    /// Returns `true` if the provided URL is compatible with the given "allowed" URL.
    pub(crate) fn is_allowed(&self, expected: &VerbatimUrl, provided: &VerbatimUrl) -> bool {
        #[allow(clippy::if_same_then_else)]
        if is_equal(expected, provided) {
            // If the URLs are canonically equivalent, they're compatible.
            true
        } else if self
            .allowed
            .get(expected)
            .is_some_and(|allowed| is_equal(allowed, provided))
        {
            // If the URL is canonically equivalent to the imprecise variant of the URL, they're
            // compatible.
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

/// Returns `true` if the [`VerbatimUrl`] appears to be a more precise variant of the previous
/// [`VerbatimUrl`].
///
/// Primarily, this method intends to accept URLs that map to the same repository, but with a
/// precise Git commit hash overriding a looser tag or branch. For example, if the previous URL
/// is `git+https://github.com/pallets/werkzeug.git@main`, this method would accept
/// `git+https://github.com/pallets/werkzeug@32e69512134c2f8183c6438b2b2e13fd24e9d19f`, and
/// assume that the latter is a more precise variant of the former. This is particularly useful
/// for workflows in which the output of `uv pip compile` is used as an input constraint on a
/// subsequent resolution, since `uv` will pin the exact commit hash of the package.
fn is_precise(previous: &VerbatimUrl, url: &VerbatimUrl) -> bool {
    if cache_key::RepositoryUrl::new(previous.raw()) != cache_key::RepositoryUrl::new(url.raw()) {
        return false;
    }

    // If there's no tag in the overriding URL, consider it incompatible.
    let Some(url_tag) = url
        .raw()
        .path()
        .rsplit_once('@')
        .map(|(_prefix, suffix)| suffix)
    else {
        return false;
    };

    // Accept the overriding URL, as long as it's a full commit hash...
    let url_is_commit = url_tag.len() == 40 && url_tag.chars().all(|ch| ch.is_ascii_hexdigit());
    if !url_is_commit {
        return false;
    }

    // If there's no tag in the previous URL, consider it compatible.
    let Some(previous_tag) = previous
        .raw()
        .path()
        .rsplit_once('@')
        .map(|(_prefix, suffix)| suffix)
    else {
        return true;
    };

    // If the previous URL is a full commit hash, consider it incompatible.
    let previous_is_commit =
        previous_tag.len() == 40 && previous_tag.chars().all(|ch| ch.is_ascii_hexdigit());
    !previous_is_commit
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

    #[test]
    fn url_precision() -> Result<(), url::ParseError> {
        // Same repository, no tag on the previous URL, non-SHA on the overriding URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(!is_precise(&previous, &url));

        // Same repository, no tag on the previous URL, SHA on the overriding URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        let url = VerbatimUrl::parse_url(
            "git+https://example.com/MyProject.git@c3cd550a7a7c41b2c286ca52fbb6dec5fea195ef",
        )?;
        assert!(is_precise(&previous, &url));

        // Same repository, tag on the previous URL, SHA on the overriding URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url(
            "git+https://example.com/MyProject.git@c3cd550a7a7c41b2c286ca52fbb6dec5fea195ef",
        )?;
        assert!(is_precise(&previous, &url));

        // Same repository, SHA on the previous URL, different SHA on the overriding URL.
        let previous = VerbatimUrl::parse_url(
            "git+https://example.com/MyProject.git@5ae5980c885e350a34ca019a84ba14a2a228d262",
        )?;
        let url = VerbatimUrl::parse_url(
            "git+https://example.com/MyProject.git@c3cd550a7a7c41b2c286ca52fbb6dec5fea195ef",
        )?;
        assert!(!is_precise(&previous, &url));

        Ok(())
    }
}
