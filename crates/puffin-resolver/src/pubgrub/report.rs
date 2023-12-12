use pubgrub::range::Range;
use pubgrub::report::{External, ReportFormatter};
use pubgrub::term::Term;
use pubgrub::type_aliases::Map;
use rustc_hash::FxHashMap;

use super::{PubGrubPackage, PubGrubVersion};

#[derive(Debug)]
pub struct PubGrubReportFormatter<'a> {
    /// The versions that were available for each package
    pub available_versions: &'a FxHashMap<PubGrubPackage, Vec<PubGrubVersion>>,
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
                    let dependency_set = self.simplify_set(dependency_set, package);
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
                    let dependency_set = self.simplify_set(dependency_set, package);
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
