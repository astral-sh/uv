use std::sync::Arc;

use tokio::sync::OnceCell;

use crate::Credentials;
use crate::credentials::Authentication;

/// An optional prompt for credentials after other authentication sources are exhausted.
///
/// Use this for a client dedicated to a single destination. Clones share the response so retries
/// reuse the entered credentials, even if the first request fails.
#[derive(Debug, Clone)]
pub struct CredentialsPrompt {
    callback: fn() -> anyhow::Result<Option<Credentials>>,
    credentials: Arc<OnceCell<Option<Arc<Authentication>>>>,
}

impl CredentialsPrompt {
    /// Create a prompt whose callback returns `None` when interactive input is unavailable.
    pub fn new(callback: fn() -> anyhow::Result<Option<Credentials>>) -> Self {
        Self {
            callback,
            credentials: Arc::new(OnceCell::new()),
        }
    }

    /// Prompt on a blocking thread, retaining the response for subsequent requests.
    pub(crate) async fn credentials(&self) -> anyhow::Result<Option<Arc<Authentication>>> {
        self.credentials
            .get_or_try_init(|| async {
                let credentials = tokio::task::spawn_blocking(self.callback).await??;
                Ok(credentials.map(Authentication::from).map(Arc::new))
            })
            .await
            .cloned()
    }
}
