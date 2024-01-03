use crate::candidate_selector::CandidateSelector;
use crate::prerelease_mode::PreReleaseStrategy;
use colored::Colorize;
use derivative::Derivative;
use pubgrub::range::Range;
use pubgrub::report::{DerivationTree, External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{PubGrubPackage, PubGrubVersion};

#[derive(Debug)]
pub(crate) struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package
    pub(crate) available_versions: &'a FxHashMap<PubGrubPackage, Vec<PubGrubVersion>>,
}

impl ReportFormatter<PubGrubPackage, Range<PubGrubVersion>> for PubGrubReportFormatter<'_> {
    type Output = String;

    fn format_external(
        &self,
        external: &External<PubGrubPackage, Range<PubGrubVersion>>,
    ) -> Self::Output {
        match external {
            External::NotRoot(package, version) => {
                format!("we are solving dependencies of {package} {version}")
            }
            External::NoVersions(package, set) => {
                if set == &Range::full() {
                    format!("there is no available version for {package}")
                } else {
                    let set = self.simplify_set(set, package);
                    format!("there is no version of {package} available matching {set}")
                }
            }
            External::UnavailableDependencies(package, set) => {
                if set == &Range::full() {
                    format!("dependencies of {package} are unavailable")
                } else {
                    let set = self.simplify_set(set, package);
                    format!("dependencies of {package} at version {set} are unavailable")
                }
            }
            External::UnusableDependencies(package, set, reason) => {
                if let Some(reason) = reason {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        format!("{package} dependencies are unusable: {reason}")
                    } else {
                        if set == &Range::full() {
                            format!("dependencies of {package} are unusable: {reason}")
                        } else {
                            let set = self.simplify_set(set, package);
                            format!("dependencies of {package}{set} are unusable: {reason}",)
                        }
                    }
                } else {
                    if set == &Range::full() {
                        format!("dependencies of {package} are unusable")
                    } else {
                        let set = self.simplify_set(set, package);
                        format!("dependencies of {package}{set} are unusable")
                    }
                }
            }
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                if package_set == &Range::full() && dependency_set == &Range::full() {
                    format!("{package} depends on {dependency}")
                } else if package_set == &Range::full() {
                    let dependency_set = self.simplify_set(dependency_set, dependency);
                    format!("{package} depends on {dependency}{dependency_set}")
                } else if dependency_set == &Range::full() {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!("{package} depends on {dependency}")
                    } else {
                        let package_set = self.simplify_set(package_set, package);
                        format!("{package}{package_set} depends on {dependency}")
                    }
                } else {
                    let dependency_set = self.simplify_set(dependency_set, dependency);
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!("{package} depends on {dependency}{dependency_set}")
                    } else {
                        let package_set = self.simplify_set(package_set, package);
                        format!("{package}{package_set} depends on {dependency}{dependency_set}")
                    }
                }
            }
        }
    }

    /// Try to print terms of an incompatibility in a human-readable way.
    fn format_terms(&self, terms: &Map<PubGrubPackage, Term<Range<PubGrubVersion>>>) -> String {
        let terms_vec: Vec<_> = terms.iter().collect();
        match terms_vec.as_slice() {
            [] | [(PubGrubPackage::Root(_), _)] => "version solving failed".into(),
            [(package @ PubGrubPackage::Package(..), Term::Positive(range))] => {
                let range = range.simplify(
                    self.available_versions
                        .get(package)
                        .unwrap_or(&vec![])
                        .iter(),
                );
                format!("{package}{range} is forbidden")
            }
            [(package @ PubGrubPackage::Package(..), Term::Negative(range))] => {
                let range = range.simplify(
                    self.available_versions
                        .get(package)
                        .unwrap_or(&vec![])
                        .iter(),
                );
                format!("{package}{range} is mandatory")
            }
            [(p1, Term::Positive(r1)), (p2, Term::Negative(r2))] => self.format_external(
                &External::FromDependencyOf((*p1).clone(), r1.clone(), (*p2).clone(), r2.clone()),
            ),
            [(p1, Term::Negative(r1)), (p2, Term::Positive(r2))] => self.format_external(
                &External::FromDependencyOf((*p2).clone(), r2.clone(), (*p1).clone(), r1.clone()),
            ),
            slice => {
                let str_terms: Vec<_> = slice.iter().map(|(p, t)| format!("{p} {t}")).collect();
                str_terms.join(", ") + " are incompatible"
            }
        }
    }
}

impl PubGrubReportFormatter<'_> {
    fn simplify_set(
        &self,
        set: &Range<PubGrubVersion>,
        package: &PubGrubPackage,
    ) -> Range<PubGrubVersion> {
        set.simplify(
            self.available_versions
                .get(package)
                .unwrap_or(&vec![])
                .iter(),
        )
    }
}

/// A set of hints to help users resolve errors by providing additional context or modifying
/// their requirements.
#[derive(Debug, Default)]
pub(crate) struct PubGrubHints(FxHashSet<PubGrubHint>);

impl PubGrubHints {
    /// Create a set of hints from a derivation tree.
    pub(crate) fn from_derivation_tree(
        derivation_tree: &DerivationTree<PubGrubPackage, Range<PubGrubVersion>>,
        selector: &CandidateSelector,
    ) -> Self {
        let mut hints = Self::default();
        match derivation_tree {
            DerivationTree::External(external) => match external {
                External::NoVersions(package, set) => {
                    // Determine whether a pre-release marker appeared in the version requirements.
                    if set.bounds().any(PubGrubVersion::any_prerelease) {
                        // Determine whether pre-releases were allowed for this package.
                        let allowed_prerelease = match selector.prerelease_strategy() {
                            PreReleaseStrategy::Disallow => false,
                            PreReleaseStrategy::Allow => true,
                            PreReleaseStrategy::IfNecessary => false,
                            PreReleaseStrategy::Explicit(packages) => {
                                if let PubGrubPackage::Package(package, ..) = package {
                                    packages.contains(package)
                                } else {
                                    false
                                }
                            }
                            PreReleaseStrategy::IfNecessaryOrExplicit(packages) => {
                                if let PubGrubPackage::Package(package, ..) = package {
                                    packages.contains(package)
                                } else {
                                    false
                                }
                            }
                        };

                        if !allowed_prerelease {
                            hints.insert(PubGrubHint::NoVersionsWithPreRelease {
                                package: package.clone(),
                                range: set.clone(),
                            });
                        }
                    }
                }
                External::NotRoot(..) => {}
                External::UnavailableDependencies(..) => {}
                External::UnusableDependencies(..) => {}
                External::FromDependencyOf(..) => {}
            },
            DerivationTree::Derived(derived) => {
                hints.extend(Self::from_derivation_tree(&derived.cause1, selector));
                hints.extend(Self::from_derivation_tree(&derived.cause2, selector));
            }
        }
        hints
    }

    /// Iterate over the hints in the set.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &PubGrubHint> {
        self.0.iter()
    }

    /// Insert a hint into the set.
    fn insert(&mut self, hint: PubGrubHint) -> bool {
        self.0.insert(hint)
    }

    /// Extend the set with another set of hints.
    fn extend(&mut self, hints: Self) {
        self.0.extend(hints.0);
    }
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Hash, PartialEq, Eq)]
pub(crate) enum PubGrubHint {
    /// A package was requested with a pre-release marker, but pre-releases weren't enabled for
    /// that package.
    NoVersionsWithPreRelease {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        range: Range<PubGrubVersion>,
    },
}

impl std::fmt::Display for PubGrubHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PubGrubHint::NoVersionsWithPreRelease { package, range } => {
                write!(
                    f,
                    "{}{} {} was requested with a pre-release marker (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    format!("{package}").bold(),
                    format!("{range}").bold(),
                )
            }
        }
    }
}
