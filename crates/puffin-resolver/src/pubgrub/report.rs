use std::borrow::Cow;
use std::ops::Bound;

use derivative::Derivative;
use owo_colors::OwoColorize;
use pubgrub::range::Range;
use pubgrub::report::{DerivationTree, External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::candidate_selector::CandidateSelector;
use crate::prerelease_mode::PreReleaseStrategy;

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
                let set = self.simplify_set(set, package);
                if set.as_ref() == &Range::full() {
                    format!("there are no versions of {package}")
                } else {
                    format!(
                        "there are no versions of {package}{}",
                        PubGrubRange::new(&set)
                    )
                }
            }
            External::UnavailableDependencies(package, set) => {
                let set = self.simplify_set(set, package);
                if set.as_ref() == &Range::full() {
                    format!("dependencies of {package} are unavailable")
                } else {
                    format!(
                        "dependencies of {package}{} are unavailable",
                        PubGrubRange::new(&set)
                    )
                }
            }
            External::UnusableDependencies(package, set, reason) => {
                if let Some(reason) = reason {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        format!("{package} dependencies are unusable: {reason}")
                    } else {
                        let set = self.simplify_set(set, package);
                        if set.as_ref() == &Range::full() {
                            format!("dependencies of {package} are unusable: {reason}")
                        } else {
                            format!(
                                "dependencies of {package}{} are unusable: {reason}",
                                PubGrubRange::new(&set)
                            )
                        }
                    }
                } else {
                    let set = self.simplify_set(set, package);
                    if set.as_ref() == &Range::full() {
                        format!("dependencies of {package} are unusable")
                    } else {
                        format!(
                            "dependencies of {package}{} are unusable",
                            PubGrubRange::new(&set)
                        )
                    }
                }
            }
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                let package_set = self.simplify_set(package_set, package);
                let dependency_set = self.simplify_set(dependency_set, dependency);
                if package_set.as_ref() == &Range::full()
                    && dependency_set.as_ref() == &Range::full()
                {
                    format!("{package} depends on {dependency}")
                } else if package_set.as_ref() == &Range::full() {
                    format!(
                        "{package} depends on {dependency}{}",
                        PubGrubRange::new(&dependency_set)
                    )
                } else if dependency_set.as_ref() == &Range::full() {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!("{package} depends on {dependency}")
                    } else {
                        format!(
                            "{package}{} depends on {dependency}",
                            PubGrubRange::new(&package_set)
                        )
                    }
                } else {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!(
                            "{package} depends on {dependency}{}",
                            PubGrubRange::new(&dependency_set)
                        )
                    } else {
                        format!(
                            "{package}{} depends on {dependency}{}",
                            PubGrubRange::new(&package_set),
                            PubGrubRange::new(&dependency_set)
                        )
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
                format!("{package}{} is forbidden", PubGrubRange::new(&range))
            }
            [(package @ PubGrubPackage::Package(..), Term::Negative(range))] => {
                let range = range.simplify(
                    self.available_versions
                        .get(package)
                        .unwrap_or(&vec![])
                        .iter(),
                );
                format!("{package}{} is mandatory", PubGrubRange::new(&range))
            }
            [(p1, Term::Positive(r1)), (p2, Term::Negative(r2))] => self.format_external(
                &External::FromDependencyOf((*p1).clone(), r1.clone(), (*p2).clone(), r2.clone()),
            ),
            [(p1, Term::Negative(r1)), (p2, Term::Positive(r2))] => self.format_external(
                &External::FromDependencyOf((*p2).clone(), r2.clone(), (*p1).clone(), r1.clone()),
            ),
            slice => {
                let str_terms: Vec<_> = slice
                    .iter()
                    .map(|(p, t)| format!("{p}{}", PubGrubTerm::from_term((*t).clone())))
                    .collect();
                str_terms.join(", ") + " are incompatible"
            }
        }
    }
}

impl PubGrubReportFormatter<'_> {
    /// Simplify a [`Range`] of versions using the available versions for a package.
    fn simplify_set<'a>(
        &self,
        set: &'a Range<PubGrubVersion>,
        package: &PubGrubPackage,
    ) -> Cow<'a, Range<PubGrubVersion>> {
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
        derivation_tree: &DerivationTree<PubGrubPackage, Range<PubGrubVersion>>,
        selector: &CandidateSelector,
    ) -> FxHashSet<PubGrubHint> {
        /// Returns `true` if pre-releases were allowed for a package.
        fn allowed_prerelease(package: &PubGrubPackage, selector: &CandidateSelector) -> bool {
            match selector.prerelease_strategy() {
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
            }
        }

        let mut hints = FxHashSet::default();
        match derivation_tree {
            DerivationTree::External(external) => match external {
                External::NoVersions(package, set) => {
                    if set.bounds().any(PubGrubVersion::any_prerelease) {
                        // A pre-release marker appeared in the version requirements.
                        if !allowed_prerelease(package, selector) {
                            hints.insert(PubGrubHint::PreReleaseRequested {
                                package: package.clone(),
                                range: self.simplify_set(set, package).into_owned(),
                            });
                        }
                    } else if let Some(version) =
                        self.available_versions.get(package).and_then(|versions| {
                            versions
                                .iter()
                                .rev()
                                .filter(|version| version.any_prerelease())
                                .find(|version| set.contains(version))
                        })
                    {
                        // There are pre-release versions available for the package.
                        if !allowed_prerelease(package, selector) {
                            hints.insert(PubGrubHint::PreReleaseAvailable {
                                package: package.clone(),
                                version: version.clone(),
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
                hints.extend(self.hints(&derived.cause1, selector));
                hints.extend(self.hints(&derived.cause2, selector));
            }
        }
        hints
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
        version: PubGrubVersion,
    },
    /// A requirement included a pre-release marker, but pre-releases weren't enabled for that
    /// package.
    PreReleaseRequested {
        package: PubGrubPackage,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        range: Range<PubGrubVersion>,
    },
}

impl std::fmt::Display for PubGrubHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PubGrubHint::PreReleaseAvailable { package, version } => {
                write!(
                    f,
                    "{}{} Pre-releases are available for {} in the requested range (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    version.bold()
                )
            }
            PubGrubHint::PreReleaseRequested { package, range } => {
                write!(
                    f,
                    "{}{} {} was requested with a pre-release marker (e.g., {}), but pre-releases weren't enabled (try: `--prerelease=allow`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.bold(),
                    PubGrubRange::new(range).bold()
                )
            }
        }
    }
}

/// A derivative of [Term] with custom formatting.
struct PubGrubTerm {
    inner: Term<Range<PubGrubVersion>>,
}

impl std::fmt::Display for PubGrubTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Term::Positive(set) => write!(f, "{set}"),
            Term::Negative(set) => {
                if let Some(version) = set.as_singleton() {
                    write!(f, "!={version}")
                } else {
                    write!(f, "!( {} )", PubGrubRange::new(set))
                }
            }
        }
    }
}

impl PubGrubTerm {
    fn from_term(term: Term<Range<PubGrubVersion>>) -> PubGrubTerm {
        PubGrubTerm { inner: term }
    }
}

/// A derivative of [Range] with custom formatting.
struct PubGrubRange<'a> {
    inner: &'a Range<PubGrubVersion>,
}

impl std::fmt::Display for PubGrubRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.inner.is_empty() {
            write!(f, "∅")?;
        } else {
            for (idx, segment) in self.inner.iter().enumerate() {
                if idx > 0 {
                    write!(f, " | ")?;
                }
                match segment {
                    (Bound::Unbounded, Bound::Unbounded) => write!(f, "*")?,
                    (Bound::Unbounded, Bound::Included(v)) => write!(f, "<={v}")?,
                    (Bound::Unbounded, Bound::Excluded(v)) => write!(f, "<{v}")?,
                    (Bound::Included(v), Bound::Unbounded) => write!(f, ">={v}")?,
                    (Bound::Included(v), Bound::Included(b)) => {
                        if v == b {
                            write!(f, "=={v}")?;
                        } else {
                            write!(f, ">={v}, <={b}")?;
                        }
                    }
                    (Bound::Included(v), Bound::Excluded(b)) => write!(f, ">={v}, <{b}")?,
                    (Bound::Excluded(v), Bound::Unbounded) => write!(f, ">{v}")?,
                    (Bound::Excluded(v), Bound::Included(b)) => write!(f, ">{v}, <={b}")?,
                    (Bound::Excluded(v), Bound::Excluded(b)) => write!(f, ">{v}, <{b}")?,
                };
            }
        }
        Ok(())
    }
}

impl PubGrubRange<'_> {
    fn new(range: &Range<PubGrubVersion>) -> PubGrubRange {
        PubGrubRange { inner: range }
    }
}
