use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::{BTreeMap, VecDeque};
use std::iter;

use rustc_hash::{FxHashMap, FxHashSet};

use uv_configuration::{Constraints, DependencyModifierScope, DependencyModifiers};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{DependencyMetadata, Requirement, RequirementScope, RequirementSource};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet};
use uv_resolver_types::universal_marker::encode_conflict_item;
use uv_resolver_types::{ConflictMarker, UniversalMarker};
use uv_workspace::WorkspaceMember;

use super::{
    Dependency, DependencyContext, DependencySelectionConflict, Lock, LockSatisfactionError,
    Package, PackageIndex, Source, implicit_constraints_marker, marker_is_unreachable,
};

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
/// requires `iniconfig`, uv can list `iniconfig` in `project`'s ordinary `dependencies`. Evaluating
/// that edge with no active extra would wrongly apply `iniconfig<2; extra != 'feature'` and reject
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

/// Detects incompatible or overlapping locked selections using the lockfile and validated
/// resolution inputs.
pub(super) struct VersionChecker<'lock, 'settings> {
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
    /// Restores parent conditions omitted from locked edge markers. Compute this at most once
    /// per validation, for a potential version mismatch on an edge or when multiple locked
    /// entries share a name. The traversal follows a section's dependencies only under marker
    /// conditions that haven't reached that section before.
    reachability: OnceCell<Reachability<'lock>>,
}

impl<'lock, 'settings> VersionChecker<'lock, 'settings> {
    /// Check that resolved versions satisfy their declarations and that selections are unique.
    pub(super) fn check(
        lock: &'lock Lock,
        workspace_members: &'settings BTreeMap<PackageName, WorkspaceMember>,
        dependency_metadata: &'settings DependencyMetadata,
        constraints: Constraints,
        modifiers: &'settings DependencyModifiers,
    ) -> Option<LockSatisfactionError> {
        let checker = Self {
            lock,
            workspace_members,
            dependency_metadata,
            constraints,
            modifiers,
            reachability: OnceCell::new(),
        };
        checker
            .find_mismatch()
            .or_else(|| checker.find_conflicting_selections())
    }

    fn find_mismatch(&self) -> Option<LockSatisfactionError> {
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
            let mut prepared_requires_dist = None;
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
                if let Some(mismatch) = self.check_requirements(
                    index,
                    context,
                    requirements,
                    dependencies,
                    &mut prepared_requires_dist,
                ) {
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
        prepared_requires_dist: &mut Option<Vec<Requirement>>,
    ) -> Option<LockSatisfactionError> {
        // `partition_point` and `take_while` below rely on `Lock::new` sorting dependencies
        // by package name. Otherwise, the search could silently skip a matching edge.
        assert!(
            dependencies.is_sorted_by(|left, right| left.package_id.name <= right.package_id.name)
        );

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
        let preprocess = |requirements: &[Requirement]| {
            Lock::preprocess_requirements(
                &package.id.name,
                package.id.version.as_ref(),
                requirements,
                context,
                self.modifiers,
            )
        };
        let section_requirements;
        let requirements = if inferred {
            let inferred_requirements = dependencies
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
            // Applying overrides preserves their extra markers. When checking an existing
            // locked edge below, enforce an override only where it holds for every possible
            // parent extra.
            //
            // Suppose `demo-parent[all]` includes `demo-parent[socks]`, and the project
            // requests only `demo-parent[other]`, which locks `demo-child==2.0.0`. A scoped
            // override can add `demo-parent[socks]` to the inferred requirements without its
            // original `extra == 'all'` guard. Flattening that self-dependency would turn
            // `demo-child==1; extra == 'socks'` into an unconditional pin and wrongly reject
            // the edge under `other`.
            let scope = context.modifier_scope(&package.id.name, package.id.version.as_ref());
            section_requirements = self
                .modifiers
                .apply(scope, &inferred_requirements)
                .map(Cow::into_owned)
                .collect();
            &section_requirements
        } else if let DependencyContext::Group(_) = context {
            section_requirements = preprocess(requirements);
            &section_requirements
        } else {
            // Production and extras share the same declarations, modifiers, and flattening.
            prepared_requires_dist.get_or_insert_with(|| {
                let mut requirements = preprocess(requirements);
                if !package.optional_dependencies.is_empty() {
                    requirements.retain(|requirement| self.may_violate_requirement(requirement));
                }
                requirements
            })
        };
        for requirement in requirements {
            let first = dependencies
                .partition_point(|dependency| dependency.package_id.name < requirement.name);
            for dependency in dependencies[first..]
                .iter()
                .take_while(|dependency| dependency.package_id.name == requirement.name)
            {
                let target = self.lock.package(dependency.index);
                if let Some(mismatch) = self.check_requirement(target, requirement, || {
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

    /// A requirement satisfied by every locked version of its target can be skipped in every extra.
    fn may_violate_requirement(&self, requirement: &Requirement) -> bool {
        let Some(specifier) = requirement.source.version_specifiers() else {
            return false;
        };
        !specifier.is_empty()
            && self
                .lock
                .packages_for_name(&requirement.name)
                .iter()
                .filter_map(|package| package.id.version.as_ref())
                .any(|version| !specifier.contains(version))
    }

    fn check_constraints(
        &self,
        package: &'lock Package,
        marker: impl Fn(MarkerTree) -> MarkerTree,
    ) -> Option<LockSatisfactionError> {
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
    ) -> Option<LockSatisfactionError> {
        let version = package.id.version.as_ref()?;
        // Only registry requirements carry a version specifier. Direct references select a URL,
        // Git source, or local path and are outside this check.
        let RequirementSource::Registry { specifier, .. } = &requirement.source else {
            return None;
        };
        if specifier.contains(version) {
            return None;
        }

        if !self.is_reachable(with_source_conflict(marker(), &requirement.source)) {
            return None;
        }

        Some(LockSatisfactionError::MismatchedDependencyVersion(
            package.id.name.clone(),
            version.clone(),
            specifier.clone(),
        ))
    }

    /// Compare distinct selections for each name. Every path that installs a package visits its
    /// production section, including paths that request extras on that package.
    fn find_conflicting_selections(&self) -> Option<LockSatisfactionError> {
        // `Lock::new` sorts packages by ID, with the name first. Locks with one entry per name
        // need no reachability analysis for this check.
        for (index, [first, next]) in self.lock.packages.array_windows().enumerate() {
            if first.id.name != next.id.name {
                continue;
            }
            let reachability = self.reachability();
            let Some(&first_marker) =
                reachability.get(&(PackageIndex(index), DependencyContext::Production))
            else {
                continue;
            };
            for (offset, other) in self.lock.packages[index + 1..]
                .iter()
                .take_while(|package| package.id.name == first.id.name)
                .enumerate()
            {
                let other_index = PackageIndex(index + 1 + offset);
                if let Some(&other_marker) =
                    reachability.get(&(other_index, DependencyContext::Production))
                    && self.is_reachable(first_marker.and(other_marker))
                {
                    return Some(LockSatisfactionError::ConflictingDependencySelections(
                        Box::new(DependencySelectionConflict {
                            first: first.id.clone(),
                            second: other.id.clone(),
                        }),
                    ));
                }
            }
        }
        None
    }

    fn is_reachable(&self, mut marker: MarkerTree) -> bool {
        if marker.is_false() {
            return false;
        }
        // A marker can be satisfiable on its own yet require two mutually exclusive
        // extras to be active together. Apply the declared conflicts before testing reachability.
        if !self.lock.conflicts.is_empty() && !marker.is_true() {
            let conflicts = ConflictMarker::from_relevant_conflicts(
                &self.lock.conflicts,
                [UniversalMarker::from_combined(marker)],
            );
            marker = marker.and(UniversalMarker::new(MarkerTree::TRUE, conflicts).combined());
        }
        !marker_is_unreachable(&self.lock.requires_python, marker)
    }

    /// A workspace member is requested in every supported environment unless a global
    /// override or project conflict restricts where the root requirement applies.
    fn workspace_member_marker(&self, name: &PackageName) -> MarkerTree {
        self.environment()
            .and(DependencyContext::Production.conflict_marker(name, &self.lock.conflicts))
            .and(self.workspace_override_marker(name, None))
    }

    /// Without an override, the workspace requests the member and all its extras.
    /// Each global override requests the base package and only the extras it names.
    fn workspace_override_marker(
        &self,
        name: &PackageName,
        extra: Option<&ExtraName>,
    ) -> MarkerTree {
        self.modifiers
            .global_overrides()
            .filter(|requirement| &requirement.name == name)
            .map(|requirement| {
                if extra.is_some_and(|extra| !requirement.extras.contains(extra)) {
                    MarkerTree::FALSE
                } else {
                    DependencyContext::Production.requirement_marker(requirement.marker)
                }
            })
            .reduce(MarkerTree::or)
            .unwrap_or(MarkerTree::TRUE)
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
        self.reachability()
            .get(&(index, context))
            .copied()
            .unwrap_or(MarkerTree::FALSE)
            .and(marker)
    }

    fn reachability(&self) -> &Reachability<'lock> {
        self.reachability
            .get_or_init(|| self.compute_reachability())
    }

    /// Restore the parent context removed from locked edge markers during serialization.
    fn compute_reachability(&self) -> Reachability<'lock> {
        // Record which projects, extras, and groups a path selects when a lockfile marker
        // or declared conflict refers to them. For example, a marker can test
        // `extra == 'extra-7-project-legacy'` for an index scoped to `project[legacy]`,
        // even if `legacy` does not appear in `conflicts`.
        //
        // Include declared conflicts too. Two edges without markers can request mutually
        // exclusive extras. Recording both selections lets the overlap check rule out any
        // environment that would require both. Encode the conflict items in the same format
        // as the extra names collected from markers.
        let mut referenced = FxHashSet::default();
        for package in &self.lock.packages {
            for marker in package.fork_markers.iter().copied().chain(
                package
                    .all_dependencies()
                    .map(|dependency| dependency.complexified_marker),
            ) {
                if marker.has_conflict_marker() {
                    marker.combined().visit_extras(|_, extra| {
                        referenced.insert(extra.clone());
                    });
                }
            }
        }
        for item in self.lock.conflicts.iter().flat_map(ConflictSet::iter) {
            referenced.insert(encode_conflict_item(item));
        }

        let mut queue = VecDeque::new();
        let environment = self.environment();
        for &index in self
            .workspace_members
            .keys()
            .filter_map(|name| self.lock.workspace_members.get(name))
        {
            let package = self.lock.package(index);
            let marker = environment.and(self.workspace_override_marker(&package.id.name, None));
            queue.push_back((index, DependencyContext::Production, marker));
            queue.extend(package.optional_dependencies.keys().map(|extra| {
                let marker =
                    environment.and(self.workspace_override_marker(&package.id.name, Some(extra)));
                (index, DependencyContext::Extra(extra), marker)
            }));
            // A group can be selected independently of the member, e.g., with `--only-group`.
            queue.extend(
                package
                    .dependency_groups
                    .keys()
                    .map(|group| (index, DependencyContext::Group(group), environment)),
            );
        }
        for (requirement, index, marker) in self.root_selections() {
            let package = self.lock.package(index);
            let marker = with_source_conflict(marker, &requirement.source);
            let marker =
                with_requested_extras(marker, &package.id.name, &requirement.extras, &referenced);
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
            let marker = match context {
                DependencyContext::Production => marker,
                DependencyContext::Extra(extra) => with_activation(
                    marker,
                    &ConflictItem::from((package.id.name.clone(), extra.clone())),
                    &referenced,
                ),
                DependencyContext::Group(group) => with_activation(
                    marker,
                    &ConflictItem::from((package.id.name.clone(), group.clone())),
                    &referenced,
                ),
            };
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
                // Requesting extras activates them on the path to the base package as well.
                let marker = with_requested_extras(
                    marker,
                    &dependency.package_id.name,
                    &dependency.extra,
                    &referenced,
                );
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

/// Restrict the path to selections in which all extras requested on `package` are active.
///
/// A dependency on `package[feature]` selects the base package and activates `feature` on the
/// same path. Carry that activation into the base package's production section as well as the
/// extra section. Otherwise, a production edge guarded by `feature` being inactive can appear
/// reachable through this path, or a selection requiring an extra that conflicts with `feature`
/// can appear to overlap it.
fn with_requested_extras<'a>(
    marker: MarkerTree,
    package: &PackageName,
    extras: impl IntoIterator<Item = &'a ExtraName>,
    referenced: &FxHashSet<ExtraName>,
) -> MarkerTree {
    if referenced.is_empty() {
        return marker;
    }
    extras.into_iter().fold(marker, |marker, extra| {
        with_activation(
            marker,
            &ConflictItem::from((package.clone(), extra.clone())),
            referenced,
        )
    })
}

/// Record that this path selects the project, extra, or dependency group represented by `item`.
///
/// `referenced` contains the selections that lockfile markers or declared conflicts can test.
/// Other selections cannot affect reachability or overlap. Without this filter, paths that
/// differ only in which unreferenced extras they request would carry different markers, and
/// the worklist could revisit the same downstream dependencies for equivalent paths.
fn with_activation(
    marker: MarkerTree,
    item: &ConflictItem,
    referenced: &FxHashSet<ExtraName>,
) -> MarkerTree {
    if referenced.is_empty() || !referenced.contains(&encode_conflict_item(item)) {
        return marker;
    }
    marker.and(
        UniversalMarker::new(MarkerTree::TRUE, ConflictMarker::from_conflict_item(item)).combined(),
    )
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
