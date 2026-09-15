use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use uv_distribution::Metadata as DistributionMetadata;
use uv_distribution_types::{Requirement, RequirementSource, RequiresPython};
use uv_normalize::{ExtraName, GroupName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::MarkerTree;

use super::{Lock, LockError, LockErrorKind, Package, PackageId, normalize_requirement};

/// Supplementary metadata needed to replay first-party source discovery.
///
/// An empty section still distinguishes complete discovery from a legacy lock. Selected static
/// local packages need no record: their metadata is reread and validated against their lock entry.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct SourceInputs {
    #[serde(rename = "package", default)]
    pub(super) packages: Vec<SourceInput>,
}

impl SourceInputs {
    /// Retain only inputs that cannot be reconstructed from current sources and selected packages.
    pub(super) fn from_inputs(
        inputs: &[crate::SourceInput],
        lock: &Lock,
        root: &Path,
    ) -> Result<Self, LockError> {
        let mut packages = Vec::new();
        for input in inputs {
            let mut requirement = relative_requirement(input.requirement.clone(), root)?;
            requirement.extras = Box::new([]);
            requirement.groups = Box::new([]);
            requirement.marker = MarkerTree::TRUE;

            let source = absolute_requirement(requirement.clone(), root)?.source;
            let mut selected = Vec::new();
            for package in lock.packages_for_name(&requirement.name) {
                if package
                    .id
                    .source
                    .satisfies_requirement_source(&source, root)?
                {
                    selected.push(package);
                }
            }

            let metadata = match &requirement.source {
                RequirementSource::Registry { .. } => continue,
                RequirementSource::Directory { .. } | RequirementSource::Path { .. } => {
                    if selected.is_empty() && !lock.supports_missing_package_metadata() {
                        // Compare declarations for mutable providers absent from the selected graph.
                        // Hash their canonical wire representation so workspace-relative paths stay
                        // relative and equivalent metadata orderings have the same fingerprint.
                        let metadata = SourceMetadata::from_metadata(&input.metadata, root)?;
                        let contents = serde_json::to_vec(&(&input.metadata.version, &metadata))
                            .map_err(LockErrorKind::SourceInputSerialization)?;
                        SourceInputMetadata::Local {
                            fingerprint: Some(blake3::hash(&contents).to_hex().to_string()),
                        }
                    } else if input.metadata.dynamic {
                        // Dynamic source trees omit their version from the selected package entry.
                        SourceInputMetadata::Local { fingerprint: None }
                    } else {
                        continue;
                    }
                }
                RequirementSource::Url { .. }
                | RequirementSource::GitDirectory { .. }
                | RequirementSource::GitPath { .. } => {
                    let metadata = SourceMetadata::from_metadata(&input.metadata, root)?;
                    let mut matching = None;
                    for package in selected.into_iter().filter(|_| {
                        !metadata.requires_dist.is_empty()
                            || !metadata.provides_extra.is_empty()
                            || !metadata.dependency_groups.is_empty()
                    }) {
                        if package.id.version.as_ref() == Some(&input.metadata.version)
                            && SourceMetadata::from_package(
                                package,
                                metadata.requires_python.clone(),
                                metadata.dynamic,
                                root,
                            )? == metadata
                        {
                            // An imprecise Git request can match multiple selected commits. Retain
                            // its metadata directly unless a single package is an exact substitute.
                            if matching.is_some() {
                                matching = None;
                                break;
                            }
                            matching = Some(package);
                        }
                    }
                    if let Some(package) = matching {
                        SourceInputMetadata::Package {
                            package: package.id.clone(),
                            requires_python: metadata.requires_python,
                            dynamic: metadata.dynamic,
                        }
                    } else {
                        SourceInputMetadata::Remote(metadata)
                    }
                }
            };
            packages.push(SourceInput {
                requirement,
                version: input.metadata.version.clone(),
                metadata,
            });
        }
        packages.sort();
        packages.dedup();
        Ok(Self { packages })
    }
}

/// An exact source identity and the information unavailable from its selected package entry.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct SourceInput {
    pub(super) requirement: Requirement,
    pub(super) version: Version,
    #[serde(flatten)]
    pub(super) metadata: SourceInputMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum SourceInputMetadata {
    /// A freshness fingerprint for an unselected local provider, or just a dynamic version.
    Local {
        #[serde(default)]
        fingerprint: Option<String>,
    },
    /// Raw remote declarations that cannot be recovered from a selected package.
    Remote(SourceMetadata),
    /// An exact selected package whose retained metadata reproduces the discovery input.
    Package {
        package: PackageId,
        #[serde(default, rename = "requires-python")]
        requires_python: Option<VersionSpecifiers>,
        #[serde(default)]
        dynamic: bool,
    },
}

impl SourceInput {
    pub(super) fn is_local_fingerprint(&self) -> bool {
        match &self.metadata {
            SourceInputMetadata::Local { fingerprint } => fingerprint.is_some(),
            SourceInputMetadata::Remote(_) | SourceInputMetadata::Package { .. } => false,
        }
    }

    /// Restore remote declarations only after discovery reaches this exact source.
    pub(super) fn metadata(
        &self,
        lock: &Lock,
        root: &Path,
    ) -> Result<Option<DistributionMetadata>, LockError> {
        let metadata = match &self.metadata {
            SourceInputMetadata::Local { .. } => return Ok(None),
            SourceInputMetadata::Remote(metadata) => metadata.clone(),
            SourceInputMetadata::Package {
                package,
                requires_python,
                dynamic,
            } => {
                let Some(index) = lock.by_id.get(package) else {
                    return Ok(None);
                };
                let package = lock.package(*index);
                let requirement = absolute_requirement(self.requirement.clone(), root)?;
                if package.id.name != requirement.name
                    || package.id.version.as_ref() != Some(&self.version)
                    || !package
                        .id
                        .source
                        .satisfies_requirement_source(&requirement.source, root)?
                {
                    return Ok(None);
                }
                SourceMetadata::from_package(package, requires_python.clone(), *dynamic, root)?
            }
        };
        Ok(Some(DistributionMetadata {
            name: self.requirement.name.clone(),
            version: self.version.clone(),
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(|requirement| absolute_requirement(requirement, root))
                .collect::<Result<_, _>>()?,
            requires_python: metadata.requires_python,
            provides_extra: metadata.provides_extra.into_iter().collect(),
            dependency_groups: metadata
                .dependency_groups
                .into_iter()
                .map(|(group, requirements)| {
                    let requirements = requirements
                        .into_iter()
                        .map(|requirement| absolute_requirement(requirement, root))
                        .collect::<Result<_, _>>()?;
                    Ok((group, requirements))
                })
                .collect::<Result<_, LockError>>()?,
            dynamic: metadata.dynamic,
        }))
    }

    /// Match the artifact identity independently of the requested extras, groups, or markers.
    pub(super) fn matches_requirement(
        &self,
        requirement: &Requirement,
        root: &Path,
    ) -> Result<bool, LockError> {
        if self.requirement.name != requirement.name {
            return Ok(false);
        }
        let requires_python = RequiresPython::from_specifiers(VersionSpecifiers::empty());
        let expected = normalize_requirement(requirement.clone(), root, &requires_python)?;
        let actual = normalize_requirement(self.requirement.clone(), root, &requires_python)?;
        Ok(expected.source == actual.source)
    }
}

/// Canonical raw declarations, before selecting extras or simplifying environment markers.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) struct SourceMetadata {
    #[serde(default)]
    pub(super) requires_python: Option<VersionSpecifiers>,
    #[serde(default)]
    pub(super) requires_dist: BTreeSet<Requirement>,
    #[serde(default, rename = "provides-extras")]
    pub(super) provides_extra: BTreeSet<ExtraName>,
    #[serde(default)]
    pub(super) dependency_groups: BTreeMap<GroupName, BTreeSet<Requirement>>,
    #[serde(default)]
    pub(super) dynamic: bool,
}

impl SourceMetadata {
    /// Canonicalize declarations without dropping information outside the lock's Python range.
    fn from_metadata(metadata: &DistributionMetadata, root: &Path) -> Result<Self, LockError> {
        Self {
            requires_python: metadata.requires_python.clone(),
            requires_dist: metadata.requires_dist.iter().cloned().collect(),
            provides_extra: metadata.provides_extra.iter().cloned().collect(),
            dependency_groups: metadata
                .dependency_groups
                .iter()
                .map(|(group, requirements)| {
                    (group.clone(), requirements.iter().cloned().collect())
                })
                .collect(),
            dynamic: metadata.dynamic,
        }
        .normalized(root)
    }

    /// Reconstruct the input fields retained by a selected package and its supplements.
    fn from_package(
        package: &Package,
        requires_python: Option<VersionSpecifiers>,
        dynamic: bool,
        root: &Path,
    ) -> Result<Self, LockError> {
        Self {
            requires_python,
            requires_dist: package.metadata.requires_dist.clone(),
            provides_extra: package.metadata.provides_extra.iter().cloned().collect(),
            dependency_groups: package.metadata.dependency_groups.clone(),
            dynamic,
        }
        .normalized(root)
    }

    /// Give persisted package metadata and discovery metadata the same relative source identities.
    fn normalized(mut self, root: &Path) -> Result<Self, LockError> {
        self.requires_dist = self
            .requires_dist
            .into_iter()
            .map(|requirement| relative_requirement(requirement, root))
            .collect::<Result<_, _>>()?;
        self.dependency_groups = self
            .dependency_groups
            .into_iter()
            .map(|(group, requirements)| {
                let requirements = requirements
                    .into_iter()
                    .map(|requirement| relative_requirement(requirement, root))
                    .collect::<Result<_, _>>()?;
                Ok((group, requirements))
            })
            .collect::<Result<_, LockError>>()?;
        Ok(self)
    }
}

/// Restore local paths while retaining whether the recorded declaration was workspace-relative.
fn absolute_requirement(requirement: Requirement, root: &Path) -> Result<Requirement, LockError> {
    let force_relative = match &requirement.source {
        RequirementSource::Path { install_path, .. }
        | RequirementSource::Directory { install_path, .. } => install_path.is_relative(),
        RequirementSource::Registry { .. }
        | RequirementSource::Url { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. } => false,
    };
    let requires_python = RequiresPython::from_specifiers(VersionSpecifiers::empty());
    let mut requirement = normalize_requirement(requirement, root, &requires_python)?;
    requirement.set_force_relative(force_relative);
    Ok(requirement)
}

/// Preserve input path preferences while removing credentials and normalizing source identities.
fn relative_requirement(requirement: Requirement, root: &Path) -> Result<Requirement, LockError> {
    let force_relative = match &requirement.source {
        RequirementSource::Path { url, .. } | RequirementSource::Directory { url, .. } => {
            url.prefers_relative()
        }
        RequirementSource::Registry { .. }
        | RequirementSource::Url { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. } => false,
    };
    let requires_python = RequiresPython::from_specifiers(VersionSpecifiers::empty());
    let mut requirement = normalize_requirement(requirement, root, &requires_python)?;
    requirement.set_force_relative(force_relative);
    requirement
        .relative_to(root)
        .map_err(LockErrorKind::RequirementRelativePath)
        .map_err(LockError::from)
}
