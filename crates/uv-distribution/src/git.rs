use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use fs_err::tokio as fs;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use tracing::debug;
use url::Url;

use distribution_types::{DirectGitUrl, SourceDist};
use uv_cache::{Cache, CacheBucket};
use uv_fs::LockedFile;
use uv_git::{Fetch, GitSource, GitUrl};

use crate::error::Error;
use crate::reporter::Facade;
use crate::Reporter;

/// Global cache of resolved Git references.
///
/// Used to ensure that a given Git URL is only resolved once, and that the resolved URL is
/// consistent across all invocations. (For example: if a Git URL refers to a branch, like `main`,
/// then the resolved URL should always refer to the same commit across the lifetime of the
/// process.)
static RESOLVED_GIT_REFS: Lazy<Mutex<FxHashMap<GitUrl, GitUrl>>> = Lazy::new(Mutex::default);

/// Download a source distribution from a Git repository.
pub(crate) async fn fetch_git_archive(
    url: &Url,
    cache: &Cache,
    reporter: Option<&Arc<dyn Reporter>>,
) -> Result<(Fetch, Option<PathBuf>), Error> {
    debug!("Fetching source distribution from Git: {url}");
    let git_dir = cache.bucket(CacheBucket::Git);

    // Avoid races between different processes, too.
    let lock_dir = git_dir.join("locks");
    fs::create_dir_all(&lock_dir)
        .await
        .map_err(Error::CacheWrite)?;
    let canonical_url = cache_key::CanonicalUrl::new(url);
    let _lock = LockedFile::acquire(
        lock_dir.join(cache_key::digest(&canonical_url)),
        &canonical_url,
    )
    .map_err(Error::CacheWrite)?;

    let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(url).map_err(Error::Git)?;

    // Extract the resolved URL from the in-memory cache, to save a look-up in the fetch.
    let url = {
        let resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        if let Some(resolved) = resolved_git_refs.get(&url) {
            resolved.clone()
        } else {
            url
        }
    };

    // Fetch the Git repository.
    let source = if let Some(reporter) = reporter {
        GitSource::new(url.clone(), git_dir).with_reporter(Facade::from(reporter.clone()))
    } else {
        GitSource::new(url.clone(), git_dir)
    };
    let fetch = tokio::task::spawn_blocking(move || source.fetch())
        .await?
        .map_err(Error::Git)?;

    // Insert the resolved URL into the in-memory cache.
    {
        let mut resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        let precise = fetch.git().clone();
        resolved_git_refs.insert(url, precise);
    }

    Ok((fetch, subdirectory))
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
    dist: &SourceDist,
    cache: &Cache,
    reporter: Option<&Arc<dyn Reporter>>,
) -> Result<Option<Url>, Error> {
    let SourceDist::Git(source_dist) = dist else {
        return Ok(None);
    };
    let git_dir = cache.bucket(CacheBucket::Git);

    let DirectGitUrl { url, subdirectory } =
        DirectGitUrl::try_from(source_dist.url.raw()).map_err(Error::Git)?;

    // If the Git reference already contains a complete SHA, short-circuit.
    if url.precise().is_some() {
        return Ok(None);
    }

    // If the Git reference is in the in-memory cache, return it.
    {
        let resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        if let Some(precise) = resolved_git_refs.get(&url) {
            return Ok(Some(Url::from(DirectGitUrl {
                url: precise.clone(),
                subdirectory,
            })));
        }
    }

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
    let precise = fetch.into_git();

    // Insert the resolved URL into the in-memory cache.
    {
        let mut resolved_git_refs = RESOLVED_GIT_REFS.lock().unwrap();
        resolved_git_refs.insert(url.clone(), precise.clone());
    }

    // Re-encode as a URL.
    Ok(Some(Url::from(DirectGitUrl {
        url: precise,
        subdirectory,
    })))
}
