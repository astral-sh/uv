// SPDX-License-Identifier: MPL-2.0

use pubgrub::error::PubGrubError;
use pubgrub::package::Package;
use pubgrub::solver::{Dependencies, DependencyProvider, OfflineDependencyProvider};
use pubgrub::type_aliases::{Map, SelectedDependencies};
use pubgrub::version_set::VersionSet;
use varisat::ExtendFormula;

fn sat_at_most_one(solver: &mut impl varisat::ExtendFormula, vars: &[varisat::Var]) {
    if vars.len() <= 1 {
        return;
    } else if vars.len() == 2 {
        solver.add_clause(&[vars[0].negative(), vars[1].negative()]);
        return;
    } else if vars.len() == 3 {
        solver.add_clause(&[vars[0].negative(), vars[1].negative()]);
        solver.add_clause(&[vars[0].negative(), vars[2].negative()]);
        solver.add_clause(&[vars[1].negative(), vars[2].negative()]);
        return;
    }
    // use the "Binary Encoding" from
    // https://www.it.uu.se/research/group/astra/ModRef10/papers/Alan%20M.%20Frisch%20and%20Paul%20A.%20Giannoros.%20SAT%20Encodings%20of%20the%20At-Most-k%20Constraint%20-%20ModRef%202010.pdf
    let len_bits = vars.len().ilog2() as usize + 1;
    let bits: Vec<varisat::Var> = solver.new_var_iter(len_bits).collect();
    for (i, p) in vars.iter().enumerate() {
        for (j, &bit) in bits.iter().enumerate() {
            solver.add_clause(&[p.negative(), bit.lit(((1 << j) & i) > 0)]);
        }
    }
}

/// Resolution can be reduced to the SAT problem. So this is an alternative implementation
/// of the resolver that uses a SAT library for the hard work. This is intended to be easy to read,
/// as compared to the real resolver. This will find a valid resolution if one exists.
///
/// The SAT library does not optimize for the newer version,
/// so the selected packages may not match the real resolver.
pub struct SatResolve<P: Package, VS: VersionSet> {
    solver: varisat::Solver<'static>,
    all_versions_by_p: Map<P, Vec<(VS::V, varisat::Var)>>,
}

impl<P: Package, VS: VersionSet> SatResolve<P, VS> {
    pub fn new(dp: &OfflineDependencyProvider<P, VS>) -> Self {
        let mut cnf = varisat::CnfFormula::new();

        let mut all_versions = vec![];
        let mut all_versions_by_p: Map<P, Vec<(VS::V, varisat::Var)>> = Map::default();

        for p in dp.packages() {
            let mut versions_for_p = vec![];
            for v in dp.versions(p).unwrap() {
                let new_var = cnf.new_var();
                all_versions.push((p.clone(), v.clone(), new_var));
                versions_for_p.push(new_var);
                all_versions_by_p
                    .entry(p.clone())
                    .or_default()
                    .push((v.clone(), new_var));
            }
            // no two versions of the same package
            sat_at_most_one(&mut cnf, &versions_for_p);
        }

        // active packages need each of there `deps` to be satisfied
        for (p, v, var) in &all_versions {
            let deps = match dp.get_dependencies(p, v).unwrap() {
                Dependencies::Unknown => panic!(),
                Dependencies::Known(d) => d,
            };
            for (p1, range) in deps {
                let empty_vec = vec![];
                let mut matches: Vec<varisat::Lit> = all_versions_by_p
                    .get(&p1)
                    .unwrap_or(&empty_vec)
                    .iter()
                    .filter(|(v1, _)| range.contains(v1))
                    .map(|(_, var1)| var1.positive())
                    .collect();
                // ^ the `dep` is satisfied or
                matches.push(var.negative());
                // ^ `p` is not active
                cnf.add_clause(&matches);
            }
        }

        let mut solver = varisat::Solver::new();
        solver.add_formula(&cnf);

        // We dont need to `solve` now. We know that "use nothing" will satisfy all the clauses so far.
        // But things run faster if we let it spend some time figuring out how the constraints interact before we add assumptions.
        solver
            .solve()
            .expect("docs say it can't error in default config");

        Self {
            solver,
            all_versions_by_p,
        }
    }

    pub fn resolve(&mut self, name: &P, ver: &VS::V) -> bool {
        if let Some(vers) = self.all_versions_by_p.get(name) {
            if let Some((_, var)) = vers.iter().find(|(v, _)| v == ver) {
                self.solver.assume(&[var.positive()]);

                self.solver
                    .solve()
                    .expect("docs say it can't error in default config")
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn is_valid_solution(&mut self, pids: &SelectedDependencies<P, VS::V>) -> bool {
        let mut assumption = vec![];

        for (p, vs) in &self.all_versions_by_p {
            let pid_for_p = pids.get(p);
            for (v, var) in vs {
                assumption.push(var.lit(pid_for_p == Some(v)))
            }
        }

        self.solver.assume(&assumption);

        self.solver
            .solve()
            .expect("docs say it can't error in default config")
    }

    pub fn check_resolve(
        &mut self,
        res: &Result<SelectedDependencies<P, VS::V>, PubGrubError<P, VS>>,
        p: &P,
        v: &VS::V,
    ) {
        match res {
            Ok(s) => {
                assert!(self.is_valid_solution(s));
            }
            Err(_) => {
                assert!(!self.resolve(p, v));
            }
        }
    }
}
