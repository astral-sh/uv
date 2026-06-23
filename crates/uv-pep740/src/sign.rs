//! Signing and generating PEP 740 attestations.
use crate::types::{self, TBSDistribution};

const PYPI_PUBLISH_V1: &str = "https://docs.pypi.org/attestations/publish/v1";

#[derive(Debug, thiserror::Error)]
pub enum SignError {
    /// The ambient environment looks right for signing, but we failed to obtain
    /// an identity token from it.
    #[error("failed to obtain ambient identity token for signing")]
    IdentityToken(#[from] sigstore_oidc::Error),
    /// The ambient environment doesn't look like it supports signing.
    #[error("no ambient identity token found for signing")]
    NoIdentityToken,
    /// The signing step itself failed.
    #[error("failed to sign attestation")]
    /// Signing succeeded, but produced a Sigstore bundle that we couldn't
    /// convert into a valid PEP 740 attestation.
    Signing(#[from] sigstore_sign::Error),
    #[error("could not convert bundle into a valid PEP 740 attestation: {0}")]
    Bundle(&'static str),
}

pub struct Signer {
    context: sigstore_sign::SigningContext,
}

impl Signer {
    pub fn new(staging: bool) -> Self {
        let context = if staging {
            sigstore_sign::SigningContext::staging()
        } else {
            sigstore_sign::SigningContext::production()
        };

        Self { context }
    }

    pub async fn sign(
        &self,
        dist: TBSDistribution,
        id_token: sigstore_oidc::IdentityToken,
    ) -> Result<types::Attestation, SignError> {
        let signer = self.context.signer(id_token);

        let attestation = sigstore_sign::Attestation::new(PYPI_PUBLISH_V1, serde_json::Value::Null)
            .add_subject(dist.filename.to_string(), dist.digest);

        signer
            .sign_attestation(attestation)
            .await?
            .try_into()
            .map_err(SignError::Bundle)
    }
}
