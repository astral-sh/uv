//! Select the oldest lockfile schema that represents all persisted URL declarations and artifacts.

use uv_configuration::Override;
use uv_distribution_filename::DistExtension;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_pep508::VersionOrUrl;
use uv_pypi_types::{ArchiveUrlFilename, ParsedUrl};

use super::{Lock, MAX_VERSION, Package, REVISION, Source, VERSION};

impl Lock {
    /// Select the minimum schema version after applying metadata and manifest filtering.
    ///
    /// Version 1 readers require archive extensions in direct URLs, including declarations for
    /// dependencies absent from the selected resolution. Version 2 records discovered filenames.
    #[must_use]
    pub fn with_minimum_schema_version(mut self) -> Self {
        let opaque = self.packages.iter().any(Package::has_opaque_urls)
            || self
                .manifest
                .requirements
                .iter()
                .chain(&self.manifest.constraints)
                .chain(self.manifest.dependency_groups.values().flatten())
                .chain(
                    self.manifest
                        .build_constraints
                        .iter()
                        .map(|entry| &entry.requirement),
                )
                .any(has_opaque_requirement)
            || self.manifest.overrides.iter().any(|entry| match entry {
                Override::Requirement(requirement) => has_opaque_requirement(requirement),
                Override::Package(package) => {
                    package.dependencies.iter().any(has_opaque_requirement)
                }
            })
            || self.manifest.dependency_metadata.iter().any(|metadata| {
                metadata.requires_dist.iter().any(|requirement| {
                    if let Some(VersionOrUrl::Url(url)) = &requirement.version_or_url
                        && let ParsedUrl::Archive(archive) = &url.parsed_url
                    {
                        DistExtension::from_path(archive.url.path()).is_err()
                    } else {
                        false
                    }
                })
            });
        (self.version, self.revision) = if opaque {
            (MAX_VERSION, 0)
        } else {
            (VERSION, REVISION)
        };
        self
    }
}

impl Package {
    pub(super) fn has_opaque_urls(&self) -> bool {
        if let Source::Direct(url, _) = &self.id.source
            && DistExtension::from_path(url.base_str()).is_err()
        {
            return true;
        }
        self.wheels
            .iter()
            .any(|wheel| wheel.header_filename.is_some())
            || self
                .sdist
                .as_ref()
                .is_some_and(|sdist| sdist.header_filename().is_some())
            || self
                .metadata
                .requires_dist
                .iter()
                .chain(self.metadata.dependency_groups.values().flatten())
                .any(has_opaque_requirement)
    }
}

fn has_opaque_requirement(requirement: &Requirement) -> bool {
    match &requirement.source {
        RequirementSource::Url { location, .. } => match ArchiveUrlFilename::from_url(location) {
            Ok(ArchiveUrlFilename::Unknown | ArchiveUrlFilename::Header { .. }) => true,
            Ok(ArchiveUrlFilename::Url(_)) | Err(_) => false,
        },
        RequirementSource::Registry { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. }
        | RequirementSource::Path { .. }
        | RequirementSource::Directory { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::Path;

    use uv_distribution_types::{FirstParty, RequiresPython, SourceDist as DistributionSourceDist};
    use uv_pep508::VerbatimUrl;
    use uv_redacted::SafeUrl;
    use uv_redacted::policy::Raw;

    use super::super::normalize_requirement;
    use super::*;

    #[test]
    fn opaque_wheel_roundtrip() -> Result<(), Box<dyn Error>> {
        let input = r#"version = 2
requires-python = ">=3.12"

[[package]]
name = "example"
version = "1.0.0"
source = { url = "https://example.org/download?artifact=wheel" }
wheels = [
    { url = "https://example.org/download?artifact=wheel", filename = "Example-1.0.0-py3-none-any.whl", hash = "sha256:53a42340ae36747fb1471f9b4b7958be1f6e2e5fc234f931aafa3e454fd31dfb" },
]
"#;
        let lock = Lock::from_toml(input)?;
        assert_eq!(Lock::from_canonical_toml(input)?, lock);
        assert_eq!(toml::from_str::<Lock>(input)?, lock);
        assert_eq!(lock.to_toml()?, input);
        assert_eq!(lock.clone().with_minimum_schema_version(), lock);
        let package = &lock.packages[0];
        assert!(package.is_wheel());
        assert!(
            package
                .to_source_dist(Path::new("/"), FirstParty::No)?
                .is_none()
        );
        assert_eq!(
            package.wheels[0].header_filename.as_deref(),
            Some("Example-1.0.0-py3-none-any.whl")
        );
        Ok(())
    }

    #[test]
    fn opaque_source_roundtrip() -> Result<(), Box<dyn Error>> {
        let input = r#"version = 2
requires-python = ">=3.12"

[[package]]
name = "example"
version = "1.0.0"
source = { url = "https://example.org/download?artifact=source", subdirectory = "python" }
sdist = { filename = "example-1.0.0.tar.gz", hash = "sha256:53a42340ae36747fb1471f9b4b7958be1f6e2e5fc234f931aafa3e454fd31dfb" }
"#;
        let lock = Lock::from_toml(input)?;
        assert_eq!(Lock::from_canonical_toml(input)?, lock);
        assert_eq!(toml::from_str::<Lock>(input)?, lock);
        assert_eq!(lock.to_toml()?, input);
        assert_eq!(lock.clone().with_minimum_schema_version(), lock);
        let package = &lock.packages[0];
        assert!(!package.is_wheel());
        let Some(DistributionSourceDist::DirectUrl(distribution)) =
            package.to_source_dist(Path::new("/"), FirstParty::No)?
        else {
            return Err("expected a direct URL source distribution".into());
        };
        assert_eq!(
            distribution.header_filename.as_deref(),
            Some("example-1.0.0.tar.gz")
        );
        assert_eq!(
            distribution.location.as_str(),
            "https://example.org/download?artifact=source"
        );
        assert_eq!(
            distribution.subdirectory.as_deref(),
            Some(Path::new("python"))
        );
        Ok(())
    }

    #[test]
    fn ordinary_wheel_retains_version_one() -> Result<(), Box<dyn Error>> {
        let input = r#"version = 1
revision = 5
requires-python = ">=3.12"

[[package]]
name = "example"
version = "1.0.0"
source = { url = "https://example.org/example-1.0.0-py3-none-any.whl" }
wheels = [
    { url = "https://example.org/example-1.0.0-py3-none-any.whl", hash = "sha256:53a42340ae36747fb1471f9b4b7958be1f6e2e5fc234f931aafa3e454fd31dfb" },
]
"#;
        let lock = Lock::from_toml(input)?.with_minimum_schema_version();
        assert_eq!(lock.to_toml()?, input);
        assert_eq!(Lock::from_canonical_toml(input)?, lock);
        Ok(())
    }

    #[test]
    fn opaque_declaration_selects_version_two() -> Result<(), Box<dyn Error>> {
        let mut lock = Lock::from_toml("version = 1\nrevision = 5\nrequires-python = '>=3.12'\n")?;
        let mut requirement: Requirement = toml::from_str(
            r#"
name = "unused"
url = "https://example.org/unused-1.0.0-py3-none-any.whl"
"#,
        )?;
        let url = SafeUrl::parse("https://example.org/download")?;
        requirement.source = RequirementSource::Url {
            location: url.clone().with_policy(Raw),
            subdirectory: None,
            archive: ArchiveUrlFilename::Unknown,
            url: VerbatimUrl::from_url(url),
        };
        lock.manifest.constraints.insert(requirement);
        let mut lock = lock.with_minimum_schema_version();
        assert_eq!((lock.version, lock.revision), (2, 0));
        assert!(lock.configured_member_default_groups().is_some());
        assert!(lock.member_group_metadata().is_some());
        assert!(lock.workspace_default_groups().is_some());

        lock.manifest.constraints.clear();
        let lock = lock.with_minimum_schema_version();
        assert_eq!((lock.version, lock.revision), (1, 5));
        Ok(())
    }

    #[test]
    fn discovered_filename_does_not_change_declaration_identity() -> Result<(), Box<dyn Error>> {
        let mut requirement: Requirement = toml::from_str(
            r#"
name = "example"
url = "https://example.org/example-1.0.0-py3-none-any.whl"
"#,
        )?;
        let url = SafeUrl::parse("https://example.org/download?artifact=wheel")?;
        requirement.source = RequirementSource::Url {
            location: url.clone().with_policy(Raw),
            subdirectory: None,
            archive: ArchiveUrlFilename::Unknown,
            url: VerbatimUrl::from_url(url),
        };
        let requires_python = RequiresPython::from_specifiers(">=3.12".parse()?);
        let expected =
            normalize_requirement(requirement.clone(), Path::new("/"), &requires_python)?;
        if let RequirementSource::Url { archive, .. } = &mut requirement.source {
            *archive = ArchiveUrlFilename::from_header("example-1.0.0-py3-none-any.whl")?;
        }
        assert_eq!(
            normalize_requirement(requirement, Path::new("/"), &requires_python)?,
            expected
        );
        Ok(())
    }
}
