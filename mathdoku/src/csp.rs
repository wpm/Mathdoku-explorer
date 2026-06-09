#![allow(dead_code)]
//! Generic constraint satisfaction problem (CSP) abstractions.
//!
//! This module defines the core traits of a CSP — [`State`] and [`Constraint`] —
//! and the [`generalized_arc_consistency`] algorithm that ties them together. The
//! concrete solver in [`crate::grid_csp`] implements these abstractions for the
//! Mathdoku grid.
//!
//! ## Structure
//!
//! A CSP consists of:
//!
//! - **Variables** — decision points, each participating in a set of constraints.
//! - **Constraints** — relations over subsets of variables (the constraint's *scope*) that rule out
//!   inconsistent value combinations.
//!
//! The relationship between variables and constraints forms a bipartite *constraint graph*:
//! variables on one side, constraints on the other, with edges connecting each variable to
//! the constraints whose scope includes it.
//!
//! ## Propagation
//!
//! [`generalized_arc_consistency`] enforces GAC via the AC-3 worklist algorithm: it
//! propagates each constraint in turn, and whenever a variable's domain shrinks, it
//! re-queues all constraints adjacent to that variable. The algorithm terminates at a
//! fixpoint where no constraint can narrow any domain further.

use std::collections::VecDeque;

/// A state that maps variables of type `V` to domains of type `D`.
pub trait State<V, D, E> {
    /// Get the domain of the specified variable.
    ///
    /// # Errors
    ///
    /// Returns an error if `variable` is not in the state.
    fn get(&self, variable: V) -> Result<D, E>;
}

/// A relation over a set of variables (the constraint's scope) in a constraint satisfaction
/// problem.
///
/// A constraint is satisfied when the values assigned to its scope variables are jointly
/// consistent. Propagation enforces generalized arc consistency (GAC): it removes from each
/// variable's domain any value not supported by some consistent tuple over the scope.
pub trait Constraint<S, V, D, E>: Sized + Clone + std::fmt::Display
where
    S: State<V, D, E>,
{
    /// Applies this constraint to `state`, returning the updated state and the variables
    /// whose domains were narrowed.
    ///
    /// # Errors
    /// Returns an error if propagation fails (e.g. a cell is out of bounds).
    fn propagate(&self, state: &S) -> Result<(S, Vec<V>), E>;

    /// Is `variable` in the scope of this constraint?
    fn in_scope(&self, variable: V) -> bool;
}

/// A candidate value set for a CSP variable.
///
/// The GAC algorithm uses [`Domain::is_empty`] to detect infeasibility: as soon
/// as any variable's domain empties, no solution exists and propagation stops.
pub trait Domain: std::fmt::Display {
    fn is_empty(&self) -> bool;
}

/// Enforces generalized arc consistency (GAC) via the AC-3 worklist algorithm.
///
/// AC-3 operates on the constraint graph, a bipartite graph with variables on one side and
/// constraints on the other. It maintains a queue of constraints to process. When a constraint
/// is propagated and reduces a variable's domain, all constraints adjacent to that variable are
/// re-added to the queue. The algorithm terminates when the queue is empty, at which point the
/// state is arc-consistent: no constraint can reduce any variable's domain further.
///
/// Returns `None` if any variable's domain becomes empty (infeasible) during propagation,
/// or if any constraint signals an error.
pub fn generalized_arc_consistency<S, V, C, D, E>(mut state: S, constraints: &[C]) -> Option<S>
where
    S: State<V, D, E>,
    V: Clone + std::fmt::Display,
    D: Domain,
    C: Constraint<S, V, D, E>,
{
    let mut q: VecDeque<C> = constraints.iter().cloned().collect();
    #[cfg(debug_assertions)]
    let mut pass = 0usize;
    while let Some(constraint) = q.pop_front() {
        #[cfg(debug_assertions)]
        {
            if q.is_empty() || pass == 0 {
                pass += 1;
                log::debug!("━━━ Pass {pass} (queue len {}) ━━━", q.len() + 1);
            }
        }
        log::debug!("  propagate: {constraint}");
        let (new_state, narrowed) = constraint.propagate(&state).ok()?;
        state = new_state;
        if !narrowed.is_empty() {
            for v in &narrowed {
                if let Ok(domain) = state.get(v.clone()) {
                    log::debug!("    narrowed: {v} → {domain}");
                }
            }
        }
        if narrowed
            .iter()
            .any(|v| state.get(v.clone()).ok().is_some_and(|d| d.is_empty()))
        {
            log::debug!("  infeasible: domain emptied");
            return None;
        }
        q.extend(
            constraints
                .iter()
                .filter(|c| narrowed.iter().any(|v| c.in_scope(v.clone())))
                .cloned(),
        );
    }
    Some(state)
}

#[cfg(test)]
mod tests {
    use super::{Constraint, State, generalized_arc_consistency};
    use crate::csp::tests::Constraints::{Equal, Sum};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn equal_overlapping_domains_intersects_both() {
        // x ∈ {1,2,3}, y ∈ {2,3,4}, x=y  →  both {2,3}
        let state = IntegerSets::new(&[("x", &[1, 2, 3]), ("y", &[2, 3, 4])]);
        let result = run(state, &[Constraints::equal("x", "y")]);
        assert_eq!(sorted(&result, "x"), [2, 3]);
        assert_eq!(sorted(&result, "y"), [2, 3]);
    }

    #[test]
    fn equal_singleton_pins_other_variable() {
        // x ∈ {5}, y ∈ {1,2,3,4,5}, x=y  →  both {5}
        let state = IntegerSets::new(&[("x", &[5]), ("y", &[1, 2, 3, 4, 5])]);
        let result = run(state, &[Constraints::equal("x", "y")]);
        assert_eq!(sorted(&result, "x"), [5]);
        assert_eq!(sorted(&result, "y"), [5]);
    }

    #[test]
    fn equal_disjoint_domains_empties_both() {
        // x ∈ {1,2}, y ∈ {3,4}, x=y  →  infeasible (no common values)
        let state = IntegerSets::new(&[("x", &[1, 2]), ("y", &[3, 4])]);
        run_infeasible(state, &[Constraints::equal("x", "y")]);
    }

    #[test]
    fn sum_two_vars_prunes_unsupported_values() {
        // x,y ∈ {1,2,3}, x+y=5  →  only (2,3),(3,2) work, so x,y ∈ {2,3}
        let state = IntegerSets::new(&[("x", &[1, 2, 3]), ("y", &[1, 2, 3])]);
        let result = run(state, &[Constraints::sum(&["x", "y"], 5)]);
        assert_eq!(sorted(&result, "x"), [2, 3]);
        assert_eq!(sorted(&result, "y"), [2, 3]);
    }

    #[test]
    fn sum_three_vars_all_values_survive() {
        // x,y,z ∈ {1,2,3}, x+y+z=6  →  permutations of (1,2,3) use every value
        let state = IntegerSets::new(&[("x", &[1, 2, 3]), ("y", &[1, 2, 3]), ("z", &[1, 2, 3])]);
        let result = run(state, &[Constraints::sum(&["x", "y", "z"], 6)]);
        assert_eq!(sorted(&result, "x"), [1, 2, 3]);
        assert_eq!(sorted(&result, "y"), [1, 2, 3]);
        assert_eq!(sorted(&result, "z"), [1, 2, 3]);
    }

    #[test]
    fn sum_infeasible_target_empties_domains() {
        // x,y ∈ {1,2}, x+y=10 — impossible
        let state = IntegerSets::new(&[("x", &[1, 2]), ("y", &[1, 2])]);
        run_infeasible(state, &[Constraints::sum(&["x", "y"], 10)]);
    }

    #[test]
    fn propagation_chains_across_constraints() {
        // x,y,z ∈ {1,2,3}; x+y=5 pins x,y ∈ {2,3}; then x=z chains to pin z ∈ {2,3}
        let state = IntegerSets::new(&[("x", &[1, 2, 3]), ("y", &[1, 2, 3]), ("z", &[1, 2, 3])]);
        let result = run(
            state,
            &[
                Constraints::sum(&["x", "y"], 5),
                Constraints::equal("x", "z"),
            ],
        );
        assert_eq!(sorted(&result, "x"), [2, 3]);
        assert_eq!(sorted(&result, "y"), [2, 3]);
        assert_eq!(sorted(&result, "z"), [2, 3]);
    }

    fn run(state: IntegerSets, constraints: &[Constraints]) -> IntegerSets {
        generalized_arc_consistency(state, constraints).expect("expected feasible state")
    }

    fn run_infeasible(state: IntegerSets, constraints: &[Constraints]) {
        assert!(
            generalized_arc_consistency(state, constraints).is_none(),
            "expected infeasible (None)"
        );
    }

    fn sorted(result: &IntegerSets, var: &str) -> Vec<u8> {
        let mut v: Vec<u8> = result
            .get(var.to_string())
            .unwrap()
            .0
            .iter()
            .copied()
            .collect();
        v.sort_unstable();
        v
    }

    #[derive(Clone, PartialEq)]
    struct TestDomain(HashSet<u8>);

    impl TestDomain {
        fn intersection<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = &'a u8> {
            self.0.intersection(&other.0)
        }
        fn insert(&mut self, v: u8) -> bool {
            self.0.insert(v)
        }
    }

    impl FromIterator<u8> for TestDomain {
        fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
            Self(iter.into_iter().collect())
        }
    }

    impl super::Domain for TestDomain {
        fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
    }

    impl std::fmt::Display for TestDomain {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let mut vals: Vec<u8> = self.0.iter().copied().collect();
            vals.sort_unstable();
            write!(
                f,
                "{{{}}}",
                vals.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    impl std::fmt::Display for Constraints {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Equal(a, b) => write!(f, "{a} = {b}"),
                Sum(vars, t) => write!(f, "{} = {t}", vars.join(" + ")),
            }
        }
    }

    struct IntegerSets(HashMap<String, TestDomain>);

    impl IntegerSets {
        fn new(init: &[(&str, &[u8])]) -> Self {
            Self(
                init.iter()
                    .map(|(k, v)| (k.to_string(), v.iter().copied().collect()))
                    .collect(),
            )
        }
    }
    impl State<String, TestDomain, InvalidVariable> for IntegerSets {
        fn get(&self, variable: String) -> Result<TestDomain, InvalidVariable> {
            self.0
                .get(&variable)
                .cloned()
                .ok_or(InvalidVariable(variable))
        }
    }
    #[derive(Debug)]
    struct InvalidVariable(String);

    #[derive(Clone)]
    enum Constraints {
        Equal(String, String),
        Sum(Vec<String>, u8),
    }

    impl Constraints {
        fn equal(a: &str, b: &str) -> Self {
            Equal(a.to_string(), b.to_string())
        }
        fn sum(vars: &[&str], target: u8) -> Self {
            Sum(vars.iter().map(ToString::to_string).collect(), target)
        }
    }

    impl Constraint<IntegerSets, String, TestDomain, InvalidVariable> for Constraints {
        fn propagate(
            &self,
            state: &IntegerSets,
        ) -> Result<(IntegerSets, Vec<String>), InvalidVariable> {
            // Returns updated state and names of variables whose domains shrank.
            let update = |name_a: &str,
                          old_a: &TestDomain,
                          new_a: TestDomain,
                          name_b: &str,
                          old_b: &TestDomain,
                          new_b: TestDomain|
             -> (IntegerSets, Vec<String>) {
                let mut changed = vec![];
                if &new_a != old_a {
                    changed.push(name_a.to_string());
                }
                if &new_b != old_b {
                    changed.push(name_b.to_string());
                }
                let mut new_map = state.0.clone();
                let _ = new_map.insert(name_a.to_string(), new_a);
                let _ = new_map.insert(name_b.to_string(), new_b);
                let new_state = IntegerSets(new_map);
                (new_state, changed)
            };

            match self {
                Equal(a, b) => {
                    let da = state.get(a.clone())?;
                    let db = state.get(b.clone())?;
                    let common: TestDomain = da.intersection(&db).copied().collect();
                    Ok(update(a, &da, common.clone(), b, &db, common))
                }
                Sum(vars, target) => {
                    fn enumerate(
                        domains: &[TestDomain],
                        supported: &mut Vec<TestDomain>,
                        idx: usize,
                        partial: u8,
                        target: u8,
                        assignment: &mut Vec<u8>,
                    ) {
                        if idx == domains.len() {
                            if partial == target {
                                for (i, &v) in assignment.iter().enumerate() {
                                    let _ = supported[i].insert(v);
                                }
                            }
                            return;
                        }
                        for &v in &domains[idx].0 {
                            let next = partial.saturating_add(v);
                            if next <= target {
                                assignment.push(v);
                                enumerate(domains, supported, idx + 1, next, target, assignment);
                                let _ = assignment.pop();
                            }
                        }
                    }
                    let domains: Vec<TestDomain> = vars
                        .iter()
                        .map(|v| state.get(v.clone()))
                        .collect::<Result<_, _>>()?;
                    let mut supported: Vec<TestDomain> = (0..vars.len())
                        .map(|_| TestDomain(HashSet::new()))
                        .collect();
                    enumerate(&domains, &mut supported, 0, 0, *target, &mut vec![]);
                    let mut changed = vec![];
                    let mut new_map = state.0.clone();
                    for (i, name) in vars.iter().enumerate() {
                        if supported[i] != domains[i] {
                            changed.push(name.clone());
                        }
                        let _ = new_map.insert(name.clone(), supported[i].clone());
                    }
                    Ok((IntegerSets(new_map), changed))
                }
            }
        }

        fn in_scope(&self, variable: String) -> bool {
            match self {
                Equal(a, b) => a == &variable || b == &variable,
                Sum(vars, _) => vars.contains(&variable),
            }
        }
    }
}
