/// Shared credential matching logic used by both text and native stores.
///
/// This module provides utilities for matching stored credentials against
/// requested URLs using realm and path prefix matching.
use uv_redacted::DisplaySafeUrl;

use crate::{Realm, Service, Username};

/// Return `true` if `stored_path` applies to `request_path`.
///
/// This treats stored paths as hierarchical prefixes, so `/api` matches `/api`
/// and `/api/v1`, but not `/apiv1`.
fn path_prefix_matches(stored_path: &str, request_path: &str) -> bool {
    if request_path == stored_path {
        return true;
    }

    let Some(remainder) = request_path.strip_prefix(stored_path) else {
        return false;
    };

    stored_path.ends_with('/') || remainder.starts_with('/')
}

/// Check if a stored credential matches a request URL with username filtering.
///
/// This performs:
/// 1. Realm matching (scheme://host:port must be equal)
/// 2. Path prefix matching (stored path must be a prefix of request path)
/// 3. Username matching (if username provided, it must match)
///
/// Returns `true` if the credential matches the request.
pub(crate) fn credential_matches(
    service: &Service,
    stored_username: &Username,
    request_url: &DisplaySafeUrl,
    request_realm: &Realm,
    request_username: Option<&str>,
) -> bool {
    let service_realm = Realm::from(service.url());

    // Only consider services in the same realm
    if service_realm != *request_realm {
        return false;
    }

    // Service path must be a path-segment prefix of request path.
    if !path_prefix_matches(service.url().path(), request_url.path()) {
        return false;
    }

    // If a username is provided, it must match
    if let Some(req_username) = request_username {
        if Some(req_username) != stored_username.as_deref() {
            return false;
        }
    }

    true
}

/// Check if a credential matches and return its specificity score.
///
/// The specificity is the length of the service path, which determines
/// which credential wins when multiple credentials match (longest path wins).
///
/// Returns `Some(specificity)` if the credential matches, `None` otherwise.
pub(crate) fn match_specificity(
    service: &Service,
    stored_username: &Username,
    request_url: &DisplaySafeUrl,
    request_realm: &Realm,
    request_username: Option<&str>,
) -> Option<usize> {
    if credential_matches(
        service,
        stored_username,
        request_url,
        request_realm,
        request_username,
    ) {
        Some(service.url().path().len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_redacted::DisplaySafeUrl;

    use super::{credential_matches, path_prefix_matches};
    use crate::{Realm, Service, Username};

    #[test]
    fn path_prefix_matches_respects_segment_boundaries() {
        assert!(path_prefix_matches("/api", "/api"));
        assert!(path_prefix_matches("/api", "/api/v1"));
        assert!(path_prefix_matches("/", "/anything"));

        assert!(!path_prefix_matches("/api", "/apiv1"));
        assert!(!path_prefix_matches("/api", "/api-private"));
    }

    #[test]
    fn credential_matches_rejects_sibling_paths() {
        let service = Service::from_str("https://example.com/api").unwrap();
        let request_url = DisplaySafeUrl::parse("https://example.com/apiv1").unwrap();
        let request_realm = Realm::from(&request_url);

        assert!(!credential_matches(
            &service,
            &Username::from(Some("user".to_string())),
            &request_url,
            &request_realm,
            Some("user"),
        ));
    }
}
