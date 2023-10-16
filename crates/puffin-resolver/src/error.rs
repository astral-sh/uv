use thiserror::Error;

use pep508_rs::Requirement;

use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::version::PubGrubVersion;

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("Failed to find a version of {0} that satisfies the requirement")]
    NotFound(Requirement),

    #[error(transparent)]
    Client(#[from] puffin_client::PypiClientError),

    #[error(transparent)]
    TrySend(#[from] futures::channel::mpsc::SendError),

    #[error(transparent)]
    PubGrub(#[from] pubgrub::error::PubGrubError<PubGrubPackage, PubGrubVersion>),
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for ResolveError {
    fn from(value: futures::channel::mpsc::TrySendError<T>) -> Self {
        value.into_send_error().into()
    }
}
