//! Utilities for retrying a network operation.
//!
//! Some network errors are considered "spurious", meaning it is not a real
//! error (such as a 404 not found) and is likely a transient error (like a
//! bad network connection) that we can hope will resolve itself shortly. The
//! [`Retry`] type offers a way to repeatedly perform some kind of network
//! operation with a delay if it detects one of these possibly transient
//! errors.
//!
//! This supports errors from [`git2`], [`reqwest`], and [`HttpNotSuccessful`]
//! 5xx HTTP errors.
//!
//! The number of retries can be configured by the user via the `net.retry`
//! config option. This indicates the number of times to retry the operation
//! (default 3 times for a total of 4 attempts).
//!
//! There are hard-coded constants that indicate how long to sleep between
//! retries. The constants are tuned to balance a few factors, such as the
//! responsiveness to the user (we don't want cargo to hang for too long
//! retrying things), and accommodating things like Cloudfront's default
//! negative TTL of 10 seconds (if Cloudfront gets a 5xx error for whatever
//! reason it won't try to fetch again for 10 seconds).
//!
//! The timeout also implements a primitive form of random jitter. This is so
//! that if multiple requests fail at the same time that they don't all flood
//! the server at the same time when they are retried. This jitter still has
//! some clumping behavior, but should be good enough.
//!
//! [`Retry`] is the core type for implementing retry logic. The
//! [`Retry::try`] method can be called with a callback, and it will
//! indicate if it needs to be called again sometime in the future if there
//! was a possibly transient error. The caller is responsible for sleeping the
//! appropriate amount of time and then calling [`Retry::try`] again.
//!
//! [`with_retry`] is a convenience function that will create a [`Retry`] and
//! handle repeatedly running a callback until it succeeds, or it runs out of
//! retries.
//!
//! Some interesting resources about retries:
//! - <https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/>
//! - <https://en.wikipedia.org/wiki/Exponential_backoff>
//! - <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Retry-After>

//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/util/network/retry.rs>
use std::cmp::min;
use std::time::Duration;

use anyhow::{Error, Result};
use rand::Rng;
use tracing::warn;

use crate::util::errors::HttpNotSuccessful;

/// State for managing retrying a network operation.
pub(crate) struct Retry {
    /// The number of failed attempts that have been done so far.
    ///
    /// Starts at 0, and increases by one each time an attempt fails.
    retries: u64,
    /// The maximum number of times the operation should be retried.
    ///
    /// 0 means it should never retry.
    max_retries: u64,
}

/// The result of attempting some operation via [`Retry::try`].
pub(crate) enum RetryResult<T> {
    /// The operation was successful.
    ///
    /// The wrapped value is the return value of the callback function.
    Success(T),
    /// The operation was an error, and it should not be tried again.
    Err(Error),
    /// The operation failed, and should be tried again in the future.
    ///
    /// The wrapped value is the number of milliseconds to wait before trying
    /// again. The caller is responsible for waiting this long and then
    /// calling [`Retry::try`] again.
    Retry(u64),
}

/// Maximum amount of time a single retry can be delayed (milliseconds).
const MAX_RETRY_SLEEP_MS: u64 = 10 * 1000;
/// The minimum initial amount of time a retry will be delayed (milliseconds).
///
/// The actual amount of time will be a random value above this.
const INITIAL_RETRY_SLEEP_BASE_MS: u64 = 500;
/// The maximum amount of additional time the initial retry will take (milliseconds).
///
/// The initial delay will be [`INITIAL_RETRY_SLEEP_BASE_MS`] plus a random range
/// from 0 to this value.
const INITIAL_RETRY_JITTER_MS: u64 = 1000;

impl Retry {
    pub(crate) fn new() -> Retry {
        Retry {
            retries: 0,
            max_retries: 3,
        }
    }

    /// Calls the given callback, and returns a [`RetryResult`] which
    /// indicates whether or not this needs to be called again at some point
    /// in the future to retry the operation if it failed.
    pub(crate) fn r#try<T>(&mut self, f: impl FnOnce() -> Result<T>) -> RetryResult<T> {
        match f() {
            Err(ref err) if maybe_spurious(err) && self.retries < self.max_retries => {
                let err_msg = err.downcast_ref::<HttpNotSuccessful>().map_or_else(
                    || err.root_cause().to_string(),
                    HttpNotSuccessful::to_string,
                );
                warn!(
                    "Spurious network error ({} tries remaining): {err_msg}",
                    self.max_retries - self.retries,
                );
                self.retries += 1;
                RetryResult::Retry(self.next_sleep_ms())
            }
            Err(e) => RetryResult::Err(e),
            Ok(r) => RetryResult::Success(r),
        }
    }

    /// Gets the next sleep duration in milliseconds.
    fn next_sleep_ms(&self) -> u64 {
        if self.retries == 1 {
            let mut rng = rand::thread_rng();
            INITIAL_RETRY_SLEEP_BASE_MS + rng.gen_range(0..INITIAL_RETRY_JITTER_MS)
        } else {
            min(
                ((self.retries - 1) * 3) * 1000 + INITIAL_RETRY_SLEEP_BASE_MS,
                MAX_RETRY_SLEEP_MS,
            )
        }
    }
}

fn maybe_spurious(err: &Error) -> bool {
    if let Some(git_err) = err.downcast_ref::<git2::Error>() {
        match git_err.class() {
            git2::ErrorClass::Net
            | git2::ErrorClass::Os
            | git2::ErrorClass::Zlib
            | git2::ErrorClass::Ssl
            | git2::ErrorClass::Http => return git_err.code() != git2::ErrorCode::Certificate,
            _ => (),
        }
    }
    if let Some(reqwest_err) = err.downcast_ref::<reqwest::Error>() {
        if reqwest_err.is_timeout()
            || reqwest_err.is_connect()
            || reqwest_err
                .status()
                .map_or(false, |status| status.is_server_error())
        {
            return true;
        }
    }
    if let Some(not_200) = err.downcast_ref::<HttpNotSuccessful>() {
        if 500 <= not_200.code && not_200.code < 600 {
            return true;
        }
    }

    false
}

/// Wrapper method for network call retry logic.
///
/// Retry counts provided by Config object `net.retry`. Config shell outputs
/// a warning on per retry.
///
/// Closure must return a `Result`.
pub(crate) fn with_retry<T, F>(mut callback: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut retry = Retry::new();
    loop {
        match retry.r#try(&mut callback) {
            RetryResult::Success(r) => return Ok(r),
            RetryResult::Err(e) => return Err(e),
            RetryResult::Retry(sleep) => std::thread::sleep(Duration::from_millis(sleep)),
        }
    }
}
