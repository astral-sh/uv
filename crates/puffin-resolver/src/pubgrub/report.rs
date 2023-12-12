use pubgrub::range::Range;
use pubgrub::report::{External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;

use super::{PubGrubPackage, PubGrubVersion};

#[derive(Debug, Default)]
pub struct PubGrubReportFormatter;

impl ReportFormatter<PubGrubPackage, Range<PubGrubVersion>> for PubGrubReportFormatter {
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
                    format!("there is no version of {package} available matching {set}")
                }
            }
            External::UnavailableDependencies(package, set) => {
                if set == &Range::full() {
                    format!("dependencies of {package} are unavailable")
                } else {
                    format!("dependencies of {package} at version {set} are unavailable")
                }
            }
            External::UnusableVersions(package, set, reason) => {
                if let Some(reason) = reason {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        format!("{package} is unusable: {reason}")
                    } else {
                        if set == &Range::full() {
                            format!("all versions of {package} are unusable: {reason}")
                        } else {
                            format!("{package}{set} is unusable: {reason}",)
                        }
                    }
                } else {
                    if set == &Range::full() {
                        format!("all versions of {package} are unusable")
                    } else {
                        format!("{package}{set} is unusable")
                    }
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
                            format!("dependencies of {package}{set} are unusable: {reason}",)
                        }
                    }
                } else {
                    if set == &Range::full() {
                        format!("dependencies of {package} are unusable")
                    } else {
                        format!("dependencies of {package}{set} are unusable")
                    }
                }
            }
            External::FromDependencyOf(package, package_set, dependency, dependency_set) => {
                if package_set == &Range::full() && dependency_set == &Range::full() {
                    format!("{package} depends on {dependency}")
                } else if package_set == &Range::full() {
                    format!("{package} depends on {dependency}{dependency_set}")
                } else if dependency_set == &Range::full() {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!("{package} depends on {dependency}")
                    } else {
                        format!("{package}{package_set} depends on {dependency}")
                    }
                } else {
                    if matches!(package, PubGrubPackage::Root(_)) {
                        // Exclude the dummy version for root packages
                        format!("{package} depends on {dependency}{dependency_set}")
                    } else {
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
                format!("{package}{range} is forbidden")
            }
            [(package @ PubGrubPackage::Package(..), Term::Negative(range))] => {
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
