use std::path::Path;

use uv_distribution_types::{Requirement, RequirementSource};

use super::requirements::normalize_requirement;
use super::{Lock, LockError, LockErrorKind, Package, SatisfiesResult, Source, normalize_url};

impl Lock {
    /// Retry resolution if a local source is gone or a direct artifact returns a 404.
    ///
    /// Only a confirmed missing artifact can be retried: authentication, hash validation,
    /// build, and other metadata errors must remain visible.
    pub(super) fn source_metadata_error<'lock>(
        &'lock self,
        package: &'lock Package,
        root: &Path,
        error: LockError,
    ) -> Result<SatisfiesResult<'lock>, LockError> {
        let can_retry = !self.supports_missing_package_metadata()
            && !self.is_workspace_package(package)
            && match &package.id.source {
                Source::Path(path)
                | Source::Directory(path)
                | Source::Editable(path)
                | Source::Virtual(path) => matches!(root.join(path).try_exists(), Ok(false)),
                Source::Direct(url, _) => {
                    if let LockErrorKind::Resolution {
                        err: uv_distribution::Error::Client(client_error),
                        ..
                    } = &*error.kind
                        && let uv_client::ErrorKind::WrappedReqwestError(request_url, request_error) =
                            client_error.kind()
                    {
                        let mut request_url = request_url.clone();
                        request_url.remove_credentials();
                        request_error.is_user_failure() && normalize_url(request_url) == *url
                    } else {
                        false
                    }
                }
                Source::Registry(_) | Source::Git(..) => false,
            };
        if can_retry {
            tracing::debug!("Could not refresh metadata for `{}`: {error}", package.id);
            Ok(SatisfiesResult::MismatchedSourceMetadata(&package.id.name))
        } else {
            Err(error)
        }
    }

    /// Return whether a source constraint is satisfied by every locked package with its name.
    ///
    /// Locks without package metadata cannot distinguish a removed source constraint from a
    /// removed first-party source declaration. Keep those constraints so validation can make
    /// that distinction without inspecting an obsolete local source.
    pub(super) fn can_omit_source_constraint(&self, constraint: &Requirement, root: &Path) -> bool {
        if self.supports_missing_package_metadata() {
            return false;
        }
        if matches!(
            constraint.source,
            RequirementSource::Registry { index: None, .. }
        ) {
            return false;
        }
        let Ok(constraint) = normalize_requirement(constraint.clone(), root, &self.requires_python)
        else {
            return false;
        };
        let packages = self.packages_for_name(&constraint.name);
        !packages.is_empty()
            && packages.iter().all(|package| {
                let source_matches = package
                    .id
                    .source
                    .satisfies_requirement_source(&constraint.source, root)
                    .is_ok_and(|matches| matches);
                let version_matches = match &constraint.source {
                    RequirementSource::Registry { specifier, .. } if !specifier.is_empty() => {
                        package
                            .id
                            .version
                            .as_ref()
                            .is_some_and(|version| specifier.contains(version))
                    }
                    RequirementSource::Registry { .. }
                    | RequirementSource::Url { .. }
                    | RequirementSource::GitDirectory { .. }
                    | RequirementSource::GitPath { .. }
                    | RequirementSource::Path { .. }
                    | RequirementSource::Directory { .. } => true,
                };
                source_matches && version_matches
            })
    }
}
