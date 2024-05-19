use std::sync::{Arc, Mutex};

use anyhow::Result;
use fs_err::tokio as fs;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use tracing::debug;
use url::Url;

use cache_key::{CanonicalUrl, RepositoryUrl};
use distribution_types::ParsedGitUrl;
use uv_cache::{Cache, CacheBucket};
use uv_fs::LockedFile;
use uv_git::{Fetch, GitReference, GitSha, GitSource, GitUrl};

use crate::error::Error;
use crate::reporter::Facade;
use crate::Reporter;

/// Global cache of resolved Git references.
///
/// Used to ensure that a given Git URL is only resolved once, and that the resolved URL is
/// consistent across all invocations. (For example: if a Git URL refers to a branch, like `main`,
/// then the resolved URL should always refer to the same commit across the lifetime of the
/// process.)
static RESOLVED_GIT_REFS: Lazy<Mutex<FxHashMap<RepositoryReference, GitSha>>> =
    Lazy::new(Mutex::default);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RepositoryReference {
    /// The URL of the Git repository, with any query parameters and fragments removed.
    url: RepositoryUrl,
    /// The reference to the commit to use, which could be a branch, tag or revision.
    reference: GitReference,
}

impl RepositoryReference {
    fn new(git: &GitUrl) -> Self {
        Self {
            url: RepositoryUrl::new(git.repository()),
            reference: git.reference().clone(),
        }
    }
}

/// Download a source distribution from a Git repository.
///
/// Assumes that the URL is a precise Git URL, with a full commit hash.
pub(crate) async fn fetch_git_archive(
    url: &GitUrl,
    cache: &Cache,
    reporter: Option<&Arc<dyn Reporter>>,
) -> Result<Fetch, Error> {
    debug!("Fetching source distribution from Git: {url}");
    let git_dir = cache.bucket(CacheBucket::Git);

    // Avoid races between different processes, too.
    let lock_dir = git_dir.join("locks");
    fs::create_dir_all(&lock_dir)
        .await
        .map_err(Error::CacheWrite)?;
    let repository_url = RepositoryUrl::new(url.repository());
    let _lock = LockedFile::acquire(
        lock_dir.join(cache_key::digest(&repository_url)),
        &repository_url,
    )
    .map_err(Error::CacheWrite)?;

    // Fetch the Git repository.
    let source = if let Some(reporter) = reporter {
        GitSource::new(url.clone(), git_dir).with_reporter(Facade::from(reporter.clone()))
    } else {
        GitSource::new(url.clone(), git_dir)
    };
    let fetch = tokio::task::spawn_blocking(move || source.fetch())
        .await?
        .map_err(Error::Git)?;

    Ok(fetch)
}

/// Given a remote source distribution, return a precise variant, if possible.
///
/// For example, given a Git dependency with a reference to a branch or tag, return a URL
/// with a precise reference to the current commit of that branch or tag.
///
/// This method takes into account various normalizations that are independent from the Git
/// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
/// prefix kinds.
pub(crate) async fn resolve_precise(
    url: &GitUrl,
    cache: &Cache,
    reporter: Option<&Arc<dyn Reporter>>,
) -> Result<Option<GitUrl>, Error> {
    // If the Git reference already contains a complete SHA, short-circuit.
    if url.precise().is_some() {
        return Ok(None);
    }

    // If the Git reference is in the in-memory cache, return it.
    {
        let resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        let reference = RepositoryReference::new(url);
        if let Some(precise) = resolved_git_refs.get(&reference) {
            return Ok(Some(url.clone().with_precise(*precise)));
        }
    }

    let git_dir = cache.bucket(CacheBucket::Git);

    // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
    // commit, etc.).
    let source = if let Some(reporter) = reporter {
        GitSource::new(url.clone(), git_dir).with_reporter(Facade::from(reporter.clone()))
    } else {
        GitSource::new(url.clone(), git_dir)
    };
    let fetch = tokio::task::spawn_blocking(move || source.fetch())
        .await?
        .map_err(Error::Git)?;
    let git = fetch.into_git();

    // Insert the resolved URL into the in-memory cache.
    if let Some(precise) = git.precise() {
        let mut resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        let reference = RepositoryReference::new(url);
        resolved_git_refs.insert(reference, precise);
    }

    Ok(Some(git))
}

/// Given a remote source distribution, return a precise variant, if possible.
///
/// For example, given a Git dependency with a reference to a branch or tag, return a URL
/// with a precise reference to the current commit of that branch or tag.
///
/// This method takes into account various normalizations that are independent from the Git
/// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
/// prefix kinds.
///
/// This method will only return precise URLs for URLs that have already been resolved via
/// [`resolve_precise`].
pub fn git_url_to_precise(url: GitUrl) -> Option<GitUrl> {
    let resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
    let reference = RepositoryReference::new(&url);
    let precise = resolved_git_refs.get(&reference)?;
    Some(url.with_precise(*precise))
}

/// Returns `true` if the URLs refer to the same Git commit.
///
/// For example, the previous URL could be a branch or tag, while the current URL would be a
/// precise commit hash.
pub fn is_same_reference<'a>(a: &'a Url, b: &'a Url) -> bool {
    let resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
    is_same_reference_impl(a, b, &resolved_git_refs)
}

/// Returns `true` if the URLs refer to the same Git commit.
///
/// Like [`is_same_reference`], but accepts a resolved reference cache for testing.
fn is_same_reference_impl<'a>(
    a: &'a Url,
    b: &'a Url,
    resolved_refs: &FxHashMap<RepositoryReference, GitSha>,
) -> bool {
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

    // Convert `a` to a repository URL.
    let a_ref = RepositoryReference::new(&a_git.url);

    // Convert `b` to a repository URL.
    let b_ref = RepositoryReference::new(&b_git.url);

    // The URLs must refer to the same repository.
    if a_ref.url != b_ref.url {
        return false;
    }

    // If the URLs have the same tag, they refer to the same commit.
    if a_ref.reference == b_ref.reference {
        return true;
    }

    // Otherwise, the URLs must resolve to the same precise commit.
    let Some(a_precise) = a_git
        .url
        .precise()
        .or_else(|| resolved_refs.get(&a_ref).copied())
    else {
        return false;
    };

    let Some(b_precise) = b_git
        .url
        .precise()
        .or_else(|| resolved_refs.get(&b_ref).copied())
    else {
        return false;
    };

    a_precise == b_precise
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustc_hash::FxHashMap;
    use std::str::FromStr;
    use url::Url;

    use crate::git::RepositoryReference;
    use uv_git::{GitSha, GitUrl};

    #[test]
    fn same_reference() -> Result<()> {
        let empty = FxHashMap::default();

        // Same repository, same tag.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main")?;
        assert!(super::is_same_reference_impl(&a, &b, &empty));

        // Same repository, same tag, same subdirectory.
        let a = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        assert!(super::is_same_reference_impl(&a, &b, &empty));

        // Different repositories, same tag.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyOtherProject.git@main")?;
        assert!(!super::is_same_reference_impl(&a, &b, &empty));

        // Same repository, different tags.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse("git+https://example.com/MyProject.git@v1.0")?;
        assert!(!super::is_same_reference_impl(&a, &b, &empty));

        // Same repository, same tag, different subdirectory.
        let a = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=pkg_dir")?;
        let b = Url::parse("git+https://example.com/MyProject.git@main#subdirectory=other_dir")?;
        assert!(!super::is_same_reference_impl(&a, &b, &empty));

        // Same repository, different tags, but same precise commit.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse(
            "git+https://example.com/MyProject.git@164a8735b081663fede48c5041667b194da15d25",
        )?;
        let mut resolved_refs = FxHashMap::default();
        resolved_refs.insert(
            RepositoryReference::new(&GitUrl::try_from(Url::parse(
                "https://example.com/MyProject@main",
            )?)?),
            GitSha::from_str("164a8735b081663fede48c5041667b194da15d25")?,
        );
        assert!(super::is_same_reference_impl(&a, &b, &resolved_refs));

        // Same repository, different tags, different precise commit.
        let a = Url::parse("git+https://example.com/MyProject.git@main")?;
        let b = Url::parse(
            "git+https://example.com/MyProject.git@164a8735b081663fede48c5041667b194da15d25",
        )?;
        let mut resolved_refs = FxHashMap::default();
        resolved_refs.insert(
            RepositoryReference::new(&GitUrl::try_from(Url::parse(
                "https://example.com/MyProject@main",
            )?)?),
            GitSha::from_str("f2c9e88f3ec9526bbcec68d150b176d96a750aba")?,
        );
        assert!(!super::is_same_reference_impl(&a, &b, &resolved_refs));

        Ok(())
    }
}
