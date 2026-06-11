//! Format-neutral schema for credentials persisted by authentication stores.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::credentials::{Password, Token, Username};
use crate::{Credentials, Service};

/// Authentication scheme used by a persisted credential.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    /// HTTP Basic authentication with a username and password.
    #[default]
    Basic,
    /// Bearer authentication with a token.
    Bearer,
}

#[derive(Debug, Error)]
pub(crate) enum BasicAuthError {
    #[error("`username` is required with `scheme = basic`")]
    MissingUsername,
    #[error("`token` cannot be provided with `scheme = basic`")]
    UnexpectedToken,
}

#[derive(Debug, Error)]
pub(crate) enum BearerAuthError {
    #[error("`token` is required with `scheme = bearer`")]
    MissingToken,
    #[error("`username` cannot be provided with `scheme = bearer`")]
    UnexpectedUsername,
    #[error("`password` cannot be provided with `scheme = bearer`")]
    UnexpectedPassword,
}

/// An invalid persisted credential representation.
#[derive(Debug, Error)]
pub(crate) enum PersistentCredentialError {
    #[error(transparent)]
    Basic(#[from] BasicAuthError),
    #[error(transparent)]
    Bearer(#[from] BearerAuthError),
}

/// A credential entry shared by the text and native stores.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "PersistentCredentialWire")]
pub(crate) struct PersistentCredential {
    /// The service URL for this credential.
    pub(crate) service: Service,
    /// The credentials for this entry.
    pub(crate) credentials: Credentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentCredentialWire {
    service: Service,
    username: Username,
    #[serde(default)]
    scheme: AuthScheme,
    password: Option<Password>,
    token: Option<String>,
}

impl Serialize for PersistentCredential {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let wire = match &self.credentials {
            Credentials::Basic { username, password } => PersistentCredentialWire {
                service: self.service.clone(),
                username: username.clone(),
                scheme: AuthScheme::Basic,
                password: password.clone(),
                token: None,
            },
            Credentials::Bearer { token } => PersistentCredentialWire {
                service: self.service.clone(),
                username: Username::none(),
                scheme: AuthScheme::Bearer,
                password: None,
                token: Some(
                    String::from_utf8(token.clone().into_bytes())
                        .map_err(serde::ser::Error::custom)?,
                ),
            },
        };
        wire.serialize(serializer)
    }
}

impl TryFrom<PersistentCredentialWire> for PersistentCredential {
    type Error = PersistentCredentialError;

    fn try_from(value: PersistentCredentialWire) -> Result<Self, Self::Error> {
        let credentials = match value.scheme {
            AuthScheme::Basic => {
                if value.username.as_deref().is_none() {
                    return Err(BasicAuthError::MissingUsername.into());
                }
                if value.token.is_some() {
                    return Err(BasicAuthError::UnexpectedToken.into());
                }
                Credentials::Basic {
                    username: value.username,
                    password: value.password,
                }
            }
            AuthScheme::Bearer => {
                if value.username.is_some() {
                    return Err(BearerAuthError::UnexpectedUsername.into());
                }
                if value.password.is_some() {
                    return Err(BearerAuthError::UnexpectedPassword.into());
                }
                let Some(token) = value.token else {
                    return Err(BearerAuthError::MissingToken.into());
                };
                Credentials::Bearer {
                    token: Token::new(token.into_bytes()),
                }
            }
        };
        Ok(Self {
            service: value.service,
            credentials,
        })
    }
}
