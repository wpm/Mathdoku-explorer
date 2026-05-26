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
