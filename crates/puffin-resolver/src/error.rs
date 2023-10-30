use pubgrub::range::Range;
use thiserror::Error;

use pep508_rs::Requirement;

use crate::pubgrub::{PubGrubPackage, PubGrubVersion};

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("Failed to find a version of {0} that satisfies the requirement")]
    NotFound(Requirement),

    #[error("The request stream terminated unexpectedly")]
    StreamTermination,

    #[error(transparent)]
    Client(#[from] puffin_client::Error),

    #[error(transparent)]
    TrySend(#[from] futures::channel::mpsc::SendError),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    PubGrub(#[from] pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>),

    #[error("Failed to build source distribution {filename}")]
    SourceDistribution {
        filename: String,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for ResolveError {
    fn from(value: futures::channel::mpsc::TrySendError<T>) -> Self {
        value.into_send_error().into()
    }
}
