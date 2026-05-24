use crate::{
    Cell, Domain, Operation, Polyomino,
    cache::viable_tuples,
    constraint::{Constraint, Outcome, PropagationCtx},
    store::Narrowed,
    types::N,
    variable::Variable,
};

/// An ordered assignment of values to the cells of a cage, one value per cell.
pub type Tuple = Vec<N>;

/// A polyomino-shaped constraint whose cell values satisfy an arithmetic
/// condition.
///
/// A `Cage` is an *immutable constraint definition*: it describes which cells it
/// covers, the operation they must satisfy, and the grid size — and nothing
/// more. It deliberately carries **no** tuple state. The set of value tuples
/// consistent with the operation is a pure function of the cage
/// ([`Cage::tuples`]); the subset still viable under a given store is obtained
/// through the cache's `viable_tuples`, which memoizes separately. This keeps
/// derived tuple data off the constraint definition.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Cage {
    polyomino: Polyomino,
    operation: Operation,
    n: N,
}

impl Cage {
    /// Creates a cage over `polyomino` whose cells must satisfy `operation` on an
    /// `n`×`n` grid.
    pub const fn new(n: N, polyomino: Polyomino, operation: Operation) -> Self {
        Self {
            polyomino,
            operation,
            n,
        }
    }

    /// Returns the polyomino covered by this cage.
    pub const fn polyomino(&self) -> &Polyomino {
        &self.polyomino
    }

    /// Returns the grid size this cage was constructed for.
    pub const fn n(&self) -> N {
        self.n
    }

    /// Iterates this cage's cells in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.polyomino.cells()
    }

    /// Returns the number of cells in this cage. Always at least 1.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.polyomino.len()
    }

    /// Returns this cage's operation.
    pub const fn operation(&self) -> Operation {
        self.operation
    }

    /// All ordered tuples that satisfy the operation, before any store-dependent
    /// pruning. A pure function of the cage definition — computed on demand,
    /// never stored.
    pub fn tuples(&self) -> Vec<Tuple> {
        self.polyomino.valid_tuples(self.n, self.operation)
    }
}

impl Constraint<Cell> for Cage {
    /// Tuple-based generalized arc consistency: read the viable tuple set (pure,
    /// cached), union the supported value at each cell position, and narrow each
    /// cell's domain to that union. Writes only domain reductions to the store.
    fn propagate(&self, ctx: &mut PropagationCtx<Cell>) -> Outcome {
        let unions = {
            let viable = viable_tuples(self, ctx.store, ctx.cache);
            let mut unions = vec![Domain::default(); self.len()];
            for tuple in viable {
                for (slot, &value) in unions.iter_mut().zip(tuple) {
                    *slot = *slot | Domain::new([value]);
                }
            }
            unions
        };
        let mut outcome = Outcome::Unchanged;
        for (cell, union) in self.cells().zip(unions) {
            match ctx.store.intersect(cell.id(), union) {
                Narrowed::Empty => return Outcome::Contradiction,
                Narrowed::Changed => outcome = Outcome::Changed,
                Narrowed::Unchanged => {}
            }
        }
        outcome
    }
}

impl serde::Serialize for Cage {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // `n` and the cell list are not serialized: `n` lives at the Puzzle
        // level, and the cells are derived from `polyomino`.
        let mut st = s.serialize_struct("Cage", 2)?;
        st.serialize_field("polyomino", &self.polyomino)?;
        st.serialize_field("operation", &self.operation)?;
        st.end()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{
        cache::TuplesCache,
        store::Store,
        test_utils::{l_shape, pair, singleton},
    };

    #[test]
    fn cage_new_given_singleton() {
        let cage = Cage::new(4, singleton(), Operation::Given(3));
        assert_eq!(cage.tuples(), vec![vec![3u8]]);
    }

    #[test]
    fn cage_new_add_prunes_horizontal_pair() {
        let cage = Cage::new(4, pair(), Operation::Add(6));
        let mut tuples = cage.tuples();
        tuples.sort_unstable();
        assert_eq!(tuples, vec![vec![2u8, 4], vec![4, 2]]);
    }

    #[test]
    fn cage_new_add_prunes_l_shape() {
        let cage = Cage::new(4, l_shape(), Operation::Add(6));
        assert_eq!(cage.tuples().len(), 7);
        assert!(!cage.tuples().contains(&vec![1u8, 1, 4]));
        assert!(!cage.tuples().contains(&vec![2, 2, 2]));
    }

    #[test]
    fn cells_and_len_track_polyomino() {
        let cage = Cage::new(4, pair(), Operation::Add(3));
        assert_eq!(
            cage.cells().collect::<Vec<_>>(),
            vec![Cell::new(0, 0), Cell::new(0, 1)]
        );
        assert_eq!(cage.len(), 2);
    }

    #[test]
    fn operation_and_polyomino_accessors() {
        let cage = Cage::new(4, singleton(), Operation::Given(3));
        assert_eq!(cage.operation(), Operation::Given(3));
        assert_eq!(cage.polyomino(), &singleton());
        assert_eq!(cage.n(), 4);
    }

    #[test]
    fn partial_cmp_orders_by_polyomino() {
        let a = Cage::new(4, singleton(), Operation::Given(1));
        let b = Cage::new(4, pair(), Operation::Add(3));
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn propagate_narrows_to_supported_values() {
        let cage = Cage::new(4, pair(), Operation::Add(3));
        let mut store = Store::full(4);
        let mut cache = TuplesCache::default();
        let outcome = {
            let mut ctx = PropagationCtx::new(&mut store, &mut cache);
            cage.propagate(&mut ctx)
        };
        assert_eq!(outcome, Outcome::Changed);
        assert_eq!(store.get(Cell::new(0, 0).id()), Domain::new([1, 2]));
        assert_eq!(store.get(Cell::new(0, 1).id()), Domain::new([1, 2]));
    }

    #[test]
    fn propagate_unchanged_when_no_reduction() {
        // Multiply over a same-row pair on n=2: tuples {1,2},{2,1} already span
        // the full domain, so a fresh store is not narrowed.
        let cage = Cage::new(2, pair(), Operation::Multiply(2));
        let mut store = Store::full(2);
        let mut cache = TuplesCache::default();
        let mut ctx = PropagationCtx::new(&mut store, &mut cache);
        assert_eq!(cage.propagate(&mut ctx), Outcome::Unchanged);
    }

    #[test]
    fn propagate_detects_contradiction() {
        let cage = Cage::new(4, pair(), Operation::Add(3));
        let mut store = Store::full(4);
        store.set(Cell::new(0, 0).id(), Domain::new([4]));
        store.set(Cell::new(0, 1).id(), Domain::new([4]));
        let mut cache = TuplesCache::default();
        let mut ctx = PropagationCtx::new(&mut store, &mut cache);
        assert_eq!(cage.propagate(&mut ctx), Outcome::Contradiction);
    }

    #[test]
    fn cage_with_target_above_value_max_yields_no_tuples() {
        // A target above the per-cell value range makes the value conversion
        // fail, so no tuples are produced.
        for (p, op) in [
            (singleton(), Operation::Given(300)),
            (pair(), Operation::Subtract(300)),
            (pair(), Operation::Divide(300)),
            (pair(), Operation::Add(300)),
        ] {
            assert!(Cage::new(4, p, op).tuples().is_empty());
        }
    }

    #[test]
    fn cage_serializes_to_polyomino_and_operation_without_n() {
        let cage = Cage::new(4, singleton(), Operation::Given(3));
        assert_eq!(
            serde_json::to_value(&cage).unwrap(),
            serde_json::json!({
                "polyomino": [{"row": 0, "column": 0}],
                "operation": {"Given": 3},
            }),
        );
    }
}
