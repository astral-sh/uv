use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

/// A global state on whether we are being rate-limited by GitHub's REST API.
/// If we are, avoid "fast-path" attempts.
pub(crate) static GITHUB_RATE_LIMIT_STATUS: LazyLock<GitHubRateLimitStatus> =
    LazyLock::new(GitHubRateLimitStatus::default);

/// GitHub REST API rate limit status tracker.
///
/// ## Assumptions
///
/// The rate limit timeout duration is much longer than the runtime of a `uv` command.
/// And so we do not need to invalidate this state based on `x-ratelimit-reset`.
#[derive(Debug, Default)]
pub(crate) struct GitHubRateLimitStatus(AtomicBool);

impl GitHubRateLimitStatus {
    pub(crate) fn activate(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
