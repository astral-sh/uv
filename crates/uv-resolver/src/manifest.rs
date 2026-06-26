use std::borrow::Cow;
use std::collections::BTreeSet;

use either::Either;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution::FlatRequiresDist;
use uv_distribution_types::Requirement;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;
use uv_types::RequestedRequirements;

use crate::preferences::Preferences;
use crate::{DependencyMode, Exclusions, ResolverEnvironment};

/// Requirements from a pip root that retain the scope of their declaring package.
#[derive(Clone, Debug)]
pub struct ScopedRequirements {
    package: PackageName,
    version: Option<Version>,
    requirements: Box<[Requirement]>,
    kind: ScopedRequirementsKind,
}

/// How to interpret the requirements in a [`ScopedRequirements`] root.
#[derive(Clone, Debug)]
enum ScopedRequirementsKind {
    /// Project metadata can contain recursive self-extras that need to be flattened.
    SourceTree { extras: Box<[ExtraName]> },
    /// Dependency groups are already flattened and can include the project itself.
    DependencyGroup,
}

impl ScopedRequirements {
    /// Create scoped requirements from source-tree project metadata.
    pub fn source_tree(
        package: PackageName,
        version: Option<Version>,
        requirements: Box<[Requirement]>,
        extras: Box<[ExtraName]>,
    ) -> Self {
        Self {
            package,
            version,
            requirements,
            kind: ScopedRequirementsKind::SourceTree { extras },
        }
    }

    /// Create scoped requirements from selected dependency groups.
    pub fn dependency_group(
        package: PackageName,
        version: Option<Version>,
        requirements: Box<[Requirement]>,
    ) -> Self {
        Self {
            package,
            version,
            requirements,
            kind: ScopedRequirementsKind::DependencyGroup,
        }
    }

    /// Apply the package scope and return the effective root requirements.
    pub fn effective(&self, overrides: &Overrides, excludes: &Excludes) -> Vec<Requirement> {
        let requirements = overrides
            .apply_for_scope(
                &self.package,
                self.version.as_ref(),
                self.requirements.iter(),
            )
            .filter(|requirement| {
                !excludes.contains_for_package_scope(
                    &self.package,
                    self.version.as_ref(),
                    &requirement.name,
                )
            })
            .map(Cow::into_owned)
            .collect::<Box<_>>();

        match &self.kind {
            ScopedRequirementsKind::SourceTree { extras } => {
                FlatRequiresDist::from_requirements(requirements, &self.package)
                    .into_iter()
                    .map(|requirement| Requirement {
                        marker: requirement.marker.simplify_extras(extras),
                        ..requirement
                    })
                    .collect()
            }
            ScopedRequirementsKind::DependencyGroup => requirements.into_vec(),
        }
    }
}

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The direct requirements for the project.
    pub(crate) requirements: Vec<Requirement>,

    /// Direct requirements that retain the scope of their declaring package.
    pub(crate) scoped_requirements: Vec<ScopedRequirements>,

    /// The constraints for the project.
    pub(crate) constraints: Constraints,

    /// The overrides for the project.
    pub(crate) overrides: Overrides,

    /// The dependency excludes for the project.
    pub(crate) excludes: Excludes,

    /// The preferences for the project.
    ///
    /// These represent "preferred" versions of a given package. For example, they may be the
    /// versions that are already installed in the environment, or already pinned in an existing
    /// lockfile.
    pub(crate) preferences: Preferences,

    /// The name of the project.
    pub(crate) project: Option<PackageName>,

    /// Members of the project's workspace.
    pub(crate) workspace_members: BTreeSet<PackageName>,

    /// The installed packages to exclude from consideration during resolution.
    ///
    /// These typically represent packages that are being upgraded or reinstalled
    /// and should be pulled from a remote source like a package index.
    pub(crate) exclusions: Exclusions,

    /// The lookahead requirements for the project.
    ///
    /// These represent transitive dependencies that should be incorporated when making
    /// determinations around "allowed" versions (for example, "allowed" URLs or "allowed"
    /// pre-release versions).
    pub(crate) lookaheads: Vec<RequestedRequirements>,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        scoped_requirements: Vec<ScopedRequirements>,
        constraints: Constraints,
        overrides: Overrides,
        excludes: Excludes,
        preferences: Preferences,
        project: Option<PackageName>,
        workspace_members: BTreeSet<PackageName>,
        exclusions: Exclusions,
        lookaheads: Vec<RequestedRequirements>,
    ) -> Self {
        Self {
            requirements,
            scoped_requirements,
            constraints,
            overrides,
            excludes,
            preferences,
            project,
            workspace_members,
            exclusions,
            lookaheads,
        }
    }

    pub fn simple(requirements: Vec<Requirement>) -> Self {
        Self {
            requirements,
            scoped_requirements: Vec::new(),
            constraints: Constraints::default(),
            overrides: Overrides::default(),
            excludes: Excludes::default(),
            preferences: Preferences::default(),
            project: None,
            exclusions: Exclusions::default(),
            workspace_members: BTreeSet::new(),
            lookaheads: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_constraints(mut self, constraints: Constraints) -> Self {
        self.constraints = constraints;
        self
    }

    #[must_use]
    pub fn with_lookaheads(mut self, lookaheads: Vec<RequestedRequirements>) -> Self {
        self.lookaheads = lookaheads;
        self
    }

    /// Return the effective requirements from package-scoped roots.
    pub(crate) fn effective_scoped_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.scoped_requirements
            .iter()
            .flat_map(|requirements| requirements.effective(&self.overrides, &self.excludes))
    }

    /// Return an iterator over all requirements, constraints, and overrides, in priority order,
    /// such that requirements come first, followed by constraints, followed by overrides.
    ///
    /// At time of writing, this is used for:
    /// - Determining which requirements should allow yanked versions.
    /// - Determining which requirements should allow pre-release versions (e.g., `torch>=2.2.0a1`).
    /// - Determining which requirements should allow direct URLs (e.g., `torch @ https://...`).
    pub(crate) fn requirements<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        self.requirements_no_overrides(env, mode)
            .chain(self.overrides(env, mode))
    }

    /// Return all requirements that affect manifest-wide candidate selection policy.
    ///
    /// Scoped overrides are included even when their scope is not selected. Whether a scoped
    /// override applies is only known during resolution, after pre-release and yanked-version
    /// policy has already been initialized.
    pub(crate) fn candidate_selection_requirements<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        self.requirements(env, mode).chain(
            self.overrides
                .scoped_requirements()
                .filter(|(package, version, requirement)| {
                    !self.excludes.contains_for_scope(
                        &self.overrides,
                        package,
                        *version,
                        &requirement.name,
                    )
                })
                .map(|(_, _, requirement)| Cow::Borrowed(requirement))
                .filter(move |requirement| {
                    requirement.evaluate_markers(env.marker_environment(), &[])
                }),
        )
    }

    /// Like [`Self::requirements`], but without the overrides.
    pub(crate) fn requirements_no_overrides<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        match mode {
            // Include all direct and transitive requirements, with constraints and overrides applied.
            DependencyMode::Transitive => Either::Left(
                self.lookaheads
                    .iter()
                    .flat_map(move |lookahead| {
                        self.overrides
                            .apply_for(
                                lookahead.package(),
                                lookahead.version(),
                                lookahead.requirements(),
                            )
                            .filter(|requirement| {
                                !self.excludes.contains_for(
                                    lookahead.package(),
                                    lookahead.version(),
                                    &requirement.name,
                                )
                            })
                            .filter(move |requirement| {
                                requirement
                                    .evaluate_markers(env.marker_environment(), lookahead.extras())
                            })
                    })
                    .chain(
                        self.overrides
                            .apply(&self.requirements)
                            .filter(|requirement| !self.excludes.contains(&requirement.name))
                            .filter(move |requirement| {
                                requirement.evaluate_markers(env.marker_environment(), &[])
                            }),
                    )
                    .chain(
                        self.effective_scoped_requirements()
                            .filter(move |requirement| {
                                requirement.evaluate_markers(env.marker_environment(), &[])
                            })
                            .map(Cow::Owned),
                    )
                    .chain(
                        self.constraints
                            .requirements()
                            .filter(|requirement| !self.excludes.contains(&requirement.name))
                            .filter(move |requirement| {
                                requirement.evaluate_markers(env.marker_environment(), &[])
                            })
                            .map(Cow::Borrowed),
                    ),
            ),
            // Include direct requirements, with constraints and overrides applied.
            DependencyMode::Direct => Either::Right(
                self.overrides
                    .apply(&self.requirements)
                    .chain(self.effective_scoped_requirements().map(Cow::Owned))
                    .chain(self.constraints.requirements().map(Cow::Borrowed))
                    .filter(|requirement| !self.excludes.contains(&requirement.name))
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    }),
            ),
        }
    }

    /// Only the overrides from [`Self::requirements`].
    pub(crate) fn overrides<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        match mode {
            // Include all direct and transitive requirements, with constraints and overrides applied.
            DependencyMode::Transitive => Either::Left(
                self.overrides
                    .global_requirements()
                    .filter(|requirement| !self.excludes.contains(&requirement.name))
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    })
                    .map(Cow::Borrowed),
            ),
            // Include direct requirements, with constraints and overrides applied.
            DependencyMode::Direct => Either::Right(
                self.overrides
                    .global_requirements()
                    .filter(|requirement| !self.excludes.contains(&requirement.name))
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    })
                    .map(Cow::Borrowed),
            ),
        }
    }

    /// Return an iterator over the names of all user-provided requirements.
    ///
    /// This includes:
    /// - Direct requirements
    /// - Dependencies of editable requirements
    /// - Transitive dependencies of local package requirements
    ///
    /// At time of writing, this is used for:
    /// - Determining which packages should use the "lowest-compatible version" of a package, when
    ///   the `lowest-direct` strategy is in use.
    pub(crate) fn user_requirements<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        match mode {
            // Include direct requirements, dependencies of editables, and transitive dependencies
            // of local packages.
            DependencyMode::Transitive => Either::Left(
                self.lookaheads
                    .iter()
                    .filter(|lookahead| lookahead.direct())
                    .flat_map(move |lookahead| {
                        self.overrides
                            .apply_for(
                                lookahead.package(),
                                lookahead.version(),
                                lookahead.requirements(),
                            )
                            .filter(|requirement| {
                                !self.excludes.contains_for(
                                    lookahead.package(),
                                    lookahead.version(),
                                    &requirement.name,
                                )
                            })
                            .filter(move |requirement| {
                                requirement
                                    .evaluate_markers(env.marker_environment(), lookahead.extras())
                            })
                    })
                    .chain(
                        self.overrides
                            .apply(&self.requirements)
                            .filter(move |requirement| {
                                requirement.evaluate_markers(env.marker_environment(), &[])
                            }),
                    )
                    .chain(
                        self.effective_scoped_requirements()
                            .filter(move |requirement| {
                                requirement.evaluate_markers(env.marker_environment(), &[])
                            })
                            .map(Cow::Owned),
                    ),
            ),

            // Restrict to the direct requirements.
            DependencyMode::Direct => Either::Right(
                self.overrides
                    .apply(self.requirements.iter())
                    .chain(self.effective_scoped_requirements().map(Cow::Owned))
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    }),
            ),
        }
    }

    /// Returns the number of input requirements.
    pub fn num_requirements(&self) -> usize {
        self.requirements.len() + self.effective_scoped_requirements().count()
    }
}
