use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::{BTreeMap, VecDeque};
use std::iter;

use rustc_hash::FxHashMap;

use uv_configuration::{Constraints, DependencyModifierScope, DependencyModifiers};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{DependencyMetadata, Requirement, RequirementScope, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_resolver_types::{ConflictMarker, UniversalMarker};
use uv_workspace::WorkspaceMember;

use super::{
    Dependency, DependencyContext, Lock, Package, PackageIndex, SatisfiesResult, Source,
    implicit_constraints_marker, marker_is_unreachable,
};

impl Lock {
    /// Check that resolved versions satisfy the declarations available in the lockfile.
    pub(super) fn check_locked_versions(
        &self,
        workspace_members: &BTreeMap<PackageName, WorkspaceMember>,
        dependency_metadata: &DependencyMetadata,
        constraints: Constraints,
        modifiers: &DependencyModifiers,
    ) -> Option<SatisfiesResult<'_>> {
        VersionChecker {
            lock: self,
            workspace_members,
            dependency_metadata,
            constraints,
            modifiers,
            reachability: OnceCell::new(),
        }
        .check()
    }
}

/// The union of path markers from workspace members and manifest roots to each locked package's
/// production, extra, or group section.
///
/// Path markers carry environment and conflict conditions. A missing entry is equivalent to
/// [`MarkerTree::FALSE`].
type Reachability<'lock> = FxHashMap<(PackageIndex, DependencyContext<'lock>), MarkerTree>;

/// Whether a dependency's location in `uv.lock` tells us which extra was active when the
/// requirement was introduced. Each `[[package]]` entry lists its resolved dependencies in
/// `dependencies`, by extra in `[package.optional-dependencies]`, or by group in
/// `[package.dev-dependencies]`. Group dependencies always evaluate markers with no active extras.
///
/// For example, if `project` declares `project[feature]` as an ordinary dependency and `feature`
/// requires `iniconfig`, uv can list `iniconfig` in `project`'s ordinary `dependencies`. Assuming no
/// extra was active there would wrongly apply `iniconfig<2; extra != 'feature'` and reject
/// a valid lock with `iniconfig==2.0.0`.
#[derive(Clone, Copy, Debug)]
enum ExtraProvenance {
    /// An optional dependency list identifies the active extra. An ordinary or group list
    /// indicates that no extra was active.
    Known,
    /// An ordinary or optional dependency list may include requirements inherited from an extra.
    /// Enforce only marker conditions that hold for every choice of extras.
    Unknown,
}

/// Detects version mismatches that can be established from the lockfile and validated resolution
/// inputs. A mismatch requires an applicable registry specifier and a reachable locked selection.
struct VersionChecker<'lock, 'settings> {
    /// Supplies the exact locked selections and any recorded dependency declarations.
    lock: &'lock Lock,
    /// Workspace members of the target being validated. A script can depend on an editable
    /// package at `.` under a marker, so its path alone cannot establish reachability.
    workspace_members: &'settings BTreeMap<PackageName, WorkspaceMember>,
    /// User-provided declarations checked against the manifest.
    dependency_metadata: &'settings DependencyMetadata,
    /// Normalized constraints that matched the manifest, checked alongside dependency requirements.
    constraints: Constraints,
    /// Overrides and exclusions that determine the effective requirements in each scope.
    modifiers: &'settings DependencyModifiers,
    /// Restores parent conditions omitted from locked edge markers. Computed lazily when an edge's
    /// target violates a specifier and the requirement and edge markers overlap.
    reachability: OnceCell<Reachability<'lock>>,
}

impl<'lock> VersionChecker<'lock, '_> {
    fn check(&self) -> Option<SatisfiesResult<'lock>> {
        for (requirement, index, marker) in self.root_selections() {
            let package = self.lock.package(index);
            if let Some(mismatch) = self
                .check_requirement(package, &requirement, || marker)
                .or_else(|| {
                    // No parent extra introduced this manifest root requirement, so its
                    // constraint markers are evaluated with a known empty set of extras.
                    self.check_constraints(package, |constraint| {
                        marker.and(constraint.resolve_extras([]))
                    })
                })
            {
                return Some(mismatch);
            }
        }

        let has_constraints = self.constraints.requirements().next().is_some();
        if has_constraints {
            // Each workspace member is a root requirement, even if no package depends on it.
            for (name, &index) in &self.lock.workspace_members {
                if self.workspace_members.contains_key(name)
                    && !self.modifiers.is_excluded(name)
                    && let Some(mismatch) =
                        self.check_constraints(self.lock.package(index), |constraint| {
                            self.workspace_member_marker(name)
                                .and(constraint.resolve_extras([]))
                        })
                {
                    return Some(mismatch);
                }
            }
        }

        let has_overrides = !self.lock.manifest.overrides.is_empty();
        for (index, package) in self.lock.packages.iter().enumerate() {
            // Registry and Git packages omit their declarations from the lockfile. For Git,
            // only a wheel filename gives the resolver a version before it reads the metadata.
            let configured_metadata = match &package.id.source {
                Source::Registry(_) => package.id.version.as_ref().and_then(|version| {
                    self.dependency_metadata
                        .get(&package.id.name, Some(version))
                }),
                Source::Git(_, git) => {
                    let wheel = git
                        .path
                        .as_ref()
                        .and_then(|path| path.file_name()?.to_str()?.parse::<WheelFilename>().ok());
                    self.dependency_metadata
                        .get(&package.id.name, wheel.as_ref().map(|wheel| &wheel.version))
                }
                Source::Direct(..)
                | Source::Path(_)
                | Source::Directory(_)
                | Source::Editable(_)
                | Source::Virtual(_) => None,
            };
            let configured_requirements = configured_metadata.map(|metadata| {
                Box::into_iter(metadata.requires_dist)
                    .map(Requirement::from)
                    .collect::<Vec<_>>()
            });
            let requires_dist = configured_requirements.as_deref().or_else(|| {
                (!package.metadata.requires_dist.is_empty())
                    .then_some(package.metadata.requires_dist.as_ref())
            });
            if !has_constraints
                && !has_overrides
                && requires_dist.unwrap_or_default().is_empty()
                && package.metadata.dependency_groups.is_empty()
            {
                continue;
            }
            let index = PackageIndex(index);
            for context in iter::once(DependencyContext::Production)
                .chain(
                    package
                        .optional_dependencies
                        .keys()
                        .map(DependencyContext::Extra),
                )
                .chain(
                    package
                        .dependency_groups
                        .keys()
                        .map(DependencyContext::Group),
                )
            {
                let dependencies = context.dependencies(package);
                if dependencies.is_empty() {
                    continue;
                }
                let requirements: &[Requirement] = match context {
                    DependencyContext::Production | DependencyContext::Extra(_) => {
                        requires_dist.unwrap_or_default()
                    }
                    DependencyContext::Group(group) => package
                        .metadata
                        .dependency_groups
                        .get(group)
                        .map_or(&[], Box::as_ref),
                };
                if let Some(mismatch) =
                    self.check_requirements(index, context, requirements, dependencies)
                {
                    return Some(mismatch);
                }
                if !has_constraints {
                    continue;
                }

                for dependency in dependencies {
                    let target = self.lock.package(dependency.index);
                    if let Some(mismatch) = self.check_constraints(target, |constraint| {
                        let marker = self.constraint_marker(
                            constraint,
                            context,
                            package,
                            requires_dist,
                            &target.id.name,
                        );
                        self.edge_marker(index, context, dependency, marker)
                    }) {
                        return Some(mismatch);
                    }
                }
            }
        }
        None
    }

    fn check_requirements(
        &self,
        index: PackageIndex,
        context: DependencyContext<'lock>,
        requirements: &[Requirement],
        dependencies: &'lock [Dependency],
    ) -> Option<SatisfiesResult<'lock>> {
        let package = self.lock.package(index);
        // If this parent has package-scoped overrides or exclusions and its version is missing,
        // skip its production and optional requirements. For example, we can't tell whether an
        // `idna` override scoped to `anyio==3.7.0` applies to a checkout with a dynamic version.
        // Dependency groups use only global overrides. A package-scoped exclusion removes the
        // dependency's group edge, so there is no locked version to check for it.
        if package.id.version.is_none()
            && self.modifiers.has_scoped_package(&package.id.name)
            && !matches!(context, DependencyContext::Group(_))
        {
            return None;
        }

        // The lock usually omits registry packages' declared requirements. Start with a bare
        // requirement for each locked dependency so a matching override can supply its specifier.
        let inferred = requirements.is_empty() && !self.lock.manifest.overrides.is_empty();
        let inferred_requirements;
        let requirements = if inferred {
            inferred_requirements = dependencies
                .iter()
                .map(|dependency| Requirement {
                    name: dependency.package_id.name.clone(),
                    extras: Box::new([]),
                    groups: Box::new([]),
                    marker: MarkerTree::TRUE,
                    source: RequirementSource::Registry {
                        specifier: VersionSpecifiers::empty(),
                        index: None,
                        conflict: None,
                    },
                    scope: RequirementScope::Global,
                    origin: None,
                })
                .collect::<Vec<_>>();
            &inferred_requirements
        } else {
            requirements
        };
        for requirement in Lock::preprocess_requirements(
            &package.id.name,
            package.id.version.as_ref(),
            requirements,
            context,
            self.modifiers,
        ) {
            let first = dependencies
                .partition_point(|dependency| dependency.package_id.name < requirement.name);
            for dependency in dependencies[first..]
                .iter()
                .take_while(|dependency| dependency.package_id.name == requirement.name)
            {
                let target = self.lock.package(dependency.index);
                if let Some(mismatch) = self.check_requirement(target, &requirement, || {
                    // If `pkg[all]` includes `pkg[docs]` and `docs` requires `sphinx`,
                    // preprocessing the declarations carries the effective `sphinx` requirement
                    // into `all`. We can then evaluate its marker for that section.
                    // Inferring `sphinx` from the locked edge under `all` loses the link to
                    // `docs`. Applying an override guarded by `extra == 'all'` could reject a
                    // valid lock, so only enforce what holds regardless of the parent extra.
                    let marker = if inferred {
                        applicable_marker(requirement.marker, context, ExtraProvenance::Unknown)
                    } else {
                        context.requirement_marker(requirement.marker)
                    };
                    self.edge_marker(index, context, dependency, marker)
                }) {
                    return Some(mismatch);
                }
            }
        }
        None
    }

    fn check_constraints(
        &self,
        package: &'lock Package,
        marker: impl Fn(MarkerTree) -> MarkerTree,
    ) -> Option<SatisfiesResult<'lock>> {
        self.constraints
            .get(&package.id.name)?
            .iter()
            .find_map(|constraint| {
                self.check_requirement(package, constraint, || marker(constraint.marker))
            })
    }

    /// Determine where this constraint applies to the dependency before flattening recursive extras.
    fn constraint_marker(
        &self,
        constraint: MarkerTree,
        context: DependencyContext<'_>,
        parent: &Package,
        requires_dist: Option<&[Requirement]>,
        dependency: &PackageName,
    ) -> MarkerTree {
        let section_marker = applicable_marker(constraint, context, ExtraProvenance::Known);
        let guaranteed = applicable_marker(constraint, context, ExtraProvenance::Unknown);
        if section_marker == guaranteed {
            return section_marker;
        }
        let Some(requires_dist) = requires_dist else {
            return guaranteed;
        };
        let scope = match &parent.id.version {
            Some(version) => DependencyModifierScope::Package(&parent.id.name, version),
            None if self.modifiers.has_scoped_package(&parent.id.name) => return guaranteed,
            None => DependencyModifierScope::Global,
        };
        let mut has_recursive_extra = false;
        let mut direct_marker = MarkerTree::FALSE;
        for requirement in self.modifiers.apply(scope, requires_dist) {
            if requirement.name == parent.id.name && !requirement.extras.is_empty() {
                has_recursive_extra = true;
            }
            if requirement.name == *dependency {
                let marker = context.requirement_marker(requirement.marker);
                direct_marker = direct_marker.or(with_source_conflict(marker, &requirement.source));
            }
        }
        if has_recursive_extra {
            // A recursive extra can add an edge to this section. A direct declaration still
            // identifies the active extra wherever its own marker holds.
            guaranteed.or(section_marker.and(direct_marker))
        } else {
            section_marker
        }
    }

    /// Given a package in the lock file and its corresponding
    /// requirement (also from the lock file), along with a
    /// corresponding marker, check whether there is a mismatch
    /// between the package version and its requirement.
    ///
    /// `None` is returned when there is no mismatch.
    fn check_requirement(
        &self,
        package: &'lock Package,
        requirement: &Requirement,
        marker: impl FnOnce() -> MarkerTree,
    ) -> Option<SatisfiesResult<'lock>> {
        let version = package.id.version.as_ref()?;
        // Only registry requirements carry a version specifier. Direct references select a URL,
        // Git source, or local path and are outside this check.
        let RequirementSource::Registry { specifier, .. } = &requirement.source else {
            return None;
        };
        if specifier.contains(version) {
            return None;
        }

        let mut marker = marker();
        if marker.is_false() {
            return None;
        }

        // Checking conflicts here is necessary so that we can
        // return `None` for cases that are actually unreachable.
        // For example, when a marker has two different conflicts
        // activated.
        marker = with_source_conflict(marker, &requirement.source);
        if !self.lock.conflicts.is_empty() {
            let conflicts = ConflictMarker::from_relevant_conflicts(
                &self.lock.conflicts,
                [UniversalMarker::from_combined(marker)],
            );
            marker = marker.and(UniversalMarker::new(MarkerTree::TRUE, conflicts).combined());
        }
        if marker_is_unreachable(&self.lock.requires_python, marker) {
            return None;
        }

        Some(SatisfiesResult::MismatchedDependencyVersion(
            &package.id.name,
            version,
            specifier.clone(),
        ))
    }

    /// A workspace member is requested in every supported environment unless a global
    /// override or project conflict restricts where the root requirement applies.
    fn workspace_member_marker(&self, name: &PackageName) -> MarkerTree {
        let context = DependencyContext::Production;
        let marker = self
            .modifiers
            .global_overrides()
            .filter(|requirement| &requirement.name == name)
            .map(|requirement| context.requirement_marker(requirement.marker))
            .reduce(MarkerTree::or)
            .unwrap_or(MarkerTree::TRUE);
        self.environment()
            .and(context.conflict_marker(name, &self.lock.conflicts))
            .and(marker)
    }

    fn environment(&self) -> MarkerTree {
        implicit_constraints_marker(
            self.lock.fork_markers_union(),
            &self.lock.supported_environments,
        )
    }

    /// Scripts and projectless workspaces record root declarations without explicit edges.
    /// Select the package in each fork before checking its version.
    fn root_selections(
        &self,
    ) -> impl Iterator<Item = (Cow<'_, Requirement>, PackageIndex, MarkerTree)> {
        let lock = self.lock;
        let environment = self.environment();
        // Flattened group requirements carry their individual Python bounds. The combined
        // group bound includes bounds inherited from other groups for interpreter selection.
        let requirements = lock
            .manifest
            .requirements
            .iter()
            .chain(lock.manifest.dependency_groups.values().flatten());
        self.modifiers
            .apply(DependencyModifierScope::Global, requirements)
            .flat_map(move |requirement| {
                // Script requirements and root dependency groups both use the production
                // marker because neither has an active parent extra. A requirement like
                // `anyio[trio]` activates `trio` on `anyio` once this marker matches.
                let marker = environment
                    .and(DependencyContext::Production.requirement_marker(requirement.marker));
                lock.packages_for_name(&requirement.name)
                    .iter()
                    .filter_map(move |package| {
                        let marker = if package.fork_markers.is_empty() {
                            marker
                        } else {
                            // The name lookup can return versions selected for other forks.
                            // Check this version only where it was selected and the
                            // requirement applies.
                            marker.and(
                                package
                                    .fork_markers
                                    .iter()
                                    .fold(MarkerTree::FALSE, |forks, fork| {
                                        forks.or(fork.combined())
                                    }),
                            )
                        };
                        if marker.is_false() {
                            return None;
                        }
                        Some((requirement.clone(), lock.by_id[&package.id], marker))
                    })
            })
    }

    /// Return the conditions under which a requirement or constraint's `marker` applies to
    /// the locked edge `dependency`. The parent package at `index` in `self.lock` records
    /// this edge in its production, extra, or group section identified by `context`.
    /// Intersect `marker` with the edge's marker and the conditions under which that section
    /// is reachable from a root. An unreachable section yields [`MarkerTree::FALSE`].
    fn edge_marker(
        &self,
        index: PackageIndex,
        context: DependencyContext<'lock>,
        dependency: &Dependency,
        marker: MarkerTree,
    ) -> MarkerTree {
        let marker = marker.and(dependency.complexified_marker.combined());
        if marker.is_false() {
            return marker;
        }
        let reachability = self
            .reachability
            .get_or_init(|| self.compute_reachability());
        reachability
            .get(&(index, context))
            .copied()
            .unwrap_or(MarkerTree::FALSE)
            .and(marker)
    }

    /// Restore the parent context removed from locked edge markers during serialization.
    fn compute_reachability(&self) -> Reachability<'lock> {
        let mut queue = VecDeque::new();
        let environment = self.environment();
        for &index in self
            .workspace_members
            .keys()
            .filter_map(|name| self.lock.workspace_members.get(name))
        {
            let package = self.lock.package(index);
            queue.push_back((index, DependencyContext::Production, environment));
            queue.extend(
                package
                    .optional_dependencies
                    .keys()
                    .map(|extra| (index, DependencyContext::Extra(extra), environment)),
            );
            queue.extend(
                package
                    .dependency_groups
                    .keys()
                    .map(|group| (index, DependencyContext::Group(group), environment)),
            );
        }
        for (requirement, index, marker) in self.root_selections() {
            let package = self.lock.package(index);
            queue.push_back((index, DependencyContext::Production, marker));
            queue.extend(requirement.extras.iter().filter_map(|extra| {
                let (extra, _) = package.optional_dependencies.get_key_value(extra)?;
                Some((index, DependencyContext::Extra(extra), marker))
            }));
        }

        let mut reachable = FxHashMap::default();
        while let Some((index, context, marker)) = queue.pop_front() {
            let package = self.lock.package(index);
            let marker =
                marker.and(context.conflict_marker(&package.id.name, &self.lock.conflicts));
            let previous = reachable
                .entry((index, context))
                .or_insert(MarkerTree::FALSE);
            // Propagate only the environments this path adds. If a previous path reached this
            // section on Linux and this path reaches it everywhere, only non-Linux remains.
            let new = marker.and(previous.negate());
            // This is our termination condition in the case of a recursive
            // dependency. That is, if `new` is impossible, then it implies
            // we've already visited `package` under precisely the same
            // conditions. Therefore, anything below won't add anything
            // new.
            if new.is_false() {
                continue;
            }
            *previous = previous.or(marker);
            for dependency in context.dependencies(package) {
                let marker = new.and(dependency.complexified_marker.combined());
                if marker.is_false() {
                    continue;
                }
                // An edge identifies the exact package, even if the lock's fork markers
                // say it belongs to a different fork.
                queue.push_back((dependency.index, DependencyContext::Production, marker));
                queue.extend(
                    dependency
                        .extra
                        .iter()
                        .map(|extra| (dependency.index, DependencyContext::Extra(extra), marker)),
                );
            }
        }
        reachable
    }
}

/// Restrict an index-scoped declaration to the extra or group that activates its source.
fn with_source_conflict(marker: MarkerTree, source: &RequirementSource) -> MarkerTree {
    if let RequirementSource::Registry {
        conflict: Some(conflict),
        ..
    } = source
    {
        marker.and(
            UniversalMarker::new(
                MarkerTree::TRUE,
                ConflictMarker::from_conflict_item(conflict),
            )
            .combined(),
        )
    } else {
        marker
    }
}

/// Resolve the `extra` conditions in `marker` for a dependency stored in `context`.
/// When `extra_provenance` is [`ExtraProvenance::Known`], an extra section activates its
/// named extra and a production section activates none. With [`ExtraProvenance::Unknown`],
/// keep only the environments where `marker` holds for every possible choice of extras.
/// Dependency groups always evaluate `marker` with no active extras.
fn applicable_marker(
    marker: MarkerTree,
    context: DependencyContext<'_>,
    extra_provenance: ExtraProvenance,
) -> MarkerTree {
    match (context, extra_provenance) {
        (DependencyContext::Extra(extra), ExtraProvenance::Known) => {
            marker.resolve_extras([(extra.clone(), MarkerTree::TRUE)])
        }
        (DependencyContext::Group(_), _)
        | (DependencyContext::Production, ExtraProvenance::Known) => marker.resolve_extras([]),
        (DependencyContext::Production | DependencyContext::Extra(_), ExtraProvenance::Unknown) => {
            // Keep the part of the marker that's guaranteed regardless of extras.
            // Applying `without_extras()` to the negated marker finds environments where some
            // choice of extras makes the original marker false. The final negation excludes them.
            marker.negate().without_extras().negate()
        }
    }
}
