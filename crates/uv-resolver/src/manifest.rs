use distribution_types::{LocalEditable, UvRequirement, UvRequirements};
use either::Either;
use pep508_rs::MarkerEnvironment;
use pypi_types::Metadata23;
use uv_configuration::{Constraints, Overrides};
use uv_normalize::PackageName;
use uv_types::RequestedRequirements;

use crate::{preferences::Preference, DependencyMode, Exclusions};

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The direct requirements for the project.
    pub(crate) requirements: Vec<UvRequirement>,

    /// The constraints for the project.
    pub(crate) constraints: Constraints,

    /// The overrides for the project.
    pub(crate) overrides: Overrides,

    /// The preferences for the project.
    ///
    /// These represent "preferred" versions of a given package. For example, they may be the
    /// versions that are already installed in the environment, or already pinned in an existing
    /// lockfile.
    pub(crate) preferences: Vec<Preference>,

    /// The name of the project.
    pub(crate) project: Option<PackageName>,

    /// The editable requirements for the project, which are built in advance.
    ///
    /// The requirements of the editables should be included in resolution as if they were
    /// direct requirements in their own right.
    pub(crate) editables: Vec<(LocalEditable, Metadata23, UvRequirements)>,

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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        requirements: Vec<UvRequirement>,
        constraints: Constraints,
        overrides: Overrides,
        preferences: Vec<Preference>,
        project: Option<PackageName>,
        editables: Vec<(LocalEditable, Metadata23, UvRequirements)>,
        exclusions: Exclusions,
        lookaheads: Vec<RequestedRequirements>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            preferences,
            project,
            editables,
            exclusions,
            lookaheads,
        }
    }

    pub fn simple(requirements: Vec<UvRequirement>) -> Self {
        Self {
            requirements,
            constraints: Constraints::default(),
            overrides: Overrides::default(),
            preferences: Vec::new(),
            project: None,
            editables: Vec::new(),
            exclusions: Exclusions::default(),
            lookaheads: Vec::new(),
        }
    }

    /// Return an iterator over all requirements, constraints, and overrides, in priority order,
    /// such that requirements come first, followed by constraints, followed by overrides.
    ///
    /// At time of writing, this is used for:
    /// - Determining which requirements should allow yanked versions.
    /// - Determining which requirements should allow pre-release versions (e.g., `torch>=2.2.0a1`).
    /// - Determining which requirements should allow direct URLs (e.g., `torch @ https://...`).
    /// - Determining which requirements should allow local version specifiers (e.g., `torch==2.2.0+cpu`).
    pub fn requirements<'a>(
        &'a self,
        markers: &'a MarkerEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = &UvRequirement> {
        match mode {
            // Include all direct and transitive requirements, with constraints and overrides applied.
            DependencyMode::Transitive => Either::Left( self
                .lookaheads
                .iter()
                .flat_map(|lookahead| {
                    self.overrides
                        .apply(lookahead.requirements())
                        .filter(|requirement| {
                            requirement.evaluate_markers(markers, lookahead.extras())
                        })
                })
                .chain(self.editables.iter()                    .flat_map(|(editable, _metadata, uv_requirements)| {

                    self.overrides
                        .apply(&uv_requirements.dependencies)
                        .filter(|requirement| {
                            requirement.evaluate_markers(markers, &editable.extras)
                        })
                }))
                .chain(
                    self.overrides
                        .apply(&self.requirements)
                        .filter(|requirement| requirement.evaluate_markers(markers, &[])),
                )
                .chain(
                    self.constraints
                        .requirements()
                        .filter(|requirement| requirement.evaluate_markers(markers, &[])),
                )
                .chain(
                    self.overrides
                        .requirements()
                        .filter(|requirement| requirement.evaluate_markers(markers, &[])),
                ))
            ,

            // Include direct requirements, with constraints and overrides applied.
            DependencyMode::Direct => Either::Right(
                self.overrides.apply(&   self.requirements)
                .chain(self.constraints.requirements())
                .chain(self.overrides.requirements())
                .filter(|requirement| requirement.evaluate_markers(markers, &[]))),
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
        markers: &'a MarkerEnvironment,
        mode: DependencyMode,
    ) -> impl Iterator<Item = &PackageName> {
        match mode {
            // Include direct requirements, dependencies of editables, and transitive dependencies
            // of local packages.
            DependencyMode::Transitive => Either::Left(
                self.lookaheads
                    .iter()
                    .filter(|lookahead| lookahead.direct())
                    .flat_map(|lookahead| {
                        self.overrides
                            .apply(lookahead.requirements())
                            .filter(|requirement| {
                                requirement.evaluate_markers(markers, lookahead.extras())
                            })
                    })
                    .chain(self.editables.iter().flat_map(
                        |(editable, _metadata, uv_requirements)| {
                            self.overrides.apply(&uv_requirements.dependencies).filter(
                                |requirement| {
                                    requirement.evaluate_markers(markers, &editable.extras)
                                },
                            )
                        },
                    ))
                    .chain(
                        self.overrides
                            .apply(&self.requirements)
                            .filter(|requirement| requirement.evaluate_markers(markers, &[])),
                    )
                    .map(|requirement| &requirement.name),
            ),

            // Restrict to the direct requirements.
            DependencyMode::Direct => Either::Right(
                self.overrides
                    .apply(self.requirements.iter())
                    .filter(|requirement| requirement.evaluate_markers(markers, &[]))
                    .map(|requirement| &requirement.name),
            ),
        }
    }

    /// Apply the overrides and constraints to a set of requirements.
    ///
    /// Constraints are always applied _on top_ of overrides, such that constraints are applied
    /// even if a requirement is overridden.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a UvRequirement>,
    ) -> impl Iterator<Item = &UvRequirement> {
        self.constraints.apply(self.overrides.apply(requirements))
    }
}
