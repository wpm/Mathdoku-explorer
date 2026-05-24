//! The constraint-satisfaction [`Variable`] trait, implemented on [`Cell`].

use std::hash::Hash;

use crate::{Cell, types::N};

/// Identifier for a constraint-satisfaction variable.
///
/// A thin newtype over [`Cell`]: in Mathdoku the variables *are* the grid cells,
/// so the identifier carries the cell coordinates directly. [`Store`] keys its
/// domains by `VarId`.
///
/// [`Store`]: crate::store::Store
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarId(pub Cell);

/// A constraint-satisfaction variable: a stable identity with a value type drawn
/// from a finite domain.
pub trait Variable {
    type Value: Copy + Eq + Hash;
    fn id(&self) -> VarId;
}

impl Variable for Cell {
    type Value = N;

    fn id(&self) -> VarId {
        VarId(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_id_round_trips() {
        let cell = Cell::new(2, 3);
        assert_eq!(cell.id(), VarId(cell));
    }
}
