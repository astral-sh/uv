use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use indexmap::IndexSet;
use itertools::Itertools;
use owo_colors::OwoColorize;
use pubgrub::{DerivationTree, Derived, External, Map, Range, ReportFormatter, Term};
use rustc_hash::FxHashMap;

use uv_configuration::{IndexStrategy, NoBinary, NoBuild};
use uv_distribution_types::{
    IncompatibleDist, IncompatibleSource, IncompatibleWheel, Index, IndexCapabilities,
    IndexLocations, IndexUrl,
};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::{AbiTag, IncompatibleTag, LanguageTag, PlatformTag, Tags};

use crate::candidate_selector::CandidateSelector;
use crate::error::ErrorTree;
use crate::fork_indexes::ForkIndexes;
use crate::fork_urls::ForkUrls;
use crate::prerelease::AllowPrerelease;
use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner, PubGrubPython};
use crate::python_requirement::{PythonRequirement, PythonRequirementSource};
use crate::resolver::{
    MetadataUnavailable, UnavailablePackage, UnavailableReason, UnavailableVersion,
};
use crate::{
    Flexibility, InMemoryIndex, Options, RequiresPython, ResolverEnvironment, VersionsResponse,
};

#[derive(Debug)]
pub(crate) struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package.
    pub(crate) available_versions: &'a FxHashMap<PackageName, BTreeSet<Version>>,

    /// The versions that were available for each package.
    pub(crate) python_requirement: &'a PythonRequirement,

    /// The members of the workspace.
    pub(crate) workspace_members: &'a BTreeSet<PackageName>,

    /// The compatible tags for the resolution.
    pub(crate) tags: Option<&'a Tags>,
}

impl ReportFormatter<PubGrubPackage, Range<Version>, UnavailableReason>
    for PubGrubReportFormatter<'_>
{
    type Output = String;

    fn format_external(
        &self,
        external: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
    ) -> Self::Output {
        match external {
            External::NotRoot(package, version) => {
                format!("we are solving dependencies of {package} {version}")
            }
            External::NoVersions(package, set) => {
                if matches!(
                    &**package,
                    PubGrubPackageInner::Python(PubGrubPython::Target)
                ) {
                    let target = self.python_requirement.target();
                    return format!(
                        "the requested {package} version ({target}) does not satisfy {}",
                        self.compatible_range(package, set)
                    );
                }
                if matches!(
                    &**package,
                    PubGrubPackageInner::Python(PubGrubPython::Installed)
                ) {
                    let installed = self.python_requirement.exact();
                    return format!(
                        "the current {package} version ({installed}) does not satisfy {}",
                        self.compatible_range(package, set)
                    );
                }

                if set == &Range::full() {
                    format!("there are no versions of {package}")
                } else if set.as_singleton().is_some() {
                    format!("there is no version of {package}{set}")
                } else {
                    let complement = set.complement();
                    let range =
                        // Note that sometimes we do not have a range of available versions, e.g.,
                        // when a package is from a non-registry source. In that case, we cannot
                        // perform further simplification of the range.
                        if let Some(available_versions) = package.name().and_then(|name| self.available_versions.get(name)) {
                            update_availability_range(&complement, available_versions)
                        } else {
                            complement
                        };
                    if range.is_empty() {
                        return format!("there are no versions of {package}");
                    }
                    if range.iter().count() == 1 {
                        format!(
                            "only {} is available",
                            self.availability_range(package, &range)
                        )
                    } else {
                        format!(
                            "only the following versions of {} {}",
                            package,
                            self.availability_range(package, &range)
                        )
                    }
                }
            }
            External::Custom(package, set, reason) => {
                if let Some(root) = self.format_root(package) {
                    format!("{root} cannot be used because {reason}")
                } else {
                    match reason {
                        UnavailableReason::Package(reason) => {
                            let message = reason.singular_message();
                            format!("{}{}", package, Padded::new(" ", &message, ""))
                        }
                        UnavailableReason::Version(reason) => {
                            let range = self.compatible_range(package, set);
                            let message = if range.plural() {
                                reason.plural_message()
                            } else {
                                reason.singular_message()
                            };
                            let context = reason.context_message(
                                self.tags,
                                self.python_requirement.target().abi_tag(),
                            );
                            if let Some(context) = context {
                                format!("{}{}{}", range, Padded::new(" ", &message, " "), context)
                            } else {
                                format!("{}{}", range, Padded::new(" ", &message, ""))
                            }
                        }
                    }
                }
            }
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                if package.name_no_root() == dependency.name_no_root() {
                    if let Some(member) = self.format_workspace_member(package) {
                        return format!(
                            "{member} depends on itself at an incompatible version ({})",
                            PackageRange::dependency(dependency, dependency_set, None)
                        );
                    }
                }

                if let Some(root) = self.format_root_requires(package) {
                    return format!(
                        "{root} {}",
                        self.dependency_range(dependency, dependency_set)
                    );
                }
                format!(
                    "{}",
                    self.compatible_range(package, package_set)
                        .depends_on(dependency, dependency_set),
                )
            }
        }
    }

    /// Try to print terms of an incompatibility in a human-readable way.
    fn format_terms(&self, terms: &Map<PubGrubPackage, Term<Range<Version>>>) -> String {
        let mut terms_vec: Vec<_> = terms.iter().collect();
        // We avoid relying on hashmap iteration order here by always sorting
        // by package first.
        terms_vec.sort_by(|&(pkg1, _), &(pkg2, _)| pkg1.cmp(pkg2));
        match terms_vec.as_slice() {
            [] => "the requirements are unsatisfiable".into(),
            [(root, _)] if matches!(&**(*root), PubGrubPackageInner::Root(_)) => {
                let root = self.format_root(root).unwrap();
                format!("{root} are unsatisfiable")
            }
            [(package, Term::Positive(range))]
                if matches!(&**(*package), PubGrubPackageInner::Package { .. }) =>
            {
                if let Some(member) = self.format_workspace_member(package) {
                    format!("{member}'s requirements are unsatisfiable")
                } else {
                    format!("{} cannot be used", self.compatible_range(package, range))
                }
            }
            [(package, Term::Negative(range))]
                if matches!(&**(*package), PubGrubPackageInner::Package { .. }) =>
            {
                format!("{} must be used", self.compatible_range(package, range))
            }
            [(p1, Term::Positive(r1)), (p2, Term::Negative(r2))] => self.format_external(
                &External::FromDependencyOf((*p1).clone(), r1.clone(), (*p2).clone(), r2.clone()),
            ),
            [(p1, Term::Negative(r1)), (p2, Term::Positive(r2))] => self.format_external(
                &External::FromDependencyOf((*p2).clone(), r2.clone(), (*p1).clone(), r1.clone()),
            ),
            slice => {
                let mut result = String::new();
                let str_terms: Vec<_> = slice
                    .iter()
                    .map(|(p, t)| format!("{}", PackageTerm::new(p, t, self)))
                    .collect();
                for (index, term) in str_terms.iter().enumerate() {
                    result.push_str(term);
                    match str_terms.len().cmp(&2) {
                        Ordering::Equal if index == 0 => {
                            result.push_str(" and ");
                        }
                        Ordering::Greater if index + 1 < str_terms.len() => {
                            result.push_str(", ");
                        }
                        _ => (),
                    }
                }
                if slice.len() == 1 {
                    result.push_str(" cannot be used");
                } else {
                    result.push_str(" are incompatible");
                }
                result
            }
        }
    }

    /// Simplest case, we just combine two external incompatibilities.
    fn explain_both_external(
        &self,
        external1: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        external2: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let external = self.format_both_external(external1, external2);
        let terms = self.format_terms(current_terms);

        format!(
            "Because {}we can conclude that {}",
            Padded::from_string("", &external, ", "),
            Padded::from_string("", &terms, "."),
        )
    }

    /// Both causes have already been explained so we use their refs.
    fn explain_both_ref(
        &self,
        ref_id1: usize,
        derived1: &Derived<PubGrubPackage, Range<Version>, UnavailableReason>,
        ref_id2: usize,
        derived2: &Derived<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        // TODO: order should be chosen to make it more logical.

        let derived1_terms = self.format_terms(&derived1.terms);
        let derived2_terms = self.format_terms(&derived2.terms);
        let current_terms = self.format_terms(current_terms);

        format!(
            "Because we know from ({}) that {}and we know from ({}) that {}{}",
            ref_id1,
            Padded::new("", &derived1_terms, " "),
            ref_id2,
            Padded::new("", &derived2_terms, ", "),
            Padded::new("", &current_terms, "."),
        )
    }

    /// One cause is derived (already explained so one-line),
    /// the other is a one-line external cause,
    /// and finally we conclude with the current incompatibility.
    fn explain_ref_and_external(
        &self,
        ref_id: usize,
        derived: &Derived<PubGrubPackage, Range<Version>, UnavailableReason>,
        external: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        // TODO: order should be chosen to make it more logical.

        let derived_terms = self.format_terms(&derived.terms);
        let external = self.format_external(external);
        let current_terms = self.format_terms(current_terms);

        format!(
            "Because we know from ({}) that {}and {}we can conclude that {}",
            ref_id,
            Padded::new("", &derived_terms, " "),
            Padded::new("", &external, ", "),
            Padded::new("", &current_terms, "."),
        )
    }

    /// Add an external cause to the chain of explanations.
    fn and_explain_external(
        &self,
        external: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let external = self.format_external(external);
        let terms = self.format_terms(current_terms);

        format!(
            "And because {}we can conclude that {}",
            Padded::from_string("", &external, ", "),
            Padded::from_string("", &terms, "."),
        )
    }

    /// Add an already explained incompat to the chain of explanations.
    fn and_explain_ref(
        &self,
        ref_id: usize,
        derived: &Derived<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let derived = self.format_terms(&derived.terms);
        let current = self.format_terms(current_terms);

        format!(
            "And because we know from ({}) that {}we can conclude that {}",
            ref_id,
            Padded::from_string("", &derived, ", "),
            Padded::from_string("", &current, "."),
        )
    }

    /// Add an already explained incompat to the chain of explanations.
    fn and_explain_prior_and_external(
        &self,
        prior_external: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        external: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let external = self.format_both_external(prior_external, external);
        let terms = self.format_terms(current_terms);

        format!(
            "And because {}we can conclude that {}",
            Padded::from_string("", &external, ", "),
            Padded::from_string("", &terms, "."),
        )
    }
}

impl PubGrubReportFormatter<'_> {
    /// Return the formatting for "the root package requires", if the given
    /// package is the root package.
    ///
    /// If not given the root package, returns `None`.
    fn format_root_requires(&self, package: &PubGrubPackage) -> Option<String> {
        if self.is_workspace() {
            if matches!(&**package, PubGrubPackageInner::Root(_)) {
                if self.is_single_project_workspace() {
                    return Some("your project requires".to_string());
                }
                return Some("your workspace requires".to_string());
            }
        }
        match &**package {
            PubGrubPackageInner::Root(Some(name)) => Some(format!("{name} depends on")),
            PubGrubPackageInner::Root(None) => Some("you require".to_string()),
            _ => None,
        }
    }

    /// Return the formatting for "the root package", if the given
    /// package is the root package.
    ///
    /// If not given the root package, returns `None`.
    fn format_root(&self, package: &PubGrubPackage) -> Option<String> {
        if self.is_workspace() {
            if matches!(&**package, PubGrubPackageInner::Root(_)) {
                if self.is_single_project_workspace() {
                    return Some("your project's requirements".to_string());
                }
                return Some("your workspace's requirements".to_string());
            }
        }
        match &**package {
            PubGrubPackageInner::Root(Some(_)) => Some("your requirements".to_string()),
            PubGrubPackageInner::Root(None) => Some("your requirements".to_string()),
            _ => None,
        }
    }

    /// Whether the resolution error is for a workspace.
    fn is_workspace(&self) -> bool {
        !self.workspace_members.is_empty()
    }

    /// Whether the resolution error is for a workspace with a exactly one project.
    fn is_single_project_workspace(&self) -> bool {
        self.workspace_members.len() == 1
    }

    /// Return a display name for the package if it is a workspace member.
    fn format_workspace_member(&self, package: &PubGrubPackage) -> Option<String> {
        match &**package {
            // TODO(zanieb): Improve handling of dev and extra for single-project workspaces
            PubGrubPackageInner::Package {
                name, extra, dev, ..
            } if self.workspace_members.contains(name) => {
                if self.is_single_project_workspace() && extra.is_none() && dev.is_none() {
                    Some("your project".to_string())
                } else {
                    Some(format!("{package}"))
                }
            }
            PubGrubPackageInner::Extra { name, .. } if self.workspace_members.contains(name) => {
                Some(format!("{package}"))
            }
            PubGrubPackageInner::Dev { name, .. } if self.workspace_members.contains(name) => {
                Some(format!("{package}"))
            }
            _ => None,
        }
    }

    /// Return whether the given package is the root package.
    fn is_root(package: &PubGrubPackage) -> bool {
        matches!(&**package, PubGrubPackageInner::Root(_))
    }

    /// Return whether the given package is a workspace member.
    fn is_single_project_workspace_member(&self, package: &PubGrubPackage) -> bool {
        match &**package {
            // TODO(zanieb): Improve handling of dev and extra for single-project workspaces
            PubGrubPackageInner::Package {
                name, extra, dev, ..
            } if self.workspace_members.contains(name) => {
                self.is_single_project_workspace() && extra.is_none() && dev.is_none()
            }
            _ => false,
        }
    }

    /// Create a [`PackageRange::compatibility`] display with this formatter attached.
    fn compatible_range<'a>(
        &'a self,
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> PackageRange<'a> {
        PackageRange::compatibility(package, range, Some(self))
    }

    /// Create a [`PackageRange::dependency`] display with this formatter attached.
    fn dependency_range<'a>(
        &'a self,
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> PackageRange<'a> {
        PackageRange::dependency(package, range, Some(self))
    }

    /// Create a [`PackageRange::availability`] display with this formatter attached.
    fn availability_range<'a>(
        &'a self,
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> PackageRange<'a> {
        PackageRange::availability(package, range, Some(self))
    }

    /// Format two external incompatibilities, combining them if possible.
    fn format_both_external(
        &self,
        external1: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
        external2: &External<PubGrubPackage, Range<Version>, UnavailableReason>,
    ) -> String {
        match (external1, external2) {
            (
                External::FromDependencyOf(package1, package_set1, dependency1, dependency_set1),
                External::FromDependencyOf(package2, _, dependency2, dependency_set2),
            ) if package1 == package2 => {
                let dependency1 = self.dependency_range(dependency1, dependency_set1);
                let dependency2 = self.dependency_range(dependency2, dependency_set2);

                if let Some(root) = self.format_root_requires(package1) {
                    return format!(
                        "{root} {}and {}",
                        Padded::new("", &dependency1, " "),
                        dependency2,
                    );
                }

                format!(
                    "{}",
                    self.compatible_range(package1, package_set1)
                        .depends_on(dependency1.package, dependency_set1)
                        .and(dependency2.package, dependency_set2),
                )
            }
            (.., External::FromDependencyOf(package, _, dependency, _))
                if Self::is_root(package)
                    && self.is_single_project_workspace_member(dependency) =>
            {
                self.format_external(external1)
            }
            (External::FromDependencyOf(package, _, dependency, _), ..)
                if Self::is_root(package)
                    && self.is_single_project_workspace_member(dependency) =>
            {
                self.format_external(external2)
            }
            _ => {
                let external1 = self.format_external(external1);
                let external2 = self.format_external(external2);

                format!(
                    "{}and {}",
                    Padded::from_string("", &external1, " "),
                    &external2,
                )
            }
        }
    }

    /// Generate the [`PubGrubHints`] for a derivation tree.
    ///
    /// The [`PubGrubHints`] help users resolve errors by providing additional context or modifying
    /// their requirements.
    pub(crate) fn generate_hints(
        &self,
        derivation_tree: &ErrorTree,
        index: &InMemoryIndex,
        selector: &CandidateSelector,
        index_locations: &IndexLocations,
        index_capabilities: &IndexCapabilities,
        available_indexes: &FxHashMap<PackageName, BTreeSet<IndexUrl>>,
        unavailable_packages: &FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: &FxHashMap<PackageName, BTreeMap<Version, MetadataUnavailable>>,
        fork_urls: &ForkUrls,
        fork_indexes: &ForkIndexes,
        env: &ResolverEnvironment,
        tags: Option<&Tags>,
        workspace_members: &BTreeSet<PackageName>,
        options: &Options,
        output_hints: &mut IndexSet<PubGrubHint>,
    ) {
        match derivation_tree {
            DerivationTree::External(External::Custom(package, set, reason)) => {
                if let Some(name) = package.name_no_root() {
                    // Check for no versions due to pre-release options.
                    if options.flexibility == Flexibility::Configurable {
                        if !fork_urls.contains_key(name) {
                            self.prerelease_available_hint(name, set, selector, env, output_hints);
                        }
                    }

                    // Check for no versions due to no `--find-links` flat index.
                    Self::index_hints(
                        name,
                        set,
                        selector,
                        index_locations,
                        index_capabilities,
                        available_indexes,
                        unavailable_packages,
                        incomplete_packages,
                        output_hints,
                    );

                    if let UnavailableReason::Version(UnavailableVersion::IncompatibleDist(
                        incompatibility,
                    )) = reason
                    {
                        match incompatibility {
                            // Check for unavailable versions due to `--no-build` or `--no-binary`.
                            IncompatibleDist::Wheel(IncompatibleWheel::NoBinary) => {
                                output_hints.insert(PubGrubHint::NoBinary {
                                    package: name.clone(),
                                    option: options.build_options.no_binary().clone(),
                                });
                            }
                            IncompatibleDist::Source(IncompatibleSource::NoBuild) => {
                                output_hints.insert(PubGrubHint::NoBuild {
                                    package: name.clone(),
                                    option: options.build_options.no_build().clone(),
                                });
                            }
                            // Check for unavailable versions due to incompatible tags.
                            IncompatibleDist::Wheel(IncompatibleWheel::Tag(tag)) => {
                                if let Some(hint) = self.tag_hint(
                                    name,
                                    set,
                                    *tag,
                                    index,
                                    selector,
                                    fork_indexes,
                                    env,
                                    tags,
                                ) {
                                    output_hints.insert(hint);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            DerivationTree::External(External::NoVersions(package, set)) => {
                if let Some(name) = package.name_no_root() {
                    // Check for no versions due to pre-release options.
                    if options.flexibility == Flexibility::Configurable {
                        if !fork_urls.contains_key(name) {
                            self.prerelease_available_hint(name, set, selector, env, output_hints);
                        }
                    }

                    // Check for no versions due to no `--find-links` flat index.
                    Self::index_hints(
                        name,
                        set,
                        selector,
                        index_locations,
                        index_capabilities,
                        available_indexes,
                        unavailable_packages,
                        incomplete_packages,
                        output_hints,
                    );
                }
            }
            DerivationTree::External(External::FromDependencyOf(
                package,
                package_set,
                dependency,
                dependency_set,
            )) => {
                // Check for a dependency on a workspace package by a non-workspace package.
                // Generally, this indicates that the workspace package is shadowing a transitive
                // dependency name.
                if let (Some(package_name), Some(dependency_name)) =
                    (package.name(), dependency.name())
                {
                    if workspace_members.contains(dependency_name)
                        && !workspace_members.contains(package_name)
                    {
                        output_hints.insert(PubGrubHint::DependsOnWorkspacePackage {
                            package: package_name.clone(),
                            dependency: dependency_name.clone(),
                            workspace: self.is_workspace() && !self.is_single_project_workspace(),
                        });
                    }

                    if package_name == dependency_name
                        && (dependency.extra().is_none() || package.extra() == dependency.extra())
                        && (dependency.dev().is_none() || dependency.dev() == package.dev())
                        && workspace_members.contains(package_name)
                    {
                        output_hints.insert(PubGrubHint::DependsOnItself {
                            package: package_name.clone(),
                            workspace: self.is_workspace() && !self.is_single_project_workspace(),
                        });
                    }
                }
                // Check for no versions due to `Requires-Python`.
                if matches!(
                    &**dependency,
                    PubGrubPackageInner::Python(PubGrubPython::Target)
                ) {
                    if let Some(name) = package.name() {
                        output_hints.insert(PubGrubHint::RequiresPython {
                            source: self.python_requirement.source(),
                            requires_python: self.python_requirement.target().clone(),
                            name: name.clone(),
                            package_set: package_set.clone(),
                            package_requires_python: dependency_set.clone(),
                        });
                    }
                }
            }
            DerivationTree::External(External::NotRoot(..)) => {}
            DerivationTree::Derived(derived) => {
                self.generate_hints(
                    &derived.cause1,
                    index,
                    selector,
                    index_locations,
                    index_capabilities,
                    available_indexes,
                    unavailable_packages,
                    incomplete_packages,
                    fork_urls,
                    fork_indexes,
                    env,
                    tags,
                    workspace_members,
                    options,
                    output_hints,
                );
                self.generate_hints(
                    &derived.cause2,
                    index,
                    selector,
                    index_locations,
                    index_capabilities,
                    available_indexes,
                    unavailable_packages,
                    incomplete_packages,
                    fork_urls,
                    fork_indexes,
                    env,
                    tags,
                    workspace_members,
                    options,
                    output_hints,
                );
            }
        };
    }

    /// Generate a [`PubGrubHint`] for a package that doesn't have any wheels matching the current
    /// Python version, ABI, or platform.
    fn tag_hint(
        &self,
        name: &PackageName,
        set: &Range<Version>,
        tag: IncompatibleTag,
        index: &InMemoryIndex,
        selector: &CandidateSelector,
        fork_indexes: &ForkIndexes,
        env: &ResolverEnvironment,
        tags: Option<&Tags>,
    ) -> Option<PubGrubHint> {
        let response = if let Some(url) = fork_indexes.get(name) {
            index.explicit().get(&(name.clone(), url.clone()))
        } else {
            index.implicit().get(name)
        }?;

        let VersionsResponse::Found(version_maps) = &*response else {
            return None;
        };

        let candidate = selector.select_no_preference(name, set, version_maps, env)?;

        let prioritized = candidate.prioritized()?;

        match tag {
            IncompatibleTag::Invalid => None,
            IncompatibleTag::Python => {
                let best = tags.and_then(Tags::python_tag);
                let tags = prioritized.python_tags().collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(PubGrubHint::LanguageTags {
                        package: name.clone(),
                        version: candidate.version().clone(),
                        tags,
                        best,
                    })
                }
            }
            IncompatibleTag::Abi | IncompatibleTag::AbiPythonVersion => {
                let best = tags.and_then(Tags::abi_tag);
                let tags = prioritized
                    .abi_tags()
                    // Ignore `none`, which is universally compatible.
                    //
                    // As an example, `none` can appear here if we're solving for Python 3.13, and
                    // the distribution includes a wheel for `cp312-none-macosx_11_0_arm64`.
                    //
                    // In that case, the wheel isn't compatible, but when solving for Python 3.13,
                    // the `cp312` Python tag _can_ be compatible (e.g., for `cp312-abi3-macosx_11_0_arm64.whl`),
                    // so this is considered an ABI incompatibility rather than Python incompatibility.
                    .filter(|tag| *tag != AbiTag::None)
                    .collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(PubGrubHint::AbiTags {
                        package: name.clone(),
                        version: candidate.version().clone(),
                        tags,
                        best,
                    })
                }
            }
            IncompatibleTag::Platform => {
                // We don't want to report all available platforms, since it's plausible that there
                // are wheels for the current platform, but at a different ABI. For example, when
                // solving for Python 3.13 on macOS, `cp312-cp312-macosx_11_0_arm64` could be
                // available along with `cp313-cp313-manylinux2014`. In this case, we'd consider
                // the distribution to be platform-incompatible, since `cp313-cp313` matches the
                // compatible wheel tags. But showing `macosx_11_0_arm64` here would be misleading.
                //
                // So, instead, we only show the platforms that are linked to otherwise-compatible
                // wheels (e.g., `manylinux2014` in `cp313-cp313-manylinux2014`). In other words,
                // we only show platforms for ABI-compatible wheels.
                let tags = prioritized
                    .platform_tags(self.tags?)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(PubGrubHint::PlatformTags {
                        package: name.clone(),
                        version: candidate.version().clone(),
                        tags,
                    })
                }
            }
        }
    }

    fn index_hints(
        name: &PackageName,
        set: &Range<Version>,
        selector: &CandidateSelector,
        index_locations: &IndexLocations,
        index_capabilities: &IndexCapabilities,
        available_indexes: &FxHashMap<PackageName, BTreeSet<IndexUrl>>,
        unavailable_packages: &FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: &FxHashMap<PackageName, BTreeMap<Version, MetadataUnavailable>>,
        hints: &mut IndexSet<PubGrubHint>,
    ) {
        let no_find_links = index_locations.flat_indexes().peekable().peek().is_none();

        // Add hints due to the package being entirely unavailable.
        match unavailable_packages.get(name) {
            Some(UnavailablePackage::NoIndex) => {
                if no_find_links {
                    hints.insert(PubGrubHint::NoIndex);
                }
            }
            Some(UnavailablePackage::Offline) => {
                hints.insert(PubGrubHint::Offline);
            }
            Some(UnavailablePackage::InvalidMetadata(reason)) => {
                hints.insert(PubGrubHint::InvalidPackageMetadata {
                    package: name.clone(),
                    reason: reason.clone(),
                });
            }
            Some(UnavailablePackage::InvalidStructure(reason)) => {
                hints.insert(PubGrubHint::InvalidPackageStructure {
                    package: name.clone(),
                    reason: reason.clone(),
                });
            }
            Some(UnavailablePackage::NotFound) => {}
            None => {}
        }

        // Add hints due to the package being unavailable at specific versions.
        if let Some(versions) = incomplete_packages.get(name) {
            for (version, incomplete) in versions.iter().rev() {
                if set.contains(version) {
                    match incomplete {
                        MetadataUnavailable::Offline => {
                            hints.insert(PubGrubHint::Offline);
                        }
                        MetadataUnavailable::InvalidMetadata(reason) => {
                            hints.insert(PubGrubHint::InvalidVersionMetadata {
                                package: name.clone(),
                                version: version.clone(),
                                reason: reason.to_string(),
                            });
                        }
                        MetadataUnavailable::InconsistentMetadata(reason) => {
                            hints.insert(PubGrubHint::InconsistentVersionMetadata {
                                package: name.clone(),
                                version: version.clone(),
                                reason: reason.to_string(),
                            });
                        }
                        MetadataUnavailable::InvalidStructure(reason) => {
                            hints.insert(PubGrubHint::InvalidVersionStructure {
                                package: name.clone(),
                                version: version.clone(),
                                reason: reason.to_string(),
                            });
                        }
                        MetadataUnavailable::RequiresPython(requires_python, python_version) => {
                            hints.insert(PubGrubHint::IncompatibleBuildRequirement {
                                package: name.clone(),
                                version: version.clone(),
                                requires_python: requires_python.clone(),
                                python_version: python_version.clone(),
                            });
                        }
                    }
                    break;
                }
            }
        }

        // Add hints due to the package being available on an index, but not at the correct version,
        // with subsequent indexes that were _not_ queried.
        if matches!(selector.index_strategy(), IndexStrategy::FirstIndex) {
            // Do not include the hint if the set is "all versions". This is an unusual but valid
            // case in which a package returns a 200 response, but without any versions or
            // distributions for the package.
            if !set
                .iter()
                .all(|range| matches!(range, (Bound::Unbounded, Bound::Unbounded)))
            {
                if let Some(found_index) = available_indexes.get(name).and_then(BTreeSet::first) {
                    // Determine whether the index is the last-available index. If not, then some
                    // indexes were not queried, and could contain a compatible version.
                    if let Some(next_index) = index_locations
                        .indexes()
                        .map(Index::url)
                        .skip_while(|url| *url != found_index)
                        .nth(1)
                    {
                        hints.insert(PubGrubHint::UncheckedIndex {
                            name: name.clone(),
                            range: set.clone(),
                            found_index: found_index.clone(),
                            next_index: next_index.clone(),
                        });
                    }
                }
            }
        }

        // Add hints due to an index returning an unauthorized response.
        for index in index_locations.allowed_indexes() {
            if index_capabilities.unauthorized(&index.url) {
                hints.insert(PubGrubHint::UnauthorizedIndex {
                    index: index.url.clone(),
                });
            }
            if index_capabilities.forbidden(&index.url) {
                // If the index is a PyTorch index (e.g., `https://download.pytorch.org/whl/cu118`),
                // avoid noting the lack of credentials. PyTorch returns a 403 (Forbidden) status
                // code for any package that does not exist.
                if index.url.url().host_str() == Some("download.pytorch.org") {
                    continue;
                }
                hints.insert(PubGrubHint::ForbiddenIndex {
                    index: index.url.clone(),
                });
            }
        }
    }

    fn prerelease_available_hint(
        &self,
        name: &PackageName,
        set: &Range<Version>,
        selector: &CandidateSelector,
        env: &ResolverEnvironment,
        hints: &mut IndexSet<PubGrubHint>,
    ) {
        let any_prerelease = set.iter().any(|(start, end)| {
            let is_pre1 = match start {
                Bound::Included(version) => version.any_prerelease(),
                Bound::Excluded(version) => version.any_prerelease(),
                Bound::Unbounded => false,
            };
            let is_pre2 = match end {
                Bound::Included(version) => version.any_prerelease(),
                Bound::Excluded(version) => version.any_prerelease(),
                Bound::Unbounded => false,
            };
            is_pre1 || is_pre2
        });

        if any_prerelease {
            // A pre-release marker appeared in the version requirements.
            if selector.prerelease_strategy().allows(name, env) != AllowPrerelease::Yes {
                hints.insert(PubGrubHint::PrereleaseRequested {
                    name: name.clone(),
                    range: set.clone(),
                });
            }
        } else if let Some(version) = self.available_versions.get(name).and_then(|versions| {
            versions
                .iter()
                .rev()
                .filter(|version| version.any_prerelease())
                .find(|version| set.contains(version))
        }) {
            // There are pre-release versions available for the package.
            if selector.prerelease_strategy().allows(name, env) != AllowPrerelease::Yes {
                hints.insert(PubGrubHint::PrereleaseAvailable {
                    package: name.clone(),
                    version: version.clone(),
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PubGrubHint {
    /// There are pre-release versions available for a package, but pre-releases weren't enabled
    /// for that package.
    ///
    PrereleaseAvailable {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
    },
    /// A requirement included a pre-release marker, but pre-releases weren't enabled for that
    /// package.
    PrereleaseRequested {
        name: PackageName,
        // excluded from `PartialEq` and `Hash`
        range: Range<Version>,
    },
    /// Requirements were unavailable due to lookups in the index being disabled and no extra
    /// index was provided via `--find-links`
    NoIndex,
    /// A package was not found in the registry, but network access was disabled.
    Offline,
    /// Metadata for a package could not be parsed.
    InvalidPackageMetadata {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The structure of a package was invalid (e.g., multiple `.dist-info` directories).
    InvalidPackageStructure {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// Metadata for a package version could not be parsed.
    InvalidVersionMetadata {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// Metadata for a package version was inconsistent (e.g., the package name did not match that
    /// of the file).
    InconsistentVersionMetadata {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The structure of a package version was invalid (e.g., multiple `.dist-info` directories).
    InvalidVersionStructure {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The source distribution has a `requires-python` requirement that is not met by the installed
    /// Python version (and static metadata is not available).
    IncompatibleBuildRequirement {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        requires_python: VersionSpecifiers,
        // excluded from `PartialEq` and `Hash`
        python_version: Version,
    },
    /// The `Requires-Python` requirement was not satisfied.
    RequiresPython {
        source: PythonRequirementSource,
        requires_python: RequiresPython,
        // excluded from `PartialEq` and `Hash`
        name: PackageName,
        // excluded from `PartialEq` and `Hash`
        package_set: Range<Version>,
        // excluded from `PartialEq` and `Hash`
        package_requires_python: Range<Version>,
    },
    /// A non-workspace package depends on a workspace package, which is likely shadowing a
    /// transitive dependency.
    DependsOnWorkspacePackage {
        package: PackageName,
        dependency: PackageName,
        workspace: bool,
    },
    /// A package depends on itself at an incompatible version.
    DependsOnItself {
        package: PackageName,
        workspace: bool,
    },
    /// A package was available on an index, but not at the correct version, and at least one
    /// subsequent index was not queried. As such, a compatible version may be available on
    /// one of the remaining indexes.
    UncheckedIndex {
        name: PackageName,
        // excluded from `PartialEq` and `Hash`
        range: Range<Version>,
        // excluded from `PartialEq` and `Hash`
        found_index: IndexUrl,
        // excluded from `PartialEq` and `Hash`
        next_index: IndexUrl,
    },
    /// No wheels are available for a package, and using source distributions was disabled.
    NoBuild {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        option: NoBuild,
    },
    /// No source distributions are available for a package, and using pre-built wheels was disabled.
    NoBinary {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        option: NoBinary,
    },
    /// An index returned an Unauthorized (401) response.
    UnauthorizedIndex { index: IndexUrl },
    /// An index returned a Forbidden (403) response.
    ForbiddenIndex { index: IndexUrl },
    /// None of the available wheels for a package have a compatible Python language tag (e.g.,
    /// `cp310` in `cp310-abi3-manylinux_2_17_x86_64.whl`).
    LanguageTags {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        tags: BTreeSet<LanguageTag>,
        // excluded from `PartialEq` and `Hash`
        best: Option<LanguageTag>,
    },
    /// None of the available wheels for a package have a compatible ABI tag (e.g., `abi3` in
    /// `cp310-abi3-manylinux_2_17_x86_64.whl`).
    AbiTags {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        tags: BTreeSet<AbiTag>,
        // excluded from `PartialEq` and `Hash`
        best: Option<AbiTag>,
    },
    /// None of the available wheels for a package have a compatible platform tag (e.g.,
    /// `manylinux_2_17_x86_64` in `cp310-abi3-manylinux_2_17_x86_64.whl`).
    PlatformTags {
        package: PackageName,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        tags: BTreeSet<PlatformTag>,
    },
}

/// This private enum mirrors [`PubGrubHint`] but only includes fields that should be
/// used for `Eq` and `Hash` implementations. It is used to derive `PartialEq` and
/// `Hash` implementations for [`PubGrubHint`].
#[derive(PartialEq, Eq, Hash)]
enum PubGrubHintCore {
    PrereleaseAvailable {
        package: PackageName,
    },
    PrereleaseRequested {
        package: PackageName,
    },
    NoIndex,
    Offline,
    InvalidPackageMetadata {
        package: PackageName,
    },
    InvalidPackageStructure {
        package: PackageName,
    },
    InvalidVersionMetadata {
        package: PackageName,
    },
    InconsistentVersionMetadata {
        package: PackageName,
    },
    InvalidVersionStructure {
        package: PackageName,
    },
    IncompatibleBuildRequirement {
        package: PackageName,
    },
    RequiresPython {
        source: PythonRequirementSource,
        requires_python: RequiresPython,
    },
    DependsOnWorkspacePackage {
        package: PackageName,
        dependency: PackageName,
        workspace: bool,
    },
    DependsOnItself {
        package: PackageName,
        workspace: bool,
    },
    UncheckedIndex {
        package: PackageName,
    },
    UnauthorizedIndex {
        index: IndexUrl,
    },
    ForbiddenIndex {
        index: IndexUrl,
    },
    NoBuild {
        package: PackageName,
    },
    NoBinary {
        package: PackageName,
    },
    LanguageTags {
        package: PackageName,
    },
    AbiTags {
        package: PackageName,
    },
    PlatformTags {
        package: PackageName,
    },
}

impl From<PubGrubHint> for PubGrubHintCore {
    #[inline]
    fn from(hint: PubGrubHint) -> Self {
        match hint {
            PubGrubHint::PrereleaseAvailable { package, .. } => {
                Self::PrereleaseAvailable { package }
            }
            PubGrubHint::PrereleaseRequested { name: package, .. } => {
                Self::PrereleaseRequested { package }
            }
            PubGrubHint::NoIndex => Self::NoIndex,
            PubGrubHint::Offline => Self::Offline,
            PubGrubHint::InvalidPackageMetadata { package, .. } => {
                Self::InvalidPackageMetadata { package }
            }
            PubGrubHint::InvalidPackageStructure { package, .. } => {
                Self::InvalidPackageStructure { package }
            }
            PubGrubHint::InvalidVersionMetadata { package, .. } => {
                Self::InvalidVersionMetadata { package }
            }
            PubGrubHint::InconsistentVersionMetadata { package, .. } => {
                Self::InconsistentVersionMetadata { package }
            }
            PubGrubHint::InvalidVersionStructure { package, .. } => {
                Self::InvalidVersionStructure { package }
            }
            PubGrubHint::IncompatibleBuildRequirement { package, .. } => {
                Self::IncompatibleBuildRequirement { package }
            }
            PubGrubHint::RequiresPython {
                source,
                requires_python,
                ..
            } => Self::RequiresPython {
                source,
                requires_python,
            },
            PubGrubHint::DependsOnWorkspacePackage {
                package,
                dependency,
                workspace,
            } => Self::DependsOnWorkspacePackage {
                package,
                dependency,
                workspace,
            },
            PubGrubHint::DependsOnItself { package, workspace } => {
                Self::DependsOnItself { package, workspace }
            }
            PubGrubHint::UncheckedIndex { name: package, .. } => Self::UncheckedIndex { package },
            PubGrubHint::UnauthorizedIndex { index } => Self::UnauthorizedIndex { index },
            PubGrubHint::ForbiddenIndex { index } => Self::ForbiddenIndex { index },
            PubGrubHint::NoBuild { package, .. } => Self::NoBuild { package },
            PubGrubHint::NoBinary { package, .. } => Self::NoBinary { package },
            PubGrubHint::LanguageTags { package, .. } => Self::LanguageTags { package },
            PubGrubHint::AbiTags { package, .. } => Self::AbiTags { package },
            PubGrubHint::PlatformTags { package, .. } => Self::PlatformTags { package },
        }
    }
}

impl std::hash::Hash for PubGrubHint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let core = PubGrubHintCore::from(self.clone());
        core.hash(state);
    }
}

impl PartialEq for PubGrubHint {
    fn eq(&self, other: &Self) -> bool {
        let core = PubGrubHintCore::from(self.clone());
        let other_core = PubGrubHintCore::from(other.clone());
        core == other_core
    }
}

impl Eq for PubGrubHint {}

impl std::fmt::Display for PubGrubHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrereleaseAvailable { package, version } => {
                write!(
                    f,
                    "{}{} Pre-releases are available for `{}` in the requested range (e.g., {}), but pre-releases weren't enabled (try: `{}`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    version.cyan(),
                    "--prerelease=allow".green(),
                )
            }
            Self::PrereleaseRequested { name, range } => {
                write!(
                    f,
                    "{}{} `{}` was requested with a pre-release marker (e.g., {}), but pre-releases weren't enabled (try: `{}`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    name.cyan(),
                    PackageRange::compatibility(&PubGrubPackage::base(name), range, None).cyan(),
                    "--prerelease=allow".green(),
                )
            }
            Self::NoIndex => {
                write!(
                    f,
                    "{}{} Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `{}`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    "--find-links <uri>".green(),
                )
            }
            Self::Offline => {
                write!(
                    f,
                    "{}{} Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.",
                    "hint".bold().cyan(),
                    ":".bold(),
                )
            }
            Self::InvalidPackageMetadata { package, reason } => {
                write!(
                    f,
                    "{}{} Metadata for `{}` could not be parsed:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::InvalidPackageStructure { package, reason } => {
                write!(
                    f,
                    "{}{} The structure of `{}` was invalid:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::InvalidVersionMetadata {
                package,
                version,
                reason,
            } => {
                write!(
                    f,
                    "{}{} Metadata for `{}` ({}) could not be parsed:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    format!("v{version}").cyan(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::InvalidVersionStructure {
                package,
                version,
                reason,
            } => {
                write!(
                    f,
                    "{}{} The structure of `{}` ({}) was invalid:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    format!("v{version}").cyan(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::InconsistentVersionMetadata {
                package,
                version,
                reason,
            } => {
                write!(
                    f,
                    "{}{} Metadata for `{}` ({}) was inconsistent:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    format!("v{version}").cyan(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::RequiresPython,
                requires_python,
                name,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The `requires-python` value ({}) includes Python versions that are not supported by your dependencies (e.g., {} only supports {}). Consider using a more restrictive `requires-python` value (like {}).",
                    "hint".bold().cyan(),
                    ":".bold(),
                    requires_python.cyan(),
                    PackageRange::compatibility(&PubGrubPackage::base(name), package_set, None).cyan(),
                    package_requires_python.cyan(),
                    package_requires_python.cyan(),
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::PythonVersion,
                requires_python,
                name,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The `--python-version` value ({}) includes Python versions that are not supported by your dependencies (e.g., {} only supports {}). Consider using a higher `--python-version` value.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    requires_python.cyan(),
                    PackageRange::compatibility(&PubGrubPackage::base(name), package_set, None).cyan(),
                    package_requires_python.cyan(),
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::Interpreter,
                requires_python: _,
                name,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The Python interpreter uses a Python version that is not supported by your dependencies (e.g., {} only supports {}). Consider passing a `--python-version` value to raise the minimum supported version.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    PackageRange::compatibility(&PubGrubPackage::base(name), package_set, None).cyan(),
                    package_requires_python.cyan(),
                )
            }
            Self::IncompatibleBuildRequirement {
                package,
                version,
                requires_python,
                python_version,
            } => {
                write!(
                    f,
                    "{}{} The source distribution for `{}` ({}) does not include static metadata. Generating metadata for this package requires Python {}, but Python {} is installed.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    format!("v{version}").cyan(),
                    requires_python.cyan(),
                    python_version.cyan(),
                )
            }
            Self::DependsOnWorkspacePackage {
                package,
                dependency,
                workspace,
            } => {
                let your_project = if *workspace {
                    "one of your workspace members"
                } else {
                    "your project"
                };
                let the_project = if *workspace {
                    "the workspace member"
                } else {
                    "the project"
                };
                write!(
                    f,
                    "{}{} The package `{}` depends on the package `{}` but the name is shadowed by {your_project}. Consider changing the name of {the_project}.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    dependency.cyan(),
                )
            }
            Self::DependsOnItself { package, workspace } => {
                let project = if *workspace {
                    "workspace member"
                } else {
                    "project"
                };
                write!(
                    f,
                    "{}{} The {project} `{}` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `{}`, consider renaming the {project} `{}` to avoid creating a conflict.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    package.cyan(),
                    package.cyan(),
                )
            }
            Self::UncheckedIndex {
                name,
                range,
                found_index,
                next_index,
            } => {
                write!(
                    f,
                    "{}{} `{}` was found on {}, but not at the requested version ({}). A compatible version may be available on a subsequent index (e.g., {}). By default, uv will only consider versions that are published on the first index that contains a given package, to avoid dependency confusion attacks. If all indexes are equally trusted, use `{}` to consider all versions from all indexes, regardless of the order in which they were defined.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    name.cyan(),
                    found_index.cyan(),
                    PackageRange::compatibility(&PubGrubPackage::base(name), range, None).cyan(),
                    next_index.cyan(),
                    "--index-strategy unsafe-best-match".green(),
                )
            }
            Self::UnauthorizedIndex { index } => {
                write!(
                    f,
                    "{}{} An index URL ({}) could not be queried due to a lack of valid authentication credentials ({}).",
                    "hint".bold().cyan(),
                    ":".bold(),
                    index.redacted().cyan(),
                    "401 Unauthorized".red(),
                )
            }
            Self::ForbiddenIndex { index } => {
                write!(
                    f,
                    "{}{} An index URL ({}) could not be queried due to a lack of valid authentication credentials ({}).",
                    "hint".bold().cyan(),
                    ":".bold(),
                    index.redacted().cyan(),
                    "403 Forbidden".red(),
                )
            }
            Self::NoBuild { package, option } => {
                let option = match option {
                    NoBuild::All => "for all packages (i.e., with `--no-build`)".to_string(),
                    NoBuild::Packages(_) => {
                        format!("for `{package}` (i.e., with `--no-build-package {package}`)")
                    }
                    NoBuild::None => unreachable!(),
                };
                write!(
                    f,
                    "{}{} Wheels are required for `{}` because building from source is disabled {option}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                )
            }
            Self::NoBinary { package, option } => {
                let option = match option {
                    NoBinary::All => "for all packages (i.e., with `--no-binary`)".to_string(),
                    NoBinary::Packages(_) => {
                        format!("for `{package}` (i.e., with `--no-binary-package {package}`)")
                    }
                    NoBinary::None => unreachable!(),
                };
                write!(
                    f,
                    "{}{} A source distribution is required for `{}` because using pre-built wheels is disabled {option}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                )
            }
            Self::LanguageTags {
                package,
                version,
                tags,
                best,
            } => {
                if let Some(best) = best {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    let best = if let Some(pretty) = best.pretty() {
                        format!("{} (`{}`)", pretty.cyan(), best.cyan())
                    } else {
                        format!("{}", best.cyan())
                    };
                    write!(
                        f,
                        "{}{} You require {}, but we only found wheels for `{}` ({}) with the following Python implementation tag{s}: {}",
                        "hint".bold().cyan(),
                        ":".bold(),
                        best,
                        package.cyan(),
                        format!("v{version}").cyan(),
                        tags.iter()
                            .map(|tag| format!("`{}`", tag.cyan()))
                            .join(", "),
                    )
                } else {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    write!(
                        f,
                        "{}{} Wheels are available for `{}` ({}) with the following Python implementation tag{s}: {}",
                        "hint".bold().cyan(),
                        ":".bold(),
                        package.cyan(),
                        format!("v{version}").cyan(),
                        tags.iter()
                            .map(|tag| format!("`{}`", tag.cyan()))
                            .join(", "),
                    )
                }
            }
            Self::AbiTags {
                package,
                version,
                tags,
                best,
            } => {
                if let Some(best) = best {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    let best = if let Some(pretty) = best.pretty() {
                        format!("{} (`{}`)", pretty.cyan(), best.cyan())
                    } else {
                        format!("{}", best.cyan())
                    };
                    write!(
                        f,
                        "{}{} You require {}, but we only found wheels for `{}` ({}) with the following Python ABI tag{s}: {}",
                        "hint".bold().cyan(),
                        ":".bold(),
                        best,
                        package.cyan(),
                        format!("v{version}").cyan(),
                        tags.iter()
                            .map(|tag| format!("`{}`", tag.cyan()))
                            .join(", "),
                    )
                } else {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    write!(
                        f,
                        "{}{} Wheels are available for `{}` ({}) with the following Python ABI tag{s}: {}",
                        "hint".bold().cyan(),
                        ":".bold(),
                        package.cyan(),
                        format!("v{version}").cyan(),
                        tags.iter()
                            .map(|tag| format!("`{}`", tag.cyan()))
                            .join(", "),
                    )
                }
            }
            Self::PlatformTags {
                package,
                version,
                tags,
            } => {
                let s = if tags.len() == 1 { "" } else { "s" };
                write!(
                    f,
                    "{}{} Wheels are available for `{}` ({}) on the following platform{s}: {}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    format!("v{version}").cyan(),
                    tags.iter()
                        .map(|tag| format!("`{}`", tag.cyan()))
                        .join(", "),
                )
            }
        }
    }
}

/// A [`Term`] and [`PubGrubPackage`] combination for display.
struct PackageTerm<'a> {
    package: &'a PubGrubPackage,
    term: &'a Term<Range<Version>>,
    formatter: &'a PubGrubReportFormatter<'a>,
}

impl std::fmt::Display for PackageTerm<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.term {
            Term::Positive(set) => {
                write!(f, "{}", self.formatter.compatible_range(self.package, set))
            }
            Term::Negative(set) => {
                if let Some(version) = set.as_singleton() {
                    // Note we do not handle the "root" package here but we should never
                    // be displaying that the root package is inequal to some version
                    let package = self.package;
                    write!(f, "{package}!={version}")
                } else {
                    write!(
                        f,
                        "{}",
                        self.formatter
                            .compatible_range(self.package, &set.complement())
                    )
                }
            }
        }
    }
}

impl PackageTerm<'_> {
    /// Create a new [`PackageTerm`] from a [`PubGrubPackage`] and a [`Term`].
    fn new<'a>(
        package: &'a PubGrubPackage,
        term: &'a Term<Range<Version>>,
        formatter: &'a PubGrubReportFormatter<'a>,
    ) -> PackageTerm<'a> {
        PackageTerm {
            package,
            term,
            formatter,
        }
    }
}

/// The kind of version ranges being displayed in [`PackageRange`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageRangeKind {
    Dependency,
    Compatibility,
    Available,
}

/// A [`Range`] and [`PubGrubPackage`] combination for display.
#[derive(Debug)]
struct PackageRange<'a> {
    package: &'a PubGrubPackage,
    range: &'a Range<Version>,
    kind: PackageRangeKind,
    formatter: Option<&'a PubGrubReportFormatter<'a>>,
}

impl PackageRange<'_> {
    fn compatibility<'a>(
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
        formatter: Option<&'a PubGrubReportFormatter<'a>>,
    ) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Compatibility,
            formatter,
        }
    }

    fn dependency<'a>(
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
        formatter: Option<&'a PubGrubReportFormatter<'a>>,
    ) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Dependency,
            formatter,
        }
    }

    fn availability<'a>(
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
        formatter: Option<&'a PubGrubReportFormatter<'a>>,
    ) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Available,
            formatter,
        }
    }

    /// Returns a boolean indicating if the predicate following this package range should
    /// be singular or plural e.g. if false use "<range> depends on <...>" and
    /// if true use "<range> depend on <...>"
    fn plural(&self) -> bool {
        // If a workspace member, always use the singular form (otherwise, it'd be "all versions of")
        if self
            .formatter
            .and_then(|formatter| formatter.format_workspace_member(self.package))
            .is_some()
        {
            return false;
        }

        let mut segments = self.range.iter();
        if let Some(segment) = segments.next() {
            // A single unbounded compatibility segment is always plural ("all versions of").
            if self.kind == PackageRangeKind::Compatibility {
                if matches!(segment, (Bound::Unbounded, Bound::Unbounded)) {
                    return true;
                }
            }
            // Otherwise, multiple segments are always plural.
            segments.next().is_some()
        } else {
            // An empty range is always singular.
            false
        }
    }
}

/// Create a range with improved segments for reporting the available versions for a package.
fn update_availability_range(
    range: &Range<Version>,
    available_versions: &BTreeSet<Version>,
) -> Range<Version> {
    let mut new_range = Range::empty();

    // Construct an available range to help guide simplification. Note this is not strictly correct,
    // as the available range should have many holes in it. However, for this use-case it should be
    // okay — we just may avoid simplifying some segments _inside_ the available range.
    let (available_range, first_available, last_available) =
        match (available_versions.first(), available_versions.last()) {
            // At least one version is available
            (Some(first), Some(last)) => {
                let range = Range::<Version>::from_range_bounds((
                    Bound::Included(first.clone()),
                    Bound::Included(last.clone()),
                ));
                // If only one version is available, return this as the bound immediately
                if first == last {
                    return range;
                }
                (range, first, last)
            }
            // SAFETY: If there's only a single item, `first` and `last` should both
            // return `Some`.
            (Some(_), None) | (None, Some(_)) => unreachable!(),
            // No versions are available; nothing to do
            (None, None) => return Range::empty(),
        };

    for segment in range.iter() {
        let (lower, upper) = segment;
        let segment_range = Range::from_range_bounds((lower.clone(), upper.clone()));

        // Drop the segment if it's disjoint with the available range, e.g., if the segment is
        // `foo>999`, and the available versions are all `<10` it's useless to show.
        if segment_range.is_disjoint(&available_range) {
            continue;
        }

        // Replace the segment if it's captured by the available range, e.g., if the segment is
        // `foo<1000` and the available versions are all `<10` we can simplify to `foo<10`.
        if available_range.subset_of(&segment_range) {
            // If the segment only has a lower or upper bound, only take the relevant part of the
            // available range. This avoids replacing `foo<100` with `foo>1,<2`, instead using
            // `foo<2` to avoid extra noise.
            if matches!(lower, Bound::Unbounded) {
                new_range = new_range.union(&Range::from_range_bounds((
                    Bound::Unbounded,
                    Bound::Included(last_available.clone()),
                )));
            } else if matches!(upper, Bound::Unbounded) {
                new_range = new_range.union(&Range::from_range_bounds((
                    Bound::Included(first_available.clone()),
                    Bound::Unbounded,
                )));
            } else {
                new_range = new_range.union(&available_range);
            }
            continue;
        }

        // If the bound is inclusive, and the version is _not_ available, change it to an exclusive
        // bound to avoid confusion, e.g., if the segment is `foo<=10` and the available versions
        // do not include `foo 10`, we should instead say `foo<10`.
        let lower = match lower {
            Bound::Included(version) if !available_versions.contains(version) => {
                Bound::Excluded(version.clone())
            }
            _ => (*lower).clone(),
        };
        let upper = match upper {
            Bound::Included(version) if !available_versions.contains(version) => {
                Bound::Excluded(version.clone())
            }
            _ => (*upper).clone(),
        };

        // Note this repeated-union construction is not particularly efficient, but there's not
        // better API exposed by PubGrub. Since we're just generating an error message, it's
        // probably okay, but we should investigate a better upstream API.
        new_range = new_range.union(&Range::from_range_bounds((lower, upper)));
    }

    new_range
}

impl std::fmt::Display for PackageRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Exit early for the root package — the range is not meaningful
        if let Some(root) = self
            .formatter
            .and_then(|formatter| formatter.format_root(self.package))
        {
            return write!(f, "{root}");
        }
        // Exit early for workspace members, only a single version is available
        if let Some(member) = self
            .formatter
            .and_then(|formatter| formatter.format_workspace_member(self.package))
        {
            return write!(f, "{member}");
        }
        let package = self.package;

        if self.range.is_empty() {
            return write!(f, "{package} ∅");
        }

        let segments: Vec<_> = self.range.iter().collect();
        if segments.len() > 1 {
            match self.kind {
                PackageRangeKind::Dependency => write!(f, "one of:")?,
                PackageRangeKind::Compatibility => write!(f, "all of:")?,
                PackageRangeKind::Available => write!(f, "are available:")?,
            }
        }
        for segment in &segments {
            if segments.len() > 1 {
                write!(f, "\n    ")?;
            }
            match segment {
                (Bound::Unbounded, Bound::Unbounded) => match self.kind {
                    PackageRangeKind::Dependency => write!(f, "{package}")?,
                    PackageRangeKind::Compatibility => write!(f, "all versions of {package}")?,
                    PackageRangeKind::Available => write!(f, "{package}")?,
                },
                (Bound::Unbounded, Bound::Included(v)) => write!(f, "{package}<={v}")?,
                (Bound::Unbounded, Bound::Excluded(v)) => write!(f, "{package}<{v}")?,
                (Bound::Included(v), Bound::Unbounded) => write!(f, "{package}>={v}")?,
                (Bound::Included(v), Bound::Included(b)) => {
                    if v == b {
                        write!(f, "{package}=={v}")?;
                    } else {
                        write!(f, "{package}>={v},<={b}")?;
                    }
                }
                (Bound::Included(v), Bound::Excluded(b)) => write!(f, "{package}>={v},<{b}")?,
                (Bound::Excluded(v), Bound::Unbounded) => write!(f, "{package}>{v}")?,
                (Bound::Excluded(v), Bound::Included(b)) => write!(f, "{package}>{v},<={b}")?,
                (Bound::Excluded(v), Bound::Excluded(b)) => write!(f, "{package}>{v},<{b}")?,
            };
        }
        if segments.len() > 1 {
            writeln!(f)?;
        }
        Ok(())
    }
}

impl PackageRange<'_> {
    fn depends_on<'a>(
        &'a self,
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> DependsOn<'a> {
        DependsOn {
            package: self,
            dependency1: PackageRange {
                package,
                range,
                kind: PackageRangeKind::Dependency,
                formatter: self.formatter,
            },
            dependency2: None,
        }
    }
}

/// A representation of A depends on B (and C).
#[derive(Debug)]
struct DependsOn<'a> {
    package: &'a PackageRange<'a>,
    dependency1: PackageRange<'a>,
    dependency2: Option<PackageRange<'a>>,
}

impl<'a> DependsOn<'a> {
    /// Adds an additional dependency.
    ///
    /// Note this overwrites previous calls to `DependsOn::and`.
    fn and(mut self, package: &'a PubGrubPackage, range: &'a Range<Version>) -> DependsOn<'a> {
        self.dependency2 = Some(PackageRange {
            package,
            range,
            kind: PackageRangeKind::Dependency,
            formatter: self.package.formatter,
        });
        self
    }
}

impl std::fmt::Display for DependsOn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Padded::new("", self.package, " "))?;
        if self.package.plural() {
            write!(f, "depend on ")?;
        } else {
            write!(f, "depends on ")?;
        };

        match self.dependency2 {
            Some(ref dependency2) => write!(
                f,
                "{}and{}",
                Padded::new("", &self.dependency1, " "),
                Padded::new(" ", &dependency2, "")
            )?,
            None => write!(f, "{}", self.dependency1)?,
        }

        Ok(())
    }
}

/// Inserts the given padding on the left and right sides of the content if
/// the content does not start and end with whitespace respectively.
#[derive(Debug)]
struct Padded<'a, T: std::fmt::Display> {
    left: &'a str,
    content: &'a T,
    right: &'a str,
}

impl<'a, T: std::fmt::Display> Padded<'a, T> {
    fn new(left: &'a str, content: &'a T, right: &'a str) -> Self {
        Padded {
            left,
            content,
            right,
        }
    }
}

impl<'a> Padded<'a, String> {
    fn from_string(left: &'a str, content: &'a String, right: &'a str) -> Self {
        Padded {
            left,
            content,
            right,
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Padded<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        let content = self.content.to_string();

        if let Some(char) = content.chars().next() {
            if !char.is_whitespace() {
                result.push_str(self.left);
            }
        }

        result.push_str(&content);

        if let Some(char) = content.chars().last() {
            if !char.is_whitespace() {
                result.push_str(self.right);
            }
        }

        write!(f, "{result}")
    }
}
