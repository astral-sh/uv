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
