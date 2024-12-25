use std::borrow::Cow;
use std::collections::BTreeSet;

use either::Either;

use uv_configuration::{Constraints, Overrides};
use uv_normalize::PackageName;
use uv_pypi_types::Requirement;
use uv_types::RequestedRequirements;

use crate::preferences::Preferences;
use crate::{DependencyMode, Exclusions, ResolverEnvironment};

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The direct requirements for the project.
    pub(crate) requirements: Vec<Requirement>,

    /// The constraints for the project.
    pub(crate) constraints: Constraints,

    /// The overrides for the project.
    pub(crate) overrides: Overrides,

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
        constraints: Constraints,
        overrides: Overrides,
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

    /// Return an iterator over all requirements, constraints, and overrides, in priority order,
    /// such that requirements come first, followed by constraints, followed by overrides.
    ///
    /// At time of writing, this is used for:
    /// - Determining which requirements should allow yanked versions.
    /// - Determining which requirements should allow pre-release versions (e.g., `torch>=2.2.0a1`).
    /// - Determining which requirements should allow direct URLs (e.g., `torch @ https://...`).
    pub fn requirements<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        self.requirements_no_overrides(env, mode)
            .chain(self.overrides(env, mode))
    }

    /// Like [`Self::requirements`], but without the overrides.
    pub fn requirements_no_overrides<'a>(
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
                            .apply(lookahead.requirements())
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
                        self.constraints
                            .requirements()
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
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    }),
            ),
        }
    }

    /// Only the overrides from [`Self::requirements`].
    pub fn overrides<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        match mode {
            // Include all direct and transitive requirements, with constraints and overrides applied.
            DependencyMode::Transitive => Either::Left(
                self.overrides
                    .requirements()
                    .filter(move |requirement| {
                        requirement.evaluate_markers(env.marker_environment(), &[])
                    })
                    .map(Cow::Borrowed),
            ),
            // Include direct requirements, with constraints and overrides applied.
            DependencyMode::Direct => Either::Right(
                self.overrides
                    .requirements()
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
    pub fn user_requirements<'a>(
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
                            .apply(lookahead.requirements())
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

    /// Returns an iterator over the direct requirements, with overrides applied.
    ///
    /// At time of writing, this is used for:
    /// - Determining which packages should have development dependencies included in the
    ///   resolution (assuming the user enabled development dependencies).
    pub fn direct_requirements<'a>(
        &'a self,
        env: &'a ResolverEnvironment,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + 'a {
        self.overrides
            .apply(self.requirements.iter())
            .filter(move |requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
    }

    /// Apply the overrides and constraints to a set of requirements.
    ///
    /// Constraints are always applied _on top_ of overrides, such that constraints are applied
    /// even if a requirement is overridden.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a Requirement>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        self.constraints.apply(self.overrides.apply(requirements))
    }

    /// Returns the number of input requirements.
    pub fn num_requirements(&self) -> usize {
        self.requirements.len()
    }
}
