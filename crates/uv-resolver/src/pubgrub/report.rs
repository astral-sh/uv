use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use indexmap::IndexSet;
use owo_colors::OwoColorize;
use pubgrub::{DerivationTree, Derived, External, Map, Range, ReportFormatter, Term};
use rustc_hash::FxHashMap;

use distribution_types::IndexLocations;
use pep440_rs::Version;
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::error::ErrorTree;
use crate::fork_urls::ForkUrls;
use crate::prerelease::AllowPrerelease;
use crate::python_requirement::{PythonRequirement, PythonRequirementSource};
use crate::resolver::{IncompletePackage, UnavailablePackage, UnavailableReason};
use crate::{RequiresPython, ResolverMarkers};

use super::{PubGrubPackage, PubGrubPackageInner, PubGrubPython};

#[derive(Debug)]
pub(crate) struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package
    pub(crate) available_versions: &'a FxHashMap<PackageName, BTreeSet<Version>>,

    /// The versions that were available for each package
    pub(crate) python_requirement: &'a PythonRequirement,

    pub(crate) workspace_members: &'a BTreeSet<PackageName>,
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

                let set = self.simplify_set(set, package);

                if set.as_ref() == &Range::full() {
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
                            // While there may be a term attached, this error applies to the entire
                            // package, so we show it for the entire package
                            format!(
                                "{}{}",
                                Padded::new("", &package, " "),
                                reason.singular_message()
                            )
                        }
                        UnavailableReason::Version(reason) => {
                            let set = self.simplify_set(set, package);
                            let range = self.compatible_range(package, &set);
                            let reason = if range.plural() {
                                reason.plural_message()
                            } else {
                                reason.singular_message()
                            };
                            format!("{}{reason}", Padded::new("", &range, " "))
                        }
                    }
                }
            }
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                let package_set = self.simplify_set(package_set, package);
                let dependency_set = self.simplify_set(dependency_set, dependency);
                if let Some(root) = self.format_root_requires(package) {
                    return format!(
                        "{root} {}",
                        self.dependency_range(dependency, &dependency_set)
                    );
                }
                format!(
                    "{}",
                    self.compatible_range(package, &package_set)
                        .depends_on(dependency, &dependency_set),
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
                let range = self.simplify_set(range, package);
                if let Some(member) = self.format_workspace_member(package) {
                    format!("{member}'s requirements are unsatisfiable")
                } else {
                    format!("{} cannot be used", self.compatible_range(package, &range))
                }
            }
            [(package, Term::Negative(range))]
                if matches!(&**(*package), PubGrubPackageInner::Package { .. }) =>
            {
                let range = self.simplify_set(range, package);
                format!("{} must be used", self.compatible_range(package, &range))
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
                if let [(p, t)] = slice {
                    if PackageTerm::new(p, t, self).plural() {
                        result.push_str(" are incompatible");
                    } else {
                        result.push_str(" is incompatible");
                    }
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
                    return Some("your projects's requirements".to_string());
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
                let dependency_set1 = self.simplify_set(dependency_set1, dependency1);
                let dependency1 = self.dependency_range(dependency1, &dependency_set1);

                let dependency_set2 = self.simplify_set(dependency_set2, dependency2);
                let dependency2 = self.dependency_range(dependency2, &dependency_set2);

                if let Some(root) = self.format_root_requires(package1) {
                    return format!(
                        "{root} {}and {}",
                        Padded::new("", &dependency1, " "),
                        dependency2,
                    );
                }
                let package_set = self.simplify_set(package_set1, package1);

                format!(
                    "{}",
                    self.compatible_range(package1, &package_set)
                        .depends_on(dependency1.package, &dependency_set1)
                        .and(dependency2.package, &dependency_set2),
                )
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

    /// Simplify a [`Range`] of versions using the available versions for a package.
    fn simplify_set<'a>(
        &self,
        set: &'a Range<Version>,
        package: &PubGrubPackage,
    ) -> Cow<'a, Range<Version>> {
        let Some(name) = package.name() else {
            return Cow::Borrowed(set);
        };
        if set == &Range::full() {
            Cow::Borrowed(set)
        } else {
            Cow::Owned(set.simplify(self.available_versions.get(name).into_iter().flatten()))
        }
    }

    /// Generate the [`PubGrubHints`] for a derivation tree.
    ///
    /// The [`PubGrubHints`] help users resolve errors by providing additional context or modifying
    /// their requirements.
    pub(crate) fn hints(
        &self,
        derivation_tree: &ErrorTree,
        selector: &CandidateSelector,
        index_locations: &IndexLocations,
        unavailable_packages: &FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: &FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
        fork_urls: &ForkUrls,
        markers: &ResolverMarkers,
    ) -> IndexSet<PubGrubHint> {
        let mut hints = IndexSet::default();
        match derivation_tree {
            DerivationTree::External(
                External::Custom(package, set, _) | External::NoVersions(package, set),
            ) => {
                if let PubGrubPackageInner::Package { name, .. } = &**package {
                    // Check for no versions due to pre-release options.
                    if !fork_urls.contains_key(name) {
                        self.prerelease_available_hint(
                            package, name, set, selector, markers, &mut hints,
                        );
                    }
                }

                if let PubGrubPackageInner::Package { name, .. } = &**package {
                    // Check for no versions due to no `--find-links` flat index
                    Self::index_hints(
                        package,
                        name,
                        set,
                        index_locations,
                        unavailable_packages,
                        incomplete_packages,
                        &mut hints,
                    );
                }
            }
            DerivationTree::External(External::FromDependencyOf(
                package,
                package_set,
                dependency,
                dependency_set,
            )) => {
                // Check for no versions due to `Requires-Python`.
                if matches!(
                    &**dependency,
                    PubGrubPackageInner::Python(PubGrubPython::Target)
                ) {
                    hints.insert(PubGrubHint::RequiresPython {
                        source: self.python_requirement.source(),
                        requires_python: self.python_requirement.target().clone(),
                        package: package.clone(),
                        package_set: self.simplify_set(package_set, package).into_owned(),
                        package_requires_python: dependency_set.clone(),
                    });
                }
            }
            DerivationTree::External(External::NotRoot(..)) => {}
            DerivationTree::Derived(derived) => {
                hints.extend(self.hints(
                    &derived.cause1,
                    selector,
                    index_locations,
                    unavailable_packages,
                    incomplete_packages,
                    fork_urls,
                    markers,
                ));
                hints.extend(self.hints(
                    &derived.cause2,
                    selector,
                    index_locations,
                    unavailable_packages,
                    incomplete_packages,
                    fork_urls,
                    markers,
                ));
            }
        }
        hints
    }

    fn index_hints(
        package: &PubGrubPackage,
        name: &PackageName,
        set: &Range<Version>,
        index_locations: &IndexLocations,
        unavailable_packages: &FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: &FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
        hints: &mut IndexSet<PubGrubHint>,
    ) {
        let no_find_links = index_locations.flat_index().peekable().peek().is_none();

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
            Some(UnavailablePackage::MissingMetadata) => {
                hints.insert(PubGrubHint::MissingPackageMetadata {
                    package: package.clone(),
                });
            }
            Some(UnavailablePackage::InvalidMetadata(reason)) => {
                hints.insert(PubGrubHint::InvalidPackageMetadata {
                    package: package.clone(),
                    reason: reason.clone(),
                });
            }
            Some(UnavailablePackage::InvalidStructure(reason)) => {
                hints.insert(PubGrubHint::InvalidPackageStructure {
                    package: package.clone(),
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
                        IncompletePackage::Offline => {
                            hints.insert(PubGrubHint::Offline);
                        }
                        IncompletePackage::MissingMetadata => {
                            hints.insert(PubGrubHint::MissingVersionMetadata {
                                package: package.clone(),
                                version: version.clone(),
                            });
                        }
                        IncompletePackage::InvalidMetadata(reason) => {
                            hints.insert(PubGrubHint::InvalidVersionMetadata {
                                package: package.clone(),
                                version: version.clone(),
                                reason: reason.clone(),
                            });
                        }
                        IncompletePackage::InconsistentMetadata(reason) => {
                            hints.insert(PubGrubHint::InconsistentVersionMetadata {
                                package: package.clone(),
                                version: version.clone(),
                                reason: reason.clone(),
                            });
                        }
                        IncompletePackage::InvalidStructure(reason) => {
                            hints.insert(PubGrubHint::InvalidVersionStructure {
                                package: package.clone(),
                                version: version.clone(),
                                reason: reason.clone(),
                            });
                        }
                    }
                    break;
                }
            }
        }
    }

    fn prerelease_available_hint(
        &self,
        package: &PubGrubPackage,
        name: &PackageName,
        set: &Range<Version>,
        selector: &CandidateSelector,
        markers: &ResolverMarkers,
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
            if selector.prerelease_strategy().allows(name, markers) != AllowPrerelease::Yes {
                hints.insert(PubGrubHint::PrereleaseRequested {
                    package: package.clone(),
                    range: self.simplify_set(set, package).into_owned(),
                });
            }
        } else if let Some(version) = package
            .name()
            .and_then(|name| self.available_versions.get(name))
            .and_then(|versions| {
                versions
                    .iter()
                    .rev()
                    .filter(|version| version.any_prerelease())
                    .find(|version| set.contains(version))
            })
        {
            // There are pre-release versions available for the package.
            if selector.prerelease_strategy().allows(name, markers) != AllowPrerelease::Yes {
                hints.insert(PubGrubHint::PrereleaseAvailable {
                    package: package.clone(),
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
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        version: Version,
    },
    /// A requirement included a pre-release marker, but pre-releases weren't enabled for that
    /// package.
    PrereleaseRequested {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        range: Range<Version>,
    },
    /// Requirements were unavailable due to lookups in the index being disabled and no extra
    /// index was provided via `--find-links`
    NoIndex,
    /// A package was not found in the registry, but network access was disabled.
    Offline,
    /// Metadata for a package could not be found.
    MissingPackageMetadata { package: PubGrubPackage },
    /// Metadata for a package could not be parsed.
    InvalidPackageMetadata {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The structure of a package was invalid (e.g., multiple `.dist-info` directories).
    InvalidPackageStructure {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// Metadata for a package version could not be found.
    MissingVersionMetadata {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        version: Version,
    },
    /// Metadata for a package version could not be parsed.
    InvalidVersionMetadata {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// Metadata for a package version was inconsistent (e.g., the package name did not match that
    /// of the file).
    InconsistentVersionMetadata {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The structure of a package version was invalid (e.g., multiple `.dist-info` directories).
    InvalidVersionStructure {
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        version: Version,
        // excluded from `PartialEq` and `Hash`
        reason: String,
    },
    /// The `Requires-Python` requirement was not satisfied.
    RequiresPython {
        source: PythonRequirementSource,
        requires_python: RequiresPython,
        // excluded from `PartialEq` and `Hash`
        package: PubGrubPackage,
        // excluded from `PartialEq` and `Hash`
        package_set: Range<Version>,
        // excluded from `PartialEq` and `Hash`
        package_requires_python: Range<Version>,
    },
}

/// This private enum mirrors [`PubGrubHint`] but only includes fields that should be
/// used for `Eq` and `Hash` implementations. It is used to derive `PartialEq` and
/// `Hash` implementations for [`PubGrubHint`].
#[derive(PartialEq, Eq, Hash)]
enum PubGrubHintCore {
    PrereleaseAvailable {
        package: PubGrubPackage,
    },
    PrereleaseRequested {
        package: PubGrubPackage,
    },
    NoIndex,
    Offline,
    MissingPackageMetadata {
        package: PubGrubPackage,
    },
    InvalidPackageMetadata {
        package: PubGrubPackage,
    },
    InvalidPackageStructure {
        package: PubGrubPackage,
    },
    MissingVersionMetadata {
        package: PubGrubPackage,
    },
    InvalidVersionMetadata {
        package: PubGrubPackage,
    },
    InconsistentVersionMetadata {
        package: PubGrubPackage,
    },
    InvalidVersionStructure {
        package: PubGrubPackage,
    },
    RequiresPython {
        source: PythonRequirementSource,
        requires_python: RequiresPython,
    },
}

impl From<PubGrubHint> for PubGrubHintCore {
    #[inline]
    fn from(hint: PubGrubHint) -> Self {
        match hint {
            PubGrubHint::PrereleaseAvailable { package, .. } => {
                Self::PrereleaseAvailable { package }
            }
            PubGrubHint::PrereleaseRequested { package, .. } => {
                Self::PrereleaseRequested { package }
            }
            PubGrubHint::NoIndex => Self::NoIndex,
            PubGrubHint::Offline => Self::Offline,
            PubGrubHint::MissingPackageMetadata { package, .. } => {
                Self::MissingPackageMetadata { package }
            }
            PubGrubHint::InvalidPackageMetadata { package, .. } => {
                Self::InvalidPackageMetadata { package }
            }
            PubGrubHint::InvalidPackageStructure { package, .. } => {
                Self::InvalidPackageStructure { package }
            }
            PubGrubHint::MissingVersionMetadata { package, .. } => {
                Self::MissingVersionMetadata { package }
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
            PubGrubHint::RequiresPython {
                source,
                requires_python,
                ..
            } => Self::RequiresPython {
                source,
                requires_python,
            },
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
                    "{}{} Pre-releases are available for {} in the requested range (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold()
                )
            }
            Self::PrereleaseRequested { package, range } => {
                write!(
                    f,
                    "{}{} {} was requested with a pre-release marker (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    PackageRange::compatibility(package, range, None).bold()
                )
            }
            Self::NoIndex => {
                write!(
                    f,
                    "{}{} Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)",
                    "hint".bold().cyan(),
                    ":".bold(),
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
            Self::MissingPackageMetadata { package } => {
                write!(
                    f,
                    "{}{} Metadata for {} could not be found, as the wheel is missing a `METADATA` file",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold()
                )
            }
            Self::InvalidPackageMetadata { package, reason } => {
                write!(
                    f,
                    "{}{} Metadata for {} could not be parsed:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::InvalidPackageStructure { package, reason } => {
                write!(
                    f,
                    "{}{} The structure of {} was invalid:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::MissingVersionMetadata { package, version } => {
                write!(
                    f,
                    "{}{} Metadata for {}=={} could not be found, as the wheel is missing a `METADATA` file",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold(),
                )
            }
            Self::InvalidVersionMetadata {
                package,
                version,
                reason,
            } => {
                write!(
                    f,
                    "{}{} Metadata for {}=={} could not be parsed:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold(),
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
                    "{}{} The structure of {}=={} was invalid:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold(),
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
                    "{}{} Metadata for {}=={} was inconsistent:\n{}",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold(),
                    textwrap::indent(reason, "  ")
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::RequiresPython,
                requires_python,
                package,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The `requires-python` value ({}) includes Python versions that are not supported by your dependencies (e.g., {} only supports {}). Consider using a more restrictive `requires-python` value (like {}).",
                    "hint".bold().cyan(),
                    ":".bold(),
                    requires_python.bold(),
                    PackageRange::compatibility(package, package_set, None).bold(),
                    package_requires_python.bold(),
                    package_requires_python.bold(),
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::PythonVersion,
                requires_python,
                package,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The `--python-version` value ({}) includes Python versions that are not supported by your dependencies (e.g., {} only supports {}). Consider using a higher `--python-version` value.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    requires_python.bold(),
                    PackageRange::compatibility(package, package_set, None).bold(),
                    package_requires_python.bold(),
                )
            }
            Self::RequiresPython {
                source: PythonRequirementSource::Interpreter,
                requires_python: _,
                package,
                package_set,
                package_requires_python,
            } => {
                write!(
                    f,
                    "{}{} The Python interpreter uses a Python version that is not supported by your dependencies (e.g., {} only supports {}). Consider passing a `--python-version` value to raise the minimum supported version.",
                    "hint".bold().cyan(),
                    ":".bold(),
                    PackageRange::compatibility(package, package_set, None).bold(),
                    package_requires_python.bold(),
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

    /// Returns `true` if the predicate following this package term should be singular or plural.
    fn plural(&self) -> bool {
        match self.term {
            Term::Positive(set) => self.formatter.compatible_range(self.package, set).plural(),
            Term::Negative(set) => {
                if set.as_singleton().is_some() {
                    false
                } else {
                    self.formatter
                        .compatible_range(self.package, &set.complement())
                        .plural()
                }
            }
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
        // `foo>999`, and the the available versions are all `<10` it's useless to show.
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
