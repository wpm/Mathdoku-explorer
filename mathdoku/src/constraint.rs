//! The propagation engine: the [`Constraint`] trait, the [`PropagationCtx`] it
//! mutates, and [`propagate_to_fixpoint`].
//!
//! A constraint narrows variable domains held in a [`Store`] via a
//! [`PropagationCtx`]. The concrete constraints are [`Cage`](crate::cage::Cage)
//! (tuple-based GAC) and [`AllDifferent`](crate::all_different::AllDifferent)
//! (Régin).

use std::marker::PhantomData;

use crate::{cache::TuplesCache, store::Store, variable::Variable};

/// What a single propagation step did to the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No domain changed.
    Unchanged,
    /// At least one domain was narrowed.
    Changed,
    /// A domain became empty — the (sub)problem is infeasible.
    Contradiction,
}

/// The mutable context a constraint propagates against.
///
/// Holds the [`Store`] (intrinsic state being narrowed) and the [`TuplesCache`]
/// (derived viable-tuple memo) as two *separate* mutable borrows, so a
/// constraint can read a cached tuple set and write domain reductions back to
/// the store without aliasing — store and cache never overlap.
pub struct PropagationCtx<'a, V: Variable> {
    pub store: &'a mut Store,
    pub cache: &'a mut TuplesCache,
    marker: PhantomData<V>,
}

impl<'a, V: Variable> PropagationCtx<'a, V> {
    pub const fn new(store: &'a mut Store, cache: &'a mut TuplesCache) -> Self {
        Self {
            store,
            cache,
            marker: PhantomData,
        }
    }
}

/// A constraint over variables of type `V`.
pub trait Constraint<V: Variable> {
    /// Narrows the domains in `ctx` toward consistency, reporting the effect.
    fn propagate(&self, ctx: &mut PropagationCtx<V>) -> Outcome;
}

/// Applies every constraint repeatedly until no domain changes (a fixed point)
/// or a contradiction is found.
pub fn propagate_to_fixpoint<V, C>(ctx: &mut PropagationCtx<V>, constraints: &[C]) -> Outcome
where
    V: Variable,
    C: Constraint<V>,
{
    let mut overall = Outcome::Unchanged;
    loop {
        let mut changed = false;
        for constraint in constraints {
            match constraint.propagate(ctx) {
                Outcome::Contradiction => return Outcome::Contradiction,
                Outcome::Changed => {
                    changed = true;
                    overall = Outcome::Changed;
                }
                Outcome::Unchanged => {}
            }
        }
        if !changed {
            return overall;
        }
    }
}
