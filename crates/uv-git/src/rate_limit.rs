use reqwest::Response;
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
    pub(crate) const fn new() -> Self {
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
    // Check if our remaining quota is within the rate limit window.
    // If it's "0", mark that we are currently being rate-limited by GitHub.
    // Source: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api?apiVersion=2022-11-28#checking-the-status-of-your-rate-limit.
    response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|h| h.to_str().ok())
        == Some("0")
}
