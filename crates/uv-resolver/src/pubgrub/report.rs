use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Bound;

use derivative::Derivative;
use distribution_types::IndexLocations;
use indexmap::{IndexMap, IndexSet};
use owo_colors::OwoColorize;
use pep440_rs::Version;
use pubgrub::range::Range;
use pubgrub::report::{DerivationTree, Derived, External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;
use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::prerelease_mode::PreReleaseStrategy;
use crate::python_requirement::PythonRequirement;
use crate::resolver::UnavailablePackage;

use super::PubGrubPackage;

#[derive(Debug)]
pub(crate) struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package
    pub(crate) available_versions: &'a IndexMap<PubGrubPackage, BTreeSet<Version>>,

    /// The versions that were available for each package
    pub(crate) python_requirement: Option<&'a PythonRequirement>,
}

impl ReportFormatter<PubGrubPackage, Range<Version>> for PubGrubReportFormatter<'_> {
    type Output = String;

    fn format_external(&self, external: &External<PubGrubPackage, Range<Version>>) -> Self::Output {
        match external {
            External::NotRoot(package, version) => {
                format!("we are solving dependencies of {package} {version}")
            }
            External::NoVersions(package, set, reason) => {
                if matches!(package, PubGrubPackage::Python(_)) {
                    if let Some(python) = self.python_requirement {
                        if python.target() == python.installed() {
                            // Simple case, the installed version is the same as the target version
                            return format!(
                                "the current {package} version ({}) does not satisfy {}",
                                python.target(),
                                PackageRange::compatibility(package, set)
                            );
                        }
                        // Complex case, the target was provided and differs from the installed one
                        // Determine which Python version requirement was not met
                        if !set.contains(python.target()) {
                            return format!(
                                "the requested {package} version ({}) does not satisfy {}",
                                python.target(),
                                PackageRange::compatibility(package, set)
                            );
                        }
                        // TODO(zanieb): Explain to the user why the installed version is relevant
                        //               when they provided a target version; probably via a "hint"
                        debug_assert!(
                            !set.contains(python.installed()),
                            "There should not be an incompatibility where the range is satisfied by both Python requirements"
                        );
                        return format!(
                            "the current {package} version ({}) does not satisfy {}",
                            python.installed(),
                            PackageRange::compatibility(package, set)
                        );
                    }
                    // We should always have the required Python versions, if we don't we'll fall back
                    // to a less helpful message in production
                    debug_assert!(
                        false,
                        "Error reporting should always be provided with Python versions"
                    );
                }
                let set = self.simplify_set(set, package);

                // Check for a reason
                if let Some(reason) = reason {
                    let formatted = if set.as_ref() == &Range::full() {
                        format!("{package} {reason}")
                    } else {
                        format!("{package}{set} {reason}")
                    };
                    return formatted;
                }

                if set.as_ref() == &Range::full() {
                    format!("there are no versions of {package}")
                } else if set.as_singleton().is_some() {
                    format!("there is no version of {package}{set}")
                } else {
                    let complement = set.complement();
                    let segments = complement.iter().collect::<Vec<_>>().len();
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
            External::Unavailable(package, set, reason) => match package {
                PubGrubPackage::Root(Some(name)) => {
                    format!("{name} cannot be used because {reason}")
                }
                PubGrubPackage::Root(None) => {
                    format!("your requirements cannot be used because {reason}")
                }
                _ => format!(
                    "{}is unusable because {reason}",
                    Padded::new("", &PackageRange::compatibility(package, set), " ")
                ),
            },
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                let package_set = self.simplify_set(package_set, package);
                let dependency_set = self.simplify_set(dependency_set, dependency);
                match package {
                    PubGrubPackage::Root(Some(name)) => format!(
                        "{name} depends on {}",
                        PackageRange::dependency(dependency, &dependency_set)
                    ),
                    PubGrubPackage::Root(None) => format!(
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
        let terms_vec: Vec<_> = terms.iter().collect();
        match terms_vec.as_slice() {
            [] | [(PubGrubPackage::Root(_), _)] => "the requirements are unsatisfiable".into(),
            [(package @ PubGrubPackage::Package(..), Term::Positive(range))] => {
                let range = self.simplify_set(range, package);
                format!(
                    "{} cannot be used",
                    PackageRange::compatibility(package, &range)
                )
            }
            [(package @ PubGrubPackage::Package(..), Term::Negative(range))] => {
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
                result.push_str(" are incompatible");
                result
            }
        }
    }

    /// Simplest case, we just combine two external incompatibilities.
    fn explain_both_external(
        &self,
        external1: &External<PubGrubPackage, Range<Version>>,
        external2: &External<PubGrubPackage, Range<Version>>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let external1 = self.format_external(external1);
        let external2 = self.format_external(external2);
        let terms = self.format_terms(current_terms);

        format!(
            "Because {}and {}we can conclude that {}",
            Padded::from_string("", &external1, " "),
            Padded::from_string("", &external2, ", "),
            Padded::from_string("", &terms, ".")
        )
    }

    /// Both causes have already been explained so we use their refs.
    fn explain_both_ref(
        &self,
        ref_id1: usize,
        derived1: &Derived<PubGrubPackage, Range<Version>>,
        ref_id2: usize,
        derived2: &Derived<PubGrubPackage, Range<Version>>,
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
        derived: &Derived<PubGrubPackage, Range<Version>>,
        external: &External<PubGrubPackage, Range<Version>>,
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
        external: &External<PubGrubPackage, Range<Version>>,
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
        derived: &Derived<PubGrubPackage, Range<Version>>,
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
        prior_external: &External<PubGrubPackage, Range<Version>>,
        external: &External<PubGrubPackage, Range<Version>>,
        current_terms: &Map<PubGrubPackage, Term<Range<Version>>>,
    ) -> String {
        let prior_external = self.format_external(prior_external);
        let external = self.format_external(external);
        let terms = self.format_terms(current_terms);

        format!(
            "And because {}and {}we can conclude that {}",
            Padded::from_string("", &prior_external, " "),
            Padded::from_string("", &external, ", "),
            Padded::from_string("", &terms, "."),
        )
    }
}

impl PubGrubReportFormatter<'_> {
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
        derivation_tree: &DerivationTree<PubGrubPackage, Range<Version>>,
        selector: &Option<CandidateSelector>,
        index_locations: &Option<IndexLocations>,
        unavailable_packages: &FxHashMap<PackageName, UnavailablePackage>,
    ) -> IndexSet<PubGrubHint> {
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

        let mut hints = IndexSet::default();
        match derivation_tree {
            DerivationTree::External(external) => match external {
                External::NoVersions(package, set, _) => {
                    // Check for no versions due to pre-release options
                    if let Some(selector) = selector {
                        if set.bounds().any(Version::any_prerelease) {
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

                    // Check for no versions due to no `--find-links` flat index
                    if let Some(index_locations) = index_locations {
                        let no_find_links =
                            index_locations.flat_index().peekable().peek().is_none();

                        if let PubGrubPackage::Package(name, ..) = package {
                            match unavailable_packages.get(name) {
                                Some(UnavailablePackage::NoIndex) => {
                                    if no_find_links {
                                        hints.insert(PubGrubHint::NoIndex);
                                    }
                                }
                                Some(UnavailablePackage::Offline) => {
                                    hints.insert(PubGrubHint::Offline);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                External::NotRoot(..) => {}
                External::Unavailable(..) => {}
                External::FromDependencyOf(..) => {}
            },
            DerivationTree::Derived(derived) => {
                hints.extend(self.hints(
                    &derived.cause1,
                    selector,
                    index_locations,
                    unavailable_packages,
                ));
                hints.extend(self.hints(
                    &derived.cause2,
                    selector,
                    index_locations,
                    unavailable_packages,
                ));
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
    /// A package was not found in the registry, but
    Offline,
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
                    PackageRange::compatibility(package, range).bold()
                )
            }
            PubGrubHint::NoIndex => {
                write!(
                    f,
                    "{}{} Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)",
                    "hint".bold().cyan(),
                    ":".bold(),
                )
            }
            PubGrubHint::Offline => {
                write!(
                    f,
                    "{}{} Packages were unavailable because the network was disabled",
                    "hint".bold().cyan(),
                    ":".bold(),
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
    fn new<'a>(package: &'a PubGrubPackage, term: &'a Term<Range<Version>>) -> PackageTerm<'a> {
        PackageTerm { package, term }
    }
}

/// The kind of version ranges being displayed in [`PackageRange`]
#[derive(Debug)]
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
        if self.range.is_empty() {
            false
        } else {
            let segments: Vec<_> = self.range.iter().collect();
            // "all versions of" is the only plural case
            matches!(segments.as_slice(), [(Bound::Unbounded, Bound::Unbounded)])
        }
    }
}

impl std::fmt::Display for PackageRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.range.is_empty() {
            write!(f, "∅")?;
        } else {
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
                let package = self.package;
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
            first: self,
            second: PackageRange::dependency(package, range),
        }
    }
}

/// A representation of A depends on B.
#[derive(Debug)]
struct DependsOn<'a> {
    first: &'a PackageRange<'a>,
    second: PackageRange<'a>,
}

impl std::fmt::Display for DependsOn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Padded::new("", self.first, " "))?;
        if self.first.plural() {
            write!(f, "depend on ")?;
        } else {
            write!(f, "depends on ")?;
        };
        write!(f, "{}", self.second)?;
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
