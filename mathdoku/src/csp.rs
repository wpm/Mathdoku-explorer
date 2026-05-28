//! Generic constraint satisfaction problem (CSP) abstractions.
//!
//! This module defines the core traits of a CSP — [`Variable`] and [`Constraint`] —
//! and the [`generalized_arc_consistency`] algorithm that ties them together. The
//! concrete solver in [`crate::puzzle_csp`] implements these abstractions for the
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

/// A decision variable in a constraint satisfaction problem.
///
/// Each variable ranges over a domain `D` and participates in a set of constraints. The
/// associated type `C` fixes which constraint type this variable pairs with, enforcing at
/// the type level that a variable and its constraints are compatible.
///
/// The constraint graph is bipartite: variables on one side, constraints on the other,
/// with edges connecting each variable to the constraints that mention it.
pub trait Variable<C> {
    /// Returns the constraints that have this variable in their scope.
    fn constraints(&self) -> Vec<C>;
}

/// A relation over a set of variables (the constraint's scope) in a constraint satisfaction
/// problem.
///
/// A constraint is satisfied when the values assigned to its scope variables are jointly
/// consistent. Propagation enforces generalized arc consistency (GAC): it removes from each
/// variable's domain any value not supported by some consistent tuple over the scope.
pub trait Constraint<S, V, E>: Sized + Clone {
    /// Enforces GAC for this constraint against `state`.
    ///
    /// Returns the updated state and the variables whose domains were narrowed. Callers
    /// use the changed-variable list to re-activate adjacent constraints in the worklist.
    ///
    /// # Errors
    /// Returns an error if propagation fails (e.g. a cell is out of bounds).
    fn propagate(&self, state: &S) -> Result<(S, Vec<V>), E>;
}

/// Enforces generalized arc consistency (GAC) via the AC-3 worklist algorithm.
///
/// AC-3 operates on the constraint graph, a bipartite graph with variables on one side and
/// constraints on the other. It maintains a queue of constraints to process. When a constraint
/// is propagated and reduces a variable's domain, all constraints adjacent to that variable are
/// re-added to the queue. The algorithm terminates when the queue is empty, at which point the
/// state is arc-consistent: no constraint can reduce any variable's domain further.
///
/// # Errors
/// Returns the first error from any constraint's [`Constraint::propagate`] call.
pub fn generalized_arc_consistency<S, V, C, E>(state: S, constraints: &[C]) -> Result<S, E>
where
    V: Variable<C>,
    C: Constraint<S, V, E>,
{
    let mut state = state;
    let mut q: VecDeque<C> = constraints.iter().cloned().collect();
    while let Some(constraint) = q.pop_front() {
        let (new_state, variables) = constraint.propagate(&state)?;
        state = new_state;
        for v in variables {
            q.extend(v.constraints().iter().cloned());
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    //! Uses a minimal concrete CSP: state is `Vec<Vec<u32>>` (one domain per variable),
    //! variables are structs carrying an id and their constraint list, errors are `String`.
    //!
    //! Two constraint types:
    //! - `Equal(a, b)` — variables `a` and `b` must share the same value.
    //! - `Sum(vars, target)` — the named variables must sum to `target`.

    use super::{Constraint, Variable, generalized_arc_consistency};

    type State = Vec<Vec<u32>>;

    #[derive(Clone)]
    struct Var {
        #[allow(dead_code)]
        id: usize,
        constraints: Vec<Csp>,
    }

    impl Variable<Csp> for Var {
        fn constraints(&self) -> Vec<Csp> {
            self.constraints.clone()
        }
    }

    #[derive(Clone)]
    enum Csp {
        Equal(usize, usize),
        Sum(Vec<usize>, u32),
    }

    impl Constraint<State, Var, String> for Csp {
        fn propagate(&self, state: &State) -> Result<(State, Vec<Var>), String> {
            match self {
                Self::Equal(a, b) => Ok(equal_propagate(*a, *b, state)),
                Self::Sum(vars, target) => Ok(sum_propagate(vars, *target, state)),
            }
        }
    }

    fn equal_propagate(a: usize, b: usize, state: &State) -> (State, Vec<Var>) {
        let intersection: Vec<u32> = state[a]
            .iter()
            .filter(|v| state[b].contains(v))
            .copied()
            .collect();
        let mut new_state = state.clone();
        let mut changed = vec![];
        if intersection != state[a] {
            new_state[a] = intersection.clone();
            changed.push(Var {
                id: a,
                constraints: vec![Csp::Equal(a, b)],
            });
        }
        if intersection != state[b] {
            new_state[b] = intersection;
            changed.push(Var {
                id: b,
                constraints: vec![Csp::Equal(a, b)],
            });
        }
        (new_state, changed)
    }

    fn extend_sum(
        pos: usize,
        domains: &[&Vec<u32>],
        current: &mut Vec<u32>,
        target: u32,
        survivors: &mut Vec<Vec<u32>>,
    ) {
        if pos == domains.len() {
            if current.iter().sum::<u32>() == target {
                for (i, &v) in current.iter().enumerate() {
                    if !survivors[i].contains(&v) {
                        survivors[i].push(v);
                    }
                }
            }
            return;
        }
        for &v in domains[pos] {
            current.push(v);
            extend_sum(pos + 1, domains, current, target, survivors);
            let _ = current.pop();
        }
    }

    fn sum_propagate(vars: &[usize], target: u32, state: &State) -> (State, Vec<Var>) {
        let domains: Vec<&Vec<u32>> = vars.iter().map(|&i| &state[i]).collect();
        let mut survivors: Vec<Vec<u32>> = vars.iter().map(|_| vec![]).collect();
        extend_sum(0, &domains, &mut vec![], target, &mut survivors);
        let mut new_state = state.clone();
        let mut changed = vec![];
        for (i, &var) in vars.iter().enumerate() {
            if survivors[i] != *domains[i] {
                new_state[var] = survivors[i].clone();
                changed.push(Var {
                    id: var,
                    constraints: vec![Csp::Sum(vars.to_vec(), target)],
                });
            }
        }
        (new_state, changed)
    }

    fn state(domains: &[&[u32]]) -> State {
        domains.iter().map(|d| d.to_vec()).collect()
    }

    fn run(initial: State, constraints: &[Csp]) -> State {
        generalized_arc_consistency(initial, constraints).unwrap()
    }

    #[test]
    fn equal_overlapping_domains_intersects_both() {
        // x ∈ {1,2,3}, y ∈ {2,3,4}, x=y  →  both {2,3}
        let result = run(state(&[&[1, 2, 3], &[2, 3, 4]]), &[Csp::Equal(0, 1)]);
        assert_eq!(result[0], [2, 3]);
        assert_eq!(result[1], [2, 3]);
    }

    #[test]
    fn equal_singleton_pins_other_variable() {
        // x ∈ {5}, y ∈ {1,2,3,4,5}, x=y  →  both {5}
        let result = run(state(&[&[5], &[1, 2, 3, 4, 5]]), &[Csp::Equal(0, 1)]);
        assert_eq!(result[0], [5]);
        assert_eq!(result[1], [5]);
    }

    #[test]
    fn equal_disjoint_domains_empties_both() {
        // x ∈ {1,2}, y ∈ {3,4}, x=y  →  both empty (infeasible)
        let result = run(state(&[&[1, 2], &[3, 4]]), &[Csp::Equal(0, 1)]);
        assert!(result[0].is_empty());
        assert!(result[1].is_empty());
    }

    #[test]
    fn sum_two_vars_prunes_unsupported_values() {
        // x,y ∈ {1,2,3}, x+y=5  →  only (2,3),(3,2) work, so x,y ∈ {2,3}
        let mut result = run(state(&[&[1, 2, 3], &[1, 2, 3]]), &[Csp::Sum(vec![0, 1], 5)]);
        result[0].sort_unstable();
        result[1].sort_unstable();
        assert_eq!(result[0], [2, 3]);
        assert_eq!(result[1], [2, 3]);
    }

    #[test]
    fn sum_three_vars_all_values_survive() {
        // x,y,z ∈ {1,2,3}, x+y+z=6  →  permutations of (1,2,3) use every value
        let mut result = run(
            state(&[&[1, 2, 3], &[1, 2, 3], &[1, 2, 3]]),
            &[Csp::Sum(vec![0, 1, 2], 6)],
        );
        result[0].sort_unstable();
        result[1].sort_unstable();
        result[2].sort_unstable();
        assert_eq!(result[0], [1, 2, 3]);
        assert_eq!(result[1], [1, 2, 3]);
        assert_eq!(result[2], [1, 2, 3]);
    }

    #[test]
    fn sum_infeasible_target_empties_domains() {
        // x,y ∈ {1,2}, x+y=10 — impossible
        let result = run(state(&[&[1, 2], &[1, 2]]), &[Csp::Sum(vec![0, 1], 10)]);
        assert!(result[0].is_empty());
        assert!(result[1].is_empty());
    }

    #[test]
    fn propagation_chains_across_constraints() {
        // x,y,z ∈ {1,2,3}; x+y=5 pins x,y ∈ {2,3}; then x=z chains to pin z ∈ {2,3}
        let mut result = run(
            state(&[&[1, 2, 3], &[1, 2, 3], &[1, 2, 3]]),
            &[Csp::Sum(vec![0, 1], 5), Csp::Equal(0, 2)],
        );
        for d in &mut result {
            d.sort_unstable();
        }
        assert_eq!(result[0], [2, 3]);
        assert_eq!(result[1], [2, 3]);
        assert_eq!(result[2], [2, 3]);
    }
}
