use std::borrow::Cow;
use std::collections::BTreeSet;

use either::Either;

use uv_configuration::{Constraints, Excludes, Overrides};
use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_types::RequestedRequirements;

use crate::preferences::Preferences;
use crate::{DependencyMode, Exclusions, ResolverEnvironment};

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The direct requirements for the project.
    pub(super) requirements: Vec<Requirement>,

    /// The constraints for the project.
    pub(super) constraints: Constraints,

    /// The overrides for the project.
    pub(super) overrides: Overrides,

    /// The dependency excludes for the project.
    pub(super) excludes: Excludes,

    /// The preferences for the project.
    ///
    /// These represent "preferred" versions of a given package. For example, they may be the
    /// versions that are already installed in the environment, or already pinned in an existing
    /// lockfile.
    pub(super) preferences: Preferences,

    /// The name of the project.
    pub(super) project: Option<PackageName>,

    /// Members of the project's workspace.
    pub(super) workspace_members: BTreeSet<PackageName>,

    /// The installed packages to exclude from consideration during resolution.
    ///
    /// These typically represent packages that are being upgraded or reinstalled
    /// and should be pulled from a remote source like a package index.
    pub(super) exclusions: Exclusions,

    /// The lookahead requirements for the project.
    ///
    /// These represent transitive dependencies that should be incorporated when making
    /// determinations around "allowed" versions (for example, "allowed" URLs or "allowed"
    /// pre-release versions).
    pub(super) lookaheads: Vec<RequestedRequirements>,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
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

    #[must_use]
    pub fn with_preferences(mut self, preferences: Preferences) -> Self {
        self.preferences = preferences;
        self
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
                    ),
            ),

            // Restrict to the direct requirements.
            DependencyMode::Direct => {
                Either::Right(self.overrides.apply(self.requirements.iter()).filter(
                    move |requirement| requirement.evaluate_markers(env.marker_environment(), &[]),
                ))
            }
        }
    }

    /// Returns the number of input requirements.
    pub fn num_requirements(&self) -> usize {
        self.requirements.len()
    }
}
