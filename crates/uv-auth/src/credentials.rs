use std::borrow::Cow;
use std::str::FromStr;

use http::Uri;
use http::header::InvalidHeaderValue;
use reqsign::aws::DefaultSigner as AwsDefaultSigner;
use reqsign::azure::DefaultSigner as AzureDefaultSigner;
use reqsign::google::DefaultSigner as GcsDefaultSigner;
use reqwest::Request;
use thiserror::Error;

pub use uv_auth_types::{Credentials, CredentialsFromUrlError, Username};
pub(crate) use uv_auth_types::{Password, Token};

pub(crate) trait CredentialsExt: Sized {
    fn from_request(request: &Request) -> Result<Option<Self>, CredentialsFromUrlError>;
    fn authenticate(&self, request: Request) -> Result<Request, InvalidHeaderValue>;
}

impl CredentialsExt for Credentials {
    /// Parse [`Credentials`] from an HTTP request, if any.
    ///
    /// Only HTTP Basic Authentication is supported.
    fn from_request(request: &Request) -> Result<Option<Self>, CredentialsFromUrlError> {
        // First, attempt to retrieve the credentials from the URL
        if let Some(credentials) = Self::from_url(request.url())? {
            return Ok(Some(credentials));
        }

        // Then, attempt to pull the credentials from the headers
        Ok(request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .and_then(Self::from_header_value))
    }

    /// Attach the credentials to the given request.
    ///
    /// Any existing credentials will be overridden.
    fn authenticate(&self, mut request: Request) -> Result<Request, InvalidHeaderValue> {
        request
            .headers_mut()
            .insert(reqwest::header::AUTHORIZATION, Self::to_header_value(self)?);
        Ok(request)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Authentication {
    /// HTTP Basic or Bearer Authentication credentials.
    Credentials(Credentials),

    /// AWS Signature Version 4 signing.
    AwsSigner(AwsDefaultSigner),

    /// Google Cloud signing.
    GcsSigner(GcsDefaultSigner),

    /// Azure Storage signing.
    AzureSigner(AzureDefaultSigner),
}

#[derive(Debug, Error)]
pub(crate) enum AuthenticationError {
    #[error("Invalid authorization header")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error("Failed to convert request URL to URI")]
    InvalidUri(#[from] http::uri::InvalidUri),

    #[error("Failed to build request for {provider} signing")]
    BuildRequest {
        provider: &'static str,
        #[source]
        source: http::Error,
    },

    #[error("Failed to sign request with {provider} credentials")]
    Sign {
        provider: &'static str,
        #[source]
        source: reqsign::Error,
    },
}

impl PartialEq for Authentication {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Credentials(a), Self::Credentials(b)) => a == b,
            (Self::AwsSigner(..), Self::AwsSigner(..)) => true,
            (Self::GcsSigner(..), Self::GcsSigner(..)) => true,
            (Self::AzureSigner(..), Self::AzureSigner(..)) => true,
            _ => false,
        }
    }
}

impl Eq for Authentication {}

impl From<Credentials> for Authentication {
    fn from(credentials: Credentials) -> Self {
        Self::Credentials(credentials)
    }
}

impl From<AwsDefaultSigner> for Authentication {
    fn from(signer: AwsDefaultSigner) -> Self {
        Self::AwsSigner(signer)
    }
}

impl From<GcsDefaultSigner> for Authentication {
    fn from(signer: GcsDefaultSigner) -> Self {
        Self::GcsSigner(signer)
    }
}

impl From<AzureDefaultSigner> for Authentication {
    fn from(signer: AzureDefaultSigner) -> Self {
        Self::AzureSigner(signer)
    }
}

impl Authentication {
    /// Return the password used for authentication, if any.
    pub(crate) fn password(&self) -> Option<&str> {
        match self {
            Self::Credentials(credentials) => credentials.password(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => None,
        }
    }

    /// Return the username used for authentication, if any.
    pub(crate) fn username(&self) -> Option<&str> {
        match self {
            Self::Credentials(credentials) => credentials.username(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => None,
        }
    }

    /// Return the username used for authentication, if any.
    pub(crate) fn as_username(&self) -> Cow<'_, Username> {
        match self {
            Self::Credentials(credentials) => credentials.as_username(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => {
                Cow::Owned(Username::none())
            }
        }
    }

    /// Return the username used for authentication, if any.
    pub(crate) fn to_username(&self) -> Username {
        match self {
            Self::Credentials(credentials) => credentials.to_username(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => Username::none(),
        }
    }

    /// Return `true` if the object contains a means of authenticating.
    pub(crate) fn is_authenticated(&self) -> bool {
        match self {
            Self::Credentials(credentials) => credentials.is_authenticated(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => true,
        }
    }

    /// Return `true` if the object contains no credentials.
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Credentials(credentials) => credentials.is_empty(),
            Self::AwsSigner(..) | Self::GcsSigner(..) | Self::AzureSigner(..) => false,
        }
    }

    /// Apply the authentication to the given request.
    ///
    /// Any existing credentials will be overridden.
    pub(crate) async fn authenticate(
        &self,
        mut request: Request,
    ) -> Result<Request, AuthenticationError> {
        match self {
            Self::Credentials(credentials) => Ok(credentials.authenticate(request)?),
            Self::AwsSigner(signer) => {
                // Build an `http::Request` from the `reqwest::Request`.
                let uri = Uri::from_str(request.url().as_str())?;
                let mut http_req = http::Request::builder()
                    .method(request.method().clone())
                    .uri(uri)
                    .body(())
                    .map_err(|source| AuthenticationError::BuildRequest {
                        provider: "AWS",
                        source,
                    })?;
                *http_req.headers_mut() = request.headers().clone();

                // Sign the parts.
                let (mut parts, ()) = http_req.into_parts();
                signer.sign(&mut parts, None).await.map_err(|source| {
                    AuthenticationError::Sign {
                        provider: "AWS",
                        source,
                    }
                })?;

                // Copy over the signed headers.
                request.headers_mut().extend(parts.headers);

                // Copy over the signed path and query, if any.
                if let Some(path_and_query) = parts.uri.path_and_query() {
                    request.url_mut().set_path(path_and_query.path());
                    request.url_mut().set_query(path_and_query.query());
                }
                Ok(request)
            }
            Self::GcsSigner(signer) => {
                // Build an `http::Request` from the `reqwest::Request`.
                let uri = Uri::from_str(request.url().as_str())?;
                let mut http_req = http::Request::builder()
                    .method(request.method().clone())
                    .uri(uri)
                    .body(())
                    .map_err(|source| AuthenticationError::BuildRequest {
                        provider: "GCS",
                        source,
                    })?;
                *http_req.headers_mut() = request.headers().clone();

                // Sign the parts.
                let (mut parts, ()) = http_req.into_parts();
                signer.sign(&mut parts, None).await.map_err(|source| {
                    AuthenticationError::Sign {
                        provider: "GCS",
                        source,
                    }
                })?;

                // Copy over the signed headers.
                request.headers_mut().extend(parts.headers);

                // Copy over the signed path and query, if any.
                if let Some(path_and_query) = parts.uri.path_and_query() {
                    request.url_mut().set_path(path_and_query.path());
                    request.url_mut().set_query(path_and_query.query());
                }
                Ok(request)
            }
            Self::AzureSigner(signer) => {
                // Build an `http::Request` from the `reqwest::Request`.
                let uri = Uri::from_str(request.url().as_str())?;
                let mut http_req = http::Request::builder()
                    .method(request.method().clone())
                    .uri(uri)
                    .body(())
                    .map_err(|source| AuthenticationError::BuildRequest {
                        provider: "Azure",
                        source,
                    })?;
                *http_req.headers_mut() = request.headers().clone();

                // Sign the parts.
                let (mut parts, ()) = http_req.into_parts();
                signer.sign(&mut parts, None).await.map_err(|source| {
                    AuthenticationError::Sign {
                        provider: "Azure",
                        source,
                    }
                })?;

                // Copy over the signed headers.
                request.headers_mut().extend(parts.headers);

                // Copy over the signed path and query, if any.
                if let Some(path_and_query) = parts.uri.path_and_query() {
                    request.url_mut().set_path(path_and_query.path());
                    request.url_mut().set_query(path_and_query.query());
                }
                Ok(request)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::future::{self, Future};

    use insta::{assert_debug_snapshot, assert_snapshot};
    use reqsign::aws::Credential as AwsCredential;
    use reqsign::azure::Credential as AzureCredential;
    use reqsign::{Context, ProvideCredential};
    use url::Url;

    use super::*;

    #[derive(Debug)]
    struct EmptyAwsCredentialProvider;

    impl ProvideCredential for EmptyAwsCredentialProvider {
        type Credential = AwsCredential;

        fn provide_credential(
            &self,
            _ctx: &Context,
        ) -> impl Future<Output = reqsign::Result<Option<Self::Credential>>> {
            future::ready(Ok(None))
        }
    }

    #[derive(Debug)]
    struct EmptyAzureCredentialProvider;

    impl ProvideCredential for EmptyAzureCredentialProvider {
        type Credential = AzureCredential;

        fn provide_credential(
            &self,
            _ctx: &Context,
        ) -> impl Future<Output = reqsign::Result<Option<Self::Credential>>> {
            future::ready(Ok(None))
        }
    }

    #[test]
    fn from_url_no_credentials() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        assert_matches!(Credentials::from_url(url), Ok(None));
    }

    #[test]
    fn from_url_username_and_password() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();
        assert_eq!(credentials.username(), Some("user"));
        assert_eq!(credentials.password(), Some("password"));
    }

    #[test]
    fn from_url_invalid_utf8_username() {
        let url = Url::parse("https://%FF:password@example.com/simple/first/").unwrap();
        let error = Credentials::from_url(&url).unwrap_err();
        assert_snapshot!(error, @"URL username contains invalid UTF-8");
    }

    #[test]
    fn from_url_invalid_utf8_password() {
        let url = Url::parse("https://user:%FF@example.com/simple/first/").unwrap();
        let error = Credentials::from_url(&url).unwrap_err();
        assert_snapshot!(error, @"URL password contains invalid UTF-8");
    }

    #[test]
    fn from_url_no_username() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();
        assert_eq!(credentials.username(), None);
        assert_eq!(credentials.password(), Some("password"));
    }

    /// Test for <https://github.com/astral-sh/uv/issues/17343>
    ///
    /// URLs with an empty username but a password (e.g., `https://:token@example.com`)
    /// should be recognized as having credentials.
    #[test]
    fn from_url_empty_username_with_password() {
        // Parse a URL with the format `:password@host` directly
        let url = Url::parse("https://:token@example.com/simple/first/").unwrap();
        let credentials = Credentials::from_url(&url).unwrap().unwrap();
        assert_eq!(credentials.username(), None);
        assert_eq!(credentials.password(), Some("token"));
        assert!(
            credentials.is_authenticated(),
            "URL with empty username but password should be considered authenticated"
        );
    }

    #[test]
    fn from_url_no_password() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();
        assert_eq!(credentials.username(), Some("user"));
        assert_eq!(credentials.password(), None);
    }

    #[test]
    fn authenticated_request_from_url() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request).unwrap();

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r#""Basic dXNlcjpwYXNzd29yZA==""#);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_user() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user@domain").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request).unwrap();

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r#""Basic dXNlckBkb21haW46cGFzc3dvcmQ=""#);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_password() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password==")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap().unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request).unwrap();

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r#""Basic dXNlcjpwYXNzd29yZD09""#);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    #[tokio::test]
    async fn authenticated_request_with_azure_signer() {
        let signer = reqsign::azure::default_signer().with_credential_provider(
            reqsign::azure::StaticCredentialProvider::new_bearer_token("token"),
        );
        let authentication = Authentication::from(signer);

        let request = Request::new(
            reqwest::Method::GET,
            Url::parse("https://account.blob.core.windows.net/container/blob.whl").unwrap(),
        );
        let request = authentication.authenticate(request).await.unwrap();

        let authorization = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set");
        assert_eq!(authorization.to_str().unwrap(), "Bearer token");
        assert!(request.headers().contains_key("x-ms-date"));
    }

    #[tokio::test]
    async fn authenticated_request_with_aws_signer_missing_credentials() {
        let signer = reqsign::aws::default_signer("s3", "us-east-1")
            .with_credential_provider(EmptyAwsCredentialProvider);
        let authentication = Authentication::from(signer);

        let request = Request::new(
            reqwest::Method::GET,
            Url::parse("https://s3.amazonaws.com/bucket/blob.whl").unwrap(),
        );
        let err = authentication.authenticate(request).await.unwrap_err();

        insta::assert_snapshot!(
            err.to_string(),
            @"Failed to sign request with AWS credentials"
        );
    }

    #[tokio::test]
    async fn authenticated_request_with_azure_signer_missing_credentials() {
        let signer =
            reqsign::azure::default_signer().with_credential_provider(EmptyAzureCredentialProvider);
        let authentication = Authentication::from(signer);

        let request = Request::new(
            reqwest::Method::GET,
            Url::parse("https://account.blob.core.windows.net/container/blob.whl").unwrap(),
        );
        let err = authentication.authenticate(request).await.unwrap_err();

        insta::assert_snapshot!(
            err.to_string(),
            @"Failed to sign request with Azure credentials"
        );
    }

    /// Passwords should be redacted in debug output.
    #[test]
    fn test_password_redaction() {
        let credentials =
            Credentials::basic(Some(String::from("user")), Some(String::from("password")));
        insta::assert_compact_debug_snapshot!(credentials, @r#"Basic { username: Username(Some("user")), password: Some(****) }"#);
    }

    /// Bearer credentials should be redacted in debug output.
    #[test]
    fn test_bearer_token_redaction() {
        let token = "super_secret_token";
        let credentials = Credentials::bearer(token.into());
        insta::assert_compact_debug_snapshot!(credentials, @"Bearer { token: **** }");
    }
}
