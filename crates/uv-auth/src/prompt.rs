use std::io;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::OnceCell;
use tokio::task::JoinError;

use crate::Credentials;
use crate::credentials::Authentication;

/// A failure to read credentials or run the credential prompt.
#[derive(Debug, Error)]
pub enum CredentialsPromptError {
    #[error("Failed to read username")]
    ReadUsername(#[source] io::Error),
    #[error("Failed to read password")]
    ReadPassword(#[source] io::Error),
    #[error("Credential prompt task failed")]
    Task(#[from] JoinError),
}

/// An optional prompt for credentials after other authentication sources are exhausted.
///
/// Use this for a client dedicated to a single destination. Clones share the response so retries
/// reuse the entered credentials, even if the first request fails.
#[derive(Debug, Clone)]
pub struct CredentialsPrompt {
    callback: fn() -> Result<Option<Credentials>, CredentialsPromptError>,
    credentials: Arc<OnceCell<Option<Arc<Authentication>>>>,
}

impl CredentialsPrompt {
    /// Create a prompt whose callback returns `None` when interactive input is unavailable.
    pub fn new(callback: fn() -> Result<Option<Credentials>, CredentialsPromptError>) -> Self {
        Self {
            callback,
            credentials: Arc::new(OnceCell::new()),
        }
    }

    /// Prompt on a blocking thread, retaining the response for subsequent requests.
    pub(crate) async fn credentials(
        &self,
    ) -> Result<Option<Arc<Authentication>>, CredentialsPromptError> {
        self.credentials
            .get_or_try_init(|| async {
                let credentials = tokio::task::spawn_blocking(self.callback).await??;
                Ok(credentials.map(Authentication::from).map(Arc::new))
            })
            .await
            .cloned()
    }
}
