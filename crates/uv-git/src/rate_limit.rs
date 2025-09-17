use reqwest::{Response, StatusCode};
use std::sync::atomic::{AtomicBool, Ordering};

/// A global state on whether we are being rate-limited by GitHub's REST API.
/// If we are, avoid "fast-path" attempts.
pub(crate) static GITHUB_RATE_LIMIT_STATUS: GitHubRateLimitStatus = GitHubRateLimitStatus::new();

/// GitHub REST API rate limit status tracker.
///
/// ## Assumptions
///
/// The rate limit timeout duration is much longer than the runtime of a `uv` command.
/// And so we do not need to invalidate this state based on `x-ratelimit-reset`.
#[derive(Debug)]
pub(crate) struct GitHubRateLimitStatus(AtomicBool);

impl GitHubRateLimitStatus {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub(crate) fn activate(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Determine if GitHub is applying rate-limiting based on the response
pub(crate) fn is_github_rate_limited(response: &Response) -> bool {
    // HTTP 403 and 429 are possible status codes in the event of a primary or secondary rate limit.
    // Source: https://docs.github.com/en/rest/using-the-rest-api/troubleshooting-the-rest-api?apiVersion=2022-11-28#rate-limit-errors
    let status_code = response.status();
    status_code == StatusCode::FORBIDDEN || status_code == StatusCode::TOO_MANY_REQUESTS
}
