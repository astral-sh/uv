use owo_colors::OwoColorize;
use uv_normalize::PackageName;
use uv_redacted::SafeUrl;

use super::PackageId;

/// A hash requirement that a lockfile artifact does not satisfy.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct LockHashError(LockHashErrorKind);

impl LockHashError {
    pub(super) fn source_requirement(
        id: PackageId,
        artifact_type: &'static str,
        expected: bool,
    ) -> Self {
        Self(LockHashErrorKind::SourceRequirement {
            id,
            artifact_type,
            expected,
        })
    }

    pub(super) fn missing_proxy_hash(
        package: PackageName,
        filename: String,
        physical: SafeUrl,
    ) -> Self {
        Self(LockHashErrorKind::MissingProxyHash {
            package,
            filename,
            physical: Box::new(physical),
        })
    }
}

#[derive(Debug, thiserror::Error)]
enum LockHashErrorKind {
    #[error("Since the package `{id}` comes from a {source} dependency, a hash was {expected} but one was not found for {artifact_type}", id = id.cyan(), source = id.source.name(), expected = if *expected { "expected" } else { "not expected" })]
    SourceRequirement {
        id: PackageId,
        artifact_type: &'static str,
        expected: bool,
    },
    #[error(
        "Cannot lock `{filename}` for `{package}` from proxy index `{physical}` because it has no supported hash"
    )]
    MissingProxyHash {
        package: PackageName,
        filename: String,
        physical: Box<SafeUrl>,
    },
}
