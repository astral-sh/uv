//! Shared credential matching for persistent stores.

use uv_redacted::DisplaySafeUrl;

use crate::index::is_path_prefix;
use crate::{RealmRef, Service};

/// The best credential match is not unique.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AmbiguousCredential;

/// Return the path specificity when a stored credential matches the request.
fn match_specificity(
    service: &Service,
    stored_username: Option<&str>,
    request_url: &DisplaySafeUrl,
    request_realm: RealmRef<'_>,
    request_username: Option<&str>,
) -> Option<usize> {
    if RealmRef::from(&**service.url()) != request_realm
        || !is_path_prefix(service.url().path(), request_url.path())
        || request_username.is_some_and(|username| stored_username != Some(username))
    {
        return None;
    }

    Some(service.url().path().len())
}

/// Select the most specific credential matching a URL and optional username.
pub(crate) fn select_credential<'a, T>(
    credentials: impl IntoIterator<Item = (&'a Service, Option<&'a str>, &'a T)>,
    request_url: &DisplaySafeUrl,
    request_username: Option<&str>,
) -> Result<Option<&'a T>, AmbiguousCredential> {
    let request_realm = RealmRef::from(&**request_url);
    let mut best = None;
    let mut best_is_ambiguous = false;

    for (service, stored_username, credential) in credentials {
        if let Some(path_specificity) = match_specificity(
            service,
            stored_username,
            request_url,
            request_realm,
            request_username,
        ) {
            let exact = service.url() == request_url && stored_username == request_username;
            let rank = (exact, path_specificity);
            if best.is_none_or(|(best_rank, _)| rank > best_rank) {
                best = Some((rank, credential));
                best_is_ambiguous = false;
            } else if best.is_some_and(|(best_rank, _)| rank == best_rank) {
                best_is_ambiguous = true;
            }
        }
    }

    if best_is_ambiguous {
        return Err(AmbiguousCredential);
    }

    Ok(best.map(|(_, credential)| credential))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_redacted::DisplaySafeUrl;

    use super::{match_specificity, select_credential};
    use crate::{RealmRef, Service};

    #[test]
    fn match_specificity_rejects_sibling_paths() {
        let service =
            Service::from_str("https://example.com/api").expect("service URL should be valid");
        let request_url = DisplaySafeUrl::parse("https://example.com/apiv1")
            .expect("request URL should be valid");
        assert_eq!(
            match_specificity(
                &service,
                Some("user"),
                &request_url,
                RealmRef::from(&*request_url),
                Some("user"),
            ),
            None
        );
    }

    #[test]
    fn select_credential_prefers_the_most_specific_path() {
        let root =
            Service::from_str("https://example.com").expect("root service URL should be valid");
        let api =
            Service::from_str("https://example.com/api").expect("API service URL should be valid");
        let request = DisplaySafeUrl::parse("https://example.com/api/project")
            .expect("request URL should be valid");
        let root_value = "root";
        let api_value = "api";

        let selected = select_credential(
            [
                (&root, Some("user"), &root_value),
                (&api, Some("user"), &api_value),
            ],
            &request,
            Some("user"),
        )
        .expect("credential match should not be ambiguous");

        assert_eq!(selected, Some(&api_value));
    }

    #[test]
    fn select_credential_rejects_equally_specific_users() {
        let service =
            Service::from_str("https://example.com/api").expect("service URL should be valid");
        let request = DisplaySafeUrl::parse("https://example.com/api/project")
            .expect("request URL should be valid");
        let first = "first";
        let second = "second";

        assert!(
            select_credential(
                [
                    (&service, Some("first"), &first),
                    (&service, Some("second"), &second),
                ],
                &request,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn select_credential_ignores_less_specific_ambiguity() {
        let root =
            Service::from_str("https://example.com").expect("root service URL should be valid");
        let api =
            Service::from_str("https://example.com/api").expect("API service URL should be valid");
        let request = DisplaySafeUrl::parse("https://example.com/api/project")
            .expect("request URL should be valid");
        let first_root = "first-root";
        let second_root = "second-root";
        let api_value = "api";

        let selected = select_credential(
            [
                (&root, Some("first"), &first_root),
                (&root, Some("second"), &second_root),
                (&api, Some("api"), &api_value),
            ],
            &request,
            None,
        )
        .expect("less-specific matches should not make the result ambiguous");

        assert_eq!(selected, Some(&api_value));
    }

    #[test]
    fn select_credential_prefers_an_exact_query_match() {
        let first = Service::from_str("https://example.com/api?signature=first")
            .expect("first service URL should be valid");
        let second = Service::from_str("https://example.com/api?signature=second")
            .expect("second service URL should be valid");
        let request = DisplaySafeUrl::parse("https://example.com/api?signature=second")
            .expect("request URL should be valid");
        let first_value = "first";
        let second_value = "second";

        let selected = select_credential(
            [
                (&first, Some("user"), &first_value),
                (&second, Some("user"), &second_value),
            ],
            &request,
            Some("user"),
        )
        .expect("the exact service should resolve the prefix ambiguity");

        assert_eq!(selected, Some(&second_value));
    }
}
