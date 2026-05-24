//! Depth-first search over a constraint problem.
//!
//! [`Search`] is a lazy iterator: each `next()` propagates the current node to a
//! fixed point, prunes on contradiction, and otherwise branches on the
//! most-constrained unsolved cell. It yields one solved [`Store`] at a time, so
//! callers can stop after the first solution (a uniqueness check) or drain it to
//! enumerate all. The viable-tuple [`TuplesCache`] is shared across the whole search.

use crate::{
    Cell, Domain,
    cache::TuplesCache,
    constraint::{Constraint, Outcome, PropagationCtx, propagate_to_fixpoint},
    store::Store,
    variable::Variable,
};

/// A lazy depth-first solver over constraints of type `C`.
pub struct Search<C: Constraint<Cell>> {
    constraints: Vec<C>,
    stack: Vec<Store>,
    cache: TuplesCache,
}

impl<C: Constraint<Cell>> Search<C> {
    /// Starts a search from `root` under `constraints`.
    pub fn new(root: Store, constraints: Vec<C>) -> Self {
        Self {
            constraints,
            stack: vec![root],
            cache: TuplesCache::default(),
        }
    }
}

impl<C: Constraint<Cell>> Iterator for Search<C> {
    type Item = Store;

    fn next(&mut self) -> Option<Store> {
        while let Some(mut store) = self.stack.pop() {
            let outcome = {
                let mut ctx = PropagationCtx::new(&mut store, &mut self.cache);
                propagate_to_fixpoint(&mut ctx, &self.constraints)
            };
            if outcome == Outcome::Contradiction || store.is_invalid() {
                continue;
            }
            // After the validity check, an all-singleton store is a solution and
            // has no cell to branch on, so `None` is exactly "solved".
            match most_constrained(&store) {
                None => return Some(store),
                Some(cell) => {
                    for value in store.get(cell.id()).iter() {
                        let mut child = store.clone();
                        child.set(cell.id(), Domain::new([value]));
                        self.stack.push(child);
                    }
                }
            }
        }
        None
    }
}

/// The unsolved cell with the smallest domain, breaking ties by row-major cell
/// order. `None` when every cell is a singleton.
fn most_constrained(store: &Store) -> Option<Cell> {
    store
        .cells()
        .map(|cell| (cell, store.get(cell.id())))
        .filter(|(_, domain)| domain.len() > 1)
        .min_by(|(a, da), (b, db)| da.len().cmp(&db.len()).then_with(|| a.cmp(b)))
        .map(|(cell, _)| cell)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn most_constrained_is_none_for_solved_store() {
        let mut store = Store::full(2);
        for row in 0..2 {
            for column in 0..2 {
                store.set(Cell::new(row, column).id(), Domain::new([1]));
            }
        }
        assert!(most_constrained(&store).is_none());
    }

    #[test]
    fn most_constrained_picks_smallest_domain() {
        let mut store = Store::full(4);
        store.set(Cell::new(0, 0).id(), Domain::new([1, 2, 3]));
        store.set(Cell::new(2, 2).id(), Domain::new([1, 2]));
        assert_eq!(most_constrained(&store), Some(Cell::new(2, 2)));
    }
}
