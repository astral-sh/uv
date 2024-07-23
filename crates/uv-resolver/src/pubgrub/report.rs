use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use derivative::Derivative;
use indexmap::IndexSet;
use owo_colors::OwoColorize;
use pubgrub::range::Range;
use pubgrub::report::{DerivationTree, Derived, External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;
use rustc_hash::FxHashMap;

use distribution_types::IndexLocations;
use pep440_rs::Version;
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::fork_urls::ForkUrls;
use crate::prerelease_mode::AllowPreRelease;
use crate::python_requirement::{PythonRequirement, PythonTarget};
use crate::resolver::{IncompletePackage, UnavailablePackage, UnavailableReason};
use crate::{RequiresPython, ResolverMarkers};

use super::{PubGrubPackage, PubGrubPackageInner, PubGrubPython};

#[derive(Debug)]
pub(crate) struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package
    pub(crate) available_versions: &'a FxHashMap<PubGrubPackage, BTreeSet<Version>>,

    /// The versions that were available for each package
    pub(crate) python_requirement: &'a PythonRequirement,
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
                    return if let Some(target) = self.python_requirement.target() {
                        format!(
                            "the requested {package} version ({target}) does not satisfy {}",
                            PackageRange::compatibility(package, set)
                        )
                    } else {
                        format!(
                            "the requested {package} version does not satisfy {}",
                            PackageRange::compatibility(package, set)
                        )
                    };
                }
                if matches!(
                    &**package,
                    PubGrubPackageInner::Python(PubGrubPython::Installed)
                ) {
                    return format!(
                        "the current {package} version ({}) does not satisfy {}",
                        self.python_requirement.installed(),
                        PackageRange::compatibility(package, set)
                    );
                }

                let set = self.simplify_set(set, package);

                if set.as_ref() == &Range::full() {
                    format!("there are no versions of {package}")
                } else if set.as_singleton().is_some() {
                    format!("there is no version of {package}{set}")
                } else {
                    let complement = set.complement();
                    let segments = complement.iter().count();
                    // Simple case, there's a single range to report
                    if segments == 1 {
                        format!(
                            "only {} is available",
                            PackageRange::compatibility(package, &complement)
                        )
                    // Complex case, there are multiple ranges
                    } else {
                        format!(
                            "only the following versions of {} {}",
                            package,
                            PackageRange::available(package, &complement)
                        )
                    }
                }
            }
            External::Custom(package, set, reason) => match &**package {
                PubGrubPackageInner::Root(Some(name)) => {
                    format!("{name} cannot be used because {reason}")
                }
                PubGrubPackageInner::Root(None) => {
                    format!("your requirements cannot be used because {reason}")
                }
                _ => match reason {
                    UnavailableReason::Package(reason) => {
                        // While there may be a term attached, this error applies to the entire
                        // package, so we show it for the entire package
                        format!("{}{reason}", Padded::new("", &package, " "))
                    }
                    UnavailableReason::Version(reason) => {
                        format!(
                            "{}{reason}",
                            Padded::new("", &PackageRange::compatibility(package, set), " ")
                        )
                    }
                },
            },
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                let package_set = self.simplify_set(package_set, package);
                let dependency_set = self.simplify_set(dependency_set, dependency);
                match &**package {
                    PubGrubPackageInner::Root(Some(name)) => format!(
                        "{name} depends on {}",
                        PackageRange::dependency(dependency, &dependency_set)
                    ),
                    PubGrubPackageInner::Root(None) => format!(
                        "you require {}",
                        PackageRange::dependency(dependency, &dependency_set)
                    ),
                    _ => format!(
                        "{}",
                        PackageRange::compatibility(package, &package_set)
                            .depends_on(dependency, &dependency_set),
                    ),
                }
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
                "the requirements are unsatisfiable".into()
            }
            [(package, Term::Positive(range))]
                if matches!(&**(*package), PubGrubPackageInner::Package { .. }) =>
            {
                let range = self.simplify_set(range, package);
                format!(
                    "{} cannot be used",
                    PackageRange::compatibility(package, &range)
                )
            }
            [(package, Term::Negative(range))]
                if matches!(&**(*package), PubGrubPackageInner::Package { .. }) =>
            {
                let range = self.simplify_set(range, package);
                format!(
                    "{} must be used",
                    PackageRange::compatibility(package, &range)
                )
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
                    .map(|(p, t)| format!("{}", PackageTerm::new(p, t)))
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
                    if PackageTerm::new(p, t).plural() {
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
                let dependency1 = PackageRange::dependency(dependency1, &dependency_set1);

                let dependency_set2 = self.simplify_set(dependency_set2, dependency2);
                let dependency2 = PackageRange::dependency(dependency2, &dependency_set2);

                match &**package1 {
                    PubGrubPackageInner::Root(Some(name)) => format!(
                        "{name} depends on {}and {}",
                        Padded::new("", &dependency1, " "),
                        dependency2,
                    ),
                    PubGrubPackageInner::Root(None) => format!(
                        "you require {}and {}",
                        Padded::new("", &dependency1, " "),
                        dependency2,
                    ),
                    _ => {
                        let package_set = self.simplify_set(package_set1, package1);

                        format!(
                            "{}",
                            PackageRange::compatibility(package1, &package_set)
                                .depends_on(dependency1.package, &dependency_set1)
                                .and(dependency2.package, &dependency_set2),
                        )
                    }
                }
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
        if set == &Range::full() {
            Cow::Borrowed(set)
        } else {
            Cow::Owned(set.simplify(self.available_versions.get(package).into_iter().flatten()))
        }
    }

    /// Generate the [`PubGrubHints`] for a derivation tree.
    ///
    /// The [`PubGrubHints`] help users resolve errors by providing additional context or modifying
    /// their requirements.
    pub(crate) fn hints(
        &self,
        derivation_tree: &DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
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
                    if let Some(PythonTarget::RequiresPython(requires_python)) =
                        self.python_requirement.target()
                    {
                        hints.insert(PubGrubHint::RequiresPython {
                            requires_python: requires_python.clone(),
                            package: package.clone(),
                            package_set: self.simplify_set(package_set, package).into_owned(),
                            package_requires_python: dependency_set.clone(),
                        });
                    }
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
            if selector.prerelease_strategy().allows(name, markers) != AllowPreRelease::Yes {
                hints.insert(PubGrubHint::PreReleaseRequested {
                    package: package.clone(),
                    range: self.simplify_set(set, package).into_owned(),
                });
            }
        } else if let Some(version) = self.available_versions.get(package).and_then(|versions| {
            versions
                .iter()
                .rev()
                .filter(|version| version.any_prerelease())
                .find(|version| set.contains(version))
        }) {
            // There are pre-release versions available for the package.
            if selector.prerelease_strategy().allows(name, markers) != AllowPreRelease::Yes {
                hints.insert(PubGrubHint::PreReleaseAvailable {
                    package: package.clone(),
                    version: version.clone(),
                });
            }
        }
    }
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Hash, PartialEq, Eq)]
pub(crate) enum PubGrubHint {
    /// There are pre-release versions available for a package, but pre-releases weren't enabled
    /// for that package.
    ///
    PreReleaseAvailable {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        version: Version,
    },
    /// A requirement included a pre-release marker, but pre-releases weren't enabled for that
    /// package.
    PreReleaseRequested {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
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
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        reason: String,
    },
    /// The structure of a package was invalid (e.g., multiple `.dist-info` directories).
    InvalidPackageStructure {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        reason: String,
    },
    /// Metadata for a package version could not be found.
    MissingVersionMetadata {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        version: Version,
    },
    /// Metadata for a package version could not be parsed.
    InvalidVersionMetadata {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        version: Version,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        reason: String,
    },
    /// Metadata for a package version was inconsistent (e.g., the package name did not match that
    /// of the file).
    InconsistentVersionMetadata {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        version: Version,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        reason: String,
    },
    /// The structure of a package version was invalid (e.g., multiple `.dist-info` directories).
    InvalidVersionStructure {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        version: Version,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        reason: String,
    },
    /// The `Requires-Python` requirement was not satisfied.
    RequiresPython {
        requires_python: RequiresPython,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        package_set: Range<Version>,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        package_requires_python: Range<Version>,
    },
}

impl std::fmt::Display for PubGrubHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreReleaseAvailable { package, version } => {
                write!(
                    f,
                    "{}{} Pre-releases are available for {} in the requested range (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold()
                )
            }
            Self::PreReleaseRequested { package, range } => {
                write!(
                    f,
                    "{}{} {} was requested with a pre-release marker (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    PackageRange::compatibility(package, range).bold()
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
                    "{}{} Packages were unavailable because the network was disabled",
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
                    PackageRange::compatibility(package, package_set).bold(),
                    package_requires_python.bold(),
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
}

impl std::fmt::Display for PackageTerm<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.term {
            Term::Positive(set) => write!(f, "{}", PackageRange::compatibility(self.package, set)),
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
                        PackageRange::compatibility(self.package, &set.complement())
                    )
                }
            }
        }
    }
}

impl PackageTerm<'_> {
    /// Create a new [`PackageTerm`] from a [`PubGrubPackage`] and a [`Term`].
    fn new<'a>(package: &'a PubGrubPackage, term: &'a Term<Range<Version>>) -> PackageTerm<'a> {
        PackageTerm { package, term }
    }

    /// Returns `true` if the predicate following this package term should be singular or plural.
    fn plural(&self) -> bool {
        match self.term {
            Term::Positive(set) => PackageRange::compatibility(self.package, set).plural(),
            Term::Negative(set) => {
                if set.as_singleton().is_some() {
                    false
                } else {
                    PackageRange::compatibility(self.package, &set.complement()).plural()
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
}

impl PackageRange<'_> {
    /// Returns a boolean indicating if the predicate following this package range should
    /// be singular or plural e.g. if false use "<range> depends on <...>" and
    /// if true use "<range> depend on <...>"
    fn plural(&self) -> bool {
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

impl std::fmt::Display for PackageRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Exit early for the root package — the range is not meaningful
        let package = match &**self.package {
            PubGrubPackageInner::Root(Some(name)) => return write!(f, "{name}"),
            PubGrubPackageInner::Root(None) => return write!(f, "your requirements"),
            _ => self.package,
        };

        if self.range.is_empty() {
            return write!(f, "{package} ∅");
        }

        let segments: Vec<_> = self.range.iter().collect();
        if segments.len() > 1 {
            match self.kind {
                PackageRangeKind::Dependency => write!(f, "one of:")?,
                PackageRangeKind::Compatibility => write!(f, "any of:")?,
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
    fn compatibility<'a>(
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Compatibility,
        }
    }

    fn dependency<'a>(package: &'a PubGrubPackage, range: &'a Range<Version>) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Dependency,
        }
    }

    fn available<'a>(package: &'a PubGrubPackage, range: &'a Range<Version>) -> PackageRange<'a> {
        PackageRange {
            package,
            range,
            kind: PackageRangeKind::Available,
        }
    }

    fn depends_on<'a>(
        &'a self,
        package: &'a PubGrubPackage,
        range: &'a Range<Version>,
    ) -> DependsOn<'a> {
        DependsOn {
            package: self,
            dependency1: PackageRange::dependency(package, range),
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
        self.dependency2 = Some(PackageRange::dependency(package, range));
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
