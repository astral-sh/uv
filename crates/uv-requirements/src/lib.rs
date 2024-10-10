use uv_distribution_types::{Dist, GitSourceDist, SourceDist};
use uv_git::GitUrl;
use uv_pypi_types::{Requirement, RequirementSource};

pub use crate::extras::*;
pub use crate::lookahead::*;
pub use crate::source_tree::*;
pub use crate::sources::*;
pub use crate::specification::*;
pub use crate::unnamed::*;

mod extras;
mod lookahead;
mod source_tree;
mod sources;
mod specification;
mod unnamed;
pub mod upgrade;

/// Convert a [`Requirement`] into a [`Dist`], if it is a direct URL.
pub(crate) fn required_dist(
    requirement: &Requirement,
) -> Result<Option<Dist>, uv_distribution_types::Error> {
    Ok(Some(match &requirement.source {
        RequirementSource::Registry { .. } => return Ok(None),
        RequirementSource::Url {
            subdirectory,
            location,
            ext,
            url,
        } => Dist::from_http_url(
            requirement.name.clone(),
            url.clone(),
            location.clone(),
            subdirectory.clone(),
            *ext,
        )?,
        RequirementSource::Git {
            repository,
            reference,
            precise,
            subdirectory,
            url,
        } => {
            let git_url = if let Some(precise) = precise {
                GitUrl::from_commit(repository.clone(), reference.clone(), *precise)
            } else {
                GitUrl::from_reference(repository.clone(), reference.clone())
            };
            Dist::Source(SourceDist::Git(GitSourceDist {
                name: requirement.name.clone(),
                git: Box::new(git_url),
                subdirectory: subdirectory.clone(),
                url: url.clone(),
            }))
        }
        RequirementSource::Path {
            install_path,
            ext,
            url,
        } => Dist::from_file_url(requirement.name.clone(), url.clone(), install_path, *ext)?,
        RequirementSource::Directory {
            install_path,
            r#virtual,
            url,
            editable,
        } => Dist::from_directory_url(
            requirement.name.clone(),
            url.clone(),
            install_path,
            *editable,
            *r#virtual,
        )?,
    }))
}
