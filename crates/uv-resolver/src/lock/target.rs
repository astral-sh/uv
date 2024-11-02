use std::collections::{BTreeMap, VecDeque};

use either::Either;
use rustc_hash::FxHashSet;

use uv_configuration::{BuildOptions, DevGroupsManifest, ExtrasSpecification, InstallOptions};
use uv_distribution_types::{Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName, DEV_DEPENDENCIES};
use uv_platform_tags::Tags;
use uv_pypi_types::{ResolverMarkerEnvironment, VerbatimParsedUrl};
use uv_workspace::dependency_groups::{DependencyGroupError, FlatDependencyGroups};
use uv_workspace::Workspace;

use crate::lock::{LockErrorKind, Package, TagPolicy};
use crate::{Lock, LockError};

/// A target that can be installed from a lockfile.
#[derive(Debug, Copy, Clone)]
pub enum InstallTarget<'env> {
    /// A project (which could be a workspace root or member).
    Project {
        workspace: &'env Workspace,
        name: &'env PackageName,
        lock: &'env Lock,
    },
    /// An entire workspace.
    Workspace {
        workspace: &'env Workspace,
        lock: &'env Lock,
    },
    /// An entire workspace with a (legacy) non-project root.
    NonProjectWorkspace {
        workspace: &'env Workspace,
        lock: &'env Lock,
    },
}

impl<'env> InstallTarget<'env> {
    /// Return the [`Workspace`] of the target.
    pub fn workspace(&self) -> &'env Workspace {
        match self {
            Self::Project { workspace, .. } => workspace,
            Self::Workspace { workspace, .. } => workspace,
            Self::NonProjectWorkspace { workspace, .. } => workspace,
        }
    }

    /// Return the [`Lock`] of the target.
    pub fn lock(&self) -> &'env Lock {
        match self {
            Self::Project { lock, .. } => lock,
            Self::Workspace { lock, .. } => lock,
            Self::NonProjectWorkspace { lock, .. } => lock,
        }
    }

    /// Return the [`PackageName`] of the target.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project { name, .. } => Either::Right(Either::Left(std::iter::once(*name))),
            Self::NonProjectWorkspace { lock, .. } => Either::Left(lock.members().iter()),
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Either::Right(Either::Right(
                        lock.root().into_iter().map(|package| &package.id.name),
                    ))
                } else {
                    Either::Left(lock.members().iter())
                }
            }
        }
    }

    /// Return the [`InstallTarget`] dependency groups.
    ///
    /// Returns dependencies that apply to the workspace root, but not any of its members. As such,
    /// only returns a non-empty iterator for virtual workspaces, which can include dev dependencies
    /// on the virtual root.
    pub fn groups(
        &self,
    ) -> Result<
        BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
        DependencyGroupError,
    > {
        match self {
            Self::Project { .. } => Ok(BTreeMap::default()),
            Self::Workspace { .. } => Ok(BTreeMap::default()),
            Self::NonProjectWorkspace { workspace, .. } => {
                // For non-projects, we might have `dependency-groups` or `tool.uv.dev-dependencies`
                // that are attached to the workspace root (which isn't a member).

                // First, collect `tool.uv.dev_dependencies`
                let dev_dependencies = workspace
                    .pyproject_toml()
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.dev_dependencies.as_ref());

                // Then, collect `dependency-groups`
                let dependency_groups = workspace
                    .pyproject_toml()
                    .dependency_groups
                    .iter()
                    .flatten()
                    .collect::<BTreeMap<_, _>>();

                // Merge any overlapping groups.
                let mut map = BTreeMap::new();
                for (name, dependencies) in
                    FlatDependencyGroups::from_dependency_groups(&dependency_groups)?
                        .into_iter()
                        .chain(
                            // Only add the `dev` group if `dev-dependencies` is defined.
                            dev_dependencies.into_iter().map(|requirements| {
                                (DEV_DEPENDENCIES.clone(), requirements.clone())
                            }),
                        )
                {
                    match map.entry(name) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependencies);
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().extend(dependencies);
                        }
                    }
                }

                Ok(map)
            }
        }
    }

    /// Return the [`PackageName`] of the target, if available.
    pub fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project { name, .. } => Some(name),
            Self::Workspace { .. } => None,
            Self::NonProjectWorkspace { .. } => None,
        }
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecification,
        dev: &DevGroupsManifest,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();

        // Add the workspace packages to the queue.
        for root_name in self.packages() {
            let root = self
                .lock()
                .find_by_name(root_name)
                .map_err(|_| LockErrorKind::MultipleRootPackages {
                    name: root_name.clone(),
                })?
                .ok_or_else(|| LockErrorKind::MissingRootPackage {
                    name: root_name.clone(),
                })?;

            if dev.prod() {
                // Add the base package.
                queue.push_back((root, None));

                // Add any extras.
                match extras {
                    ExtrasSpecification::None => {}
                    ExtrasSpecification::All => {
                        for extra in root.optional_dependencies.keys() {
                            queue.push_back((root, Some(extra)));
                        }
                    }
                    ExtrasSpecification::Some(extras) => {
                        for extra in extras {
                            queue.push_back((root, Some(extra)));
                        }
                    }
                }
            }

            // Add any dev dependencies.
            for group in dev.iter() {
                for dep in root.dependency_groups.get(group).into_iter().flatten() {
                    if dep.complexified_marker.evaluate(marker_env, &[]) {
                        let dep_dist = self.lock().find_by_id(&dep.package_id);
                        if seen.insert((&dep.package_id, None)) {
                            queue.push_back((dep_dist, None));
                        }
                        for extra in &dep.extra {
                            if seen.insert((&dep.package_id, Some(extra))) {
                                queue.push_back((dep_dist, Some(extra)));
                            }
                        }
                    }
                }
            }
        }

        // Add any dependency groups that are exclusive to the workspace root (e.g., dev
        // dependencies in (legacy) non-project workspace roots).
        let groups = self
            .groups()
            .map_err(|err| LockErrorKind::DependencyGroup { err })?;
        for group in dev.iter() {
            for dependency in groups.get(group).into_iter().flatten() {
                if dependency.marker.evaluate(marker_env, &[]) {
                    let root_name = &dependency.name;
                    let root = self
                        .lock()
                        .find_by_markers(root_name, marker_env)
                        .map_err(|_| LockErrorKind::MultipleRootPackages {
                            name: root_name.clone(),
                        })?
                        .ok_or_else(|| LockErrorKind::MissingRootPackage {
                            name: root_name.clone(),
                        })?;

                    // Add the base package.
                    queue.push_back((root, None));

                    // Add any extras.
                    for extra in &dependency.extras {
                        queue.push_back((root, Some(extra)));
                    }
                }
            }
        }

        let mut map = BTreeMap::default();
        let mut hashes = BTreeMap::default();
        while let Some((dist, extra)) = queue.pop_front() {
            let deps = if let Some(extra) = extra {
                Either::Left(dist.optional_dependencies.get(extra).into_iter().flatten())
            } else {
                Either::Right(dist.dependencies.iter())
            };
            for dep in deps {
                if dep.complexified_marker.evaluate(marker_env, &[]) {
                    let dep_dist = self.lock().find_by_id(&dep.package_id);
                    if seen.insert((&dep.package_id, None)) {
                        queue.push_back((dep_dist, None));
                    }
                    for extra in &dep.extra {
                        if seen.insert((&dep.package_id, Some(extra))) {
                            queue.push_back((dep_dist, Some(extra)));
                        }
                    }
                }
            }
            if install_options.include_package(
                &dist.id.name,
                self.project_name(),
                self.lock().members(),
            ) {
                map.insert(
                    dist.id.name.clone(),
                    ResolvedDist::Installable(dist.to_dist(
                        self.workspace().install_path(),
                        TagPolicy::Required(tags),
                        build_options,
                    )?),
                );
                hashes.insert(dist.id.name.clone(), dist.hashes());
            }
        }
        let diagnostics = vec![];
        Ok(Resolution::new(map, hashes, diagnostics))
    }
}
