// SPDX-License-Identifier: MPL-2.0

//! Build a report as clear as possible as to why
//! dependency solving failed.

use std::fmt;
use std::ops::{Deref, DerefMut};

use crate::package::Package;
use crate::range::Range;
use crate::term::Term;
use crate::type_aliases::Map;
use crate::version::Version;

/// Reporter trait.
pub trait Reporter<P: Package, V: Version> {
    /// Output type of the report.
    type Output;

    /// Generate a report from the derivation tree
    /// describing the resolution failure.
    fn report(derivation_tree: &DerivationTree<P, V>) -> Self::Output;
}

/// Derivation tree resulting in the impossibility
/// to solve the dependencies of our root package.
#[derive(Debug, Clone)]
pub enum DerivationTree<P: Package, V: Version> {
    /// External incompatibility.
    External(External<P, V>),
    /// Incompatibility derived from two others.
    Derived(Derived<P, V>),
}

/// Incompatibilities that are not derived from others,
/// they have their own reason.
#[derive(Debug, Clone)]
pub enum External<P: Package, V: Version> {
    /// Initial incompatibility aiming at picking the root package for the first decision.
    NotRoot(P, V),
    /// There are no versions in the given range for this package.
    NoVersions(P, Range<V>),
    /// Dependencies of the package are unavailable for versions in that range.
    UnavailableDependencies(P, Range<V>),
    /// Incompatibility coming from the dependencies of a given package.
    FromDependencyOf(P, Range<V>, P, Range<V>),
}

/// Incompatibility derived from two others.
#[derive(Debug, Clone)]
pub struct Derived<P: Package, V: Version> {
    /// Terms of the incompatibility.
    pub terms: Map<P, Term<V>>,
    /// Indicate if that incompatibility is present multiple times
    /// in the derivation tree.
    /// If that is the case, it has a unique id, provided in that option.
    /// Then, we may want to only explain it once,
    /// and refer to the explanation for the other times.
    pub shared_id: Option<usize>,
    /// First cause.
    pub cause1: Box<DerivationTree<P, V>>,
    /// Second cause.
    pub cause2: Box<DerivationTree<P, V>>,
}

impl<P: Package, V: Version> DerivationTree<P, V> {
    /// Merge the [NoVersions](External::NoVersions) external incompatibilities
    /// with the other one they are matched with
    /// in a derived incompatibility.
    /// This cleans up quite nicely the generated report.
    /// You might want to do this if you know that the
    /// [DependencyProvider](crate::solver::DependencyProvider)
    /// was not run in some kind of offline mode that may not
    /// have access to all versions existing.
    pub fn collapse_no_versions(&mut self) {
        match self {
            DerivationTree::External(_) => {}
            DerivationTree::Derived(derived) => {
                match (derived.cause1.deref_mut(), derived.cause2.deref_mut()) {
                    (DerivationTree::External(External::NoVersions(p, r)), ref mut cause2) => {
                        cause2.collapse_no_versions();
                        *self = cause2
                            .clone()
                            .merge_no_versions(p.to_owned(), r.to_owned())
                            .unwrap_or_else(|| self.to_owned());
                    }
                    (ref mut cause1, DerivationTree::External(External::NoVersions(p, r))) => {
                        cause1.collapse_no_versions();
                        *self = cause1
                            .clone()
                            .merge_no_versions(p.to_owned(), r.to_owned())
                            .unwrap_or_else(|| self.to_owned());
                    }
                    _ => {
                        derived.cause1.collapse_no_versions();
                        derived.cause2.collapse_no_versions();
                    }
                }
            }
        }
    }

    fn merge_no_versions(self, package: P, range: Range<V>) -> Option<Self> {
        match self {
            // TODO: take care of the Derived case.
            // Once done, we can remove the Option.
            DerivationTree::Derived(_) => Some(self),
            DerivationTree::External(External::NotRoot(_, _)) => {
                panic!("How did we end up with a NoVersions merged with a NotRoot?")
            }
            DerivationTree::External(External::NoVersions(_, r)) => Some(DerivationTree::External(
                External::NoVersions(package, range.union(&r)),
            )),
            DerivationTree::External(External::UnavailableDependencies(_, r)) => {
                Some(DerivationTree::External(External::UnavailableDependencies(
                    package,
                    range.union(&r),
                )))
            }
            DerivationTree::External(External::FromDependencyOf(p1, r1, p2, r2)) => {
                if p1 == package {
                    Some(DerivationTree::External(External::FromDependencyOf(
                        p1,
                        r1.union(&range),
                        p2,
                        r2,
                    )))
                } else {
                    Some(DerivationTree::External(External::FromDependencyOf(
                        p1,
                        r1,
                        p2,
                        r2.union(&range),
                    )))
                }
            }
        }
    }
}

impl<P: Package, V: Version> fmt::Display for External<P, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRoot(package, version) => {
                write!(f, "we are solving dependencies of {} {}", package, version)
            }
            Self::NoVersions(package, range) => {
                if range == &Range::any() {
                    write!(f, "there is no available version for {}", package)
                } else {
                    write!(f, "there is no version of {} in {}", package, range)
                }
            }
            Self::UnavailableDependencies(package, range) => {
                if range == &Range::any() {
                    write!(f, "dependencies of {} are unavailable", package)
                } else {
                    write!(
                        f,
                        "dependencies of {} at version {} are unavailable",
                        package, range
                    )
                }
            }
            Self::FromDependencyOf(p, range_p, dep, range_dep) => {
                if range_p == &Range::any() && range_dep == &Range::any() {
                    write!(f, "{} depends on {}", p, dep)
                } else if range_p == &Range::any() {
                    write!(f, "{} depends on {} {}", p, dep, range_dep)
                } else if range_dep == &Range::any() {
                    write!(f, "{} {} depends on {}", p, range_p, dep)
                } else {
                    write!(f, "{} {} depends on {} {}", p, range_p, dep, range_dep)
                }
            }
        }
    }
}

/// Default reporter able to generate an explanation as a [String].
pub struct DefaultStringReporter {
    /// Number of explanations already with a line reference.
    ref_count: usize,
    /// Shared nodes that have already been marked with a line reference.
    /// The incompatibility ids are the keys, and the line references are the values.
    shared_with_ref: Map<usize, usize>,
    /// Accumulated lines of the report already generated.
    lines: Vec<String>,
}

impl DefaultStringReporter {
    /// Initialize the reporter.
    fn new() -> Self {
        Self {
            ref_count: 0,
            shared_with_ref: Map::default(),
            lines: Vec::new(),
        }
    }

    fn build_recursive<P: Package, V: Version>(&mut self, derived: &Derived<P, V>) {
        self.build_recursive_helper(derived);
        if let Some(id) = derived.shared_id {
            if self.shared_with_ref.get(&id) == None {
                self.add_line_ref();
                self.shared_with_ref.insert(id, self.ref_count);
            }
        };
    }

    fn build_recursive_helper<P: Package, V: Version>(&mut self, current: &Derived<P, V>) {
        match (current.cause1.deref(), current.cause2.deref()) {
            (DerivationTree::External(external1), DerivationTree::External(external2)) => {
                // Simplest case, we just combine two external incompatibilities.
                self.lines.push(Self::explain_both_external(
                    external1,
                    external2,
                    &current.terms,
                ));
            }
            (DerivationTree::Derived(derived), DerivationTree::External(external)) => {
                // One cause is derived, so we explain this first
                // then we add the one-line external part
                // and finally conclude with the current incompatibility.
                self.report_one_each(derived, external, &current.terms);
            }
            (DerivationTree::External(external), DerivationTree::Derived(derived)) => {
                self.report_one_each(derived, external, &current.terms);
            }
            (DerivationTree::Derived(derived1), DerivationTree::Derived(derived2)) => {
                // This is the most complex case since both causes are also derived.
                match (
                    self.line_ref_of(derived1.shared_id),
                    self.line_ref_of(derived2.shared_id),
                ) {
                    // If both causes already have been referenced (shared_id),
                    // the explanation simply uses those references.
                    (Some(ref1), Some(ref2)) => self.lines.push(Self::explain_both_ref(
                        ref1,
                        derived1,
                        ref2,
                        derived2,
                        &current.terms,
                    )),
                    // Otherwise, if one only has a line number reference,
                    // we recursively call the one without reference and then
                    // add the one with reference to conclude.
                    (Some(ref1), None) => {
                        self.build_recursive(derived2);
                        self.lines
                            .push(Self::and_explain_ref(ref1, derived1, &current.terms));
                    }
                    (None, Some(ref2)) => {
                        self.build_recursive(derived1);
                        self.lines
                            .push(Self::and_explain_ref(ref2, derived2, &current.terms));
                    }
                    // Finally, if no line reference exists yet,
                    // we call recursively the first one and then,
                    //   - if this was a shared node, it will get a line ref
                    //     and we can simply recall this with the current node.
                    //   - otherwise, we add a line reference to it,
                    //     recursively call on the second node,
                    //     and finally conclude.
                    (None, None) => {
                        self.build_recursive(derived1);
                        if derived1.shared_id != None {
                            self.lines.push("".into());
                            self.build_recursive(current);
                        } else {
                            self.add_line_ref();
                            let ref1 = self.ref_count;
                            self.lines.push("".into());
                            self.build_recursive(derived2);
                            self.lines
                                .push(Self::and_explain_ref(ref1, derived1, &current.terms));
                        }
                    }
                }
            }
        }
    }

    /// Report a derived and an external incompatibility.
    ///
    /// The result will depend on the fact that the derived incompatibility
    /// has already been explained or not.
    fn report_one_each<P: Package, V: Version>(
        &mut self,
        derived: &Derived<P, V>,
        external: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) {
        match self.line_ref_of(derived.shared_id) {
            Some(ref_id) => self.lines.push(Self::explain_ref_and_external(
                ref_id,
                derived,
                external,
                current_terms,
            )),
            None => self.report_recurse_one_each(derived, external, current_terms),
        }
    }

    /// Report one derived (without a line ref yet) and one external.
    fn report_recurse_one_each<P: Package, V: Version>(
        &mut self,
        derived: &Derived<P, V>,
        external: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) {
        match (derived.cause1.deref(), derived.cause2.deref()) {
            // If the derived cause has itself one external prior cause,
            // we can chain the external explanations.
            (DerivationTree::Derived(prior_derived), DerivationTree::External(prior_external)) => {
                self.build_recursive(prior_derived);
                self.lines.push(Self::and_explain_prior_and_external(
                    prior_external,
                    external,
                    current_terms,
                ));
            }
            // If the derived cause has itself one external prior cause,
            // we can chain the external explanations.
            (DerivationTree::External(prior_external), DerivationTree::Derived(prior_derived)) => {
                self.build_recursive(prior_derived);
                self.lines.push(Self::and_explain_prior_and_external(
                    prior_external,
                    external,
                    current_terms,
                ));
            }
            _ => {
                self.build_recursive(derived);
                self.lines
                    .push(Self::and_explain_external(external, current_terms));
            }
        }
    }

    // String explanations #####################################################

    /// Simplest case, we just combine two external incompatibilities.
    fn explain_both_external<P: Package, V: Version>(
        external1: &External<P, V>,
        external2: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        // TODO: order should be chosen to make it more logical.
        format!(
            "Because {} and {}, {}.",
            external1,
            external2,
            Self::string_terms(current_terms)
        )
    }

    /// Both causes have already been explained so we use their refs.
    fn explain_both_ref<P: Package, V: Version>(
        ref_id1: usize,
        derived1: &Derived<P, V>,
        ref_id2: usize,
        derived2: &Derived<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        // TODO: order should be chosen to make it more logical.
        format!(
            "Because {} ({}) and {} ({}), {}.",
            Self::string_terms(&derived1.terms),
            ref_id1,
            Self::string_terms(&derived2.terms),
            ref_id2,
            Self::string_terms(current_terms)
        )
    }

    /// One cause is derived (already explained so one-line),
    /// the other is a one-line external cause,
    /// and finally we conclude with the current incompatibility.
    fn explain_ref_and_external<P: Package, V: Version>(
        ref_id: usize,
        derived: &Derived<P, V>,
        external: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        // TODO: order should be chosen to make it more logical.
        format!(
            "Because {} ({}) and {}, {}.",
            Self::string_terms(&derived.terms),
            ref_id,
            external,
            Self::string_terms(current_terms)
        )
    }

    /// Add an external cause to the chain of explanations.
    fn and_explain_external<P: Package, V: Version>(
        external: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        format!(
            "And because {}, {}.",
            external,
            Self::string_terms(current_terms)
        )
    }

    /// Add an already explained incompat to the chain of explanations.
    fn and_explain_ref<P: Package, V: Version>(
        ref_id: usize,
        derived: &Derived<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        format!(
            "And because {} ({}), {}.",
            Self::string_terms(&derived.terms),
            ref_id,
            Self::string_terms(current_terms)
        )
    }

    /// Add an already explained incompat to the chain of explanations.
    fn and_explain_prior_and_external<P: Package, V: Version>(
        prior_external: &External<P, V>,
        external: &External<P, V>,
        current_terms: &Map<P, Term<V>>,
    ) -> String {
        format!(
            "And because {} and {}, {}.",
            prior_external,
            external,
            Self::string_terms(current_terms)
        )
    }

    /// Try to print terms of an incompatibility in a human-readable way.
    pub fn string_terms<P: Package, V: Version>(terms: &Map<P, Term<V>>) -> String {
        let terms_vec: Vec<_> = terms.iter().collect();
        match terms_vec.as_slice() {
            [] => "version solving failed".into(),
            // TODO: special case when that unique package is root.
            [(package, Term::Positive(range))] => format!("{} {} is forbidden", package, range),
            [(package, Term::Negative(range))] => format!("{} {} is mandatory", package, range),
            [(p1, Term::Positive(r1)), (p2, Term::Negative(r2))] => {
                External::FromDependencyOf(p1, r1.clone(), p2, r2.clone()).to_string()
            }
            [(p1, Term::Negative(r1)), (p2, Term::Positive(r2))] => {
                External::FromDependencyOf(p2, r2.clone(), p1, r1.clone()).to_string()
            }
            slice => {
                let str_terms: Vec<_> = slice.iter().map(|(p, t)| format!("{} {}", p, t)).collect();
                str_terms.join(", ") + " are incompatible"
            }
        }
    }

    // Helper functions ########################################################

    fn add_line_ref(&mut self) {
        let new_count = self.ref_count + 1;
        self.ref_count = new_count;
        if let Some(line) = self.lines.last_mut() {
            *line = format!("{} ({})", line, new_count);
        }
    }

    fn line_ref_of(&self, shared_id: Option<usize>) -> Option<usize> {
        shared_id.and_then(|id| self.shared_with_ref.get(&id).cloned())
    }
}

impl<P: Package, V: Version> Reporter<P, V> for DefaultStringReporter {
    type Output = String;

    fn report(derivation_tree: &DerivationTree<P, V>) -> Self::Output {
        match derivation_tree {
            DerivationTree::External(external) => external.to_string(),
            DerivationTree::Derived(derived) => {
                let mut reporter = Self::new();
                reporter.build_recursive(derived);
                reporter.lines.join("\n")
            }
        }
    }
}
