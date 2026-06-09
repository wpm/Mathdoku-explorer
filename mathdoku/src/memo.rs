//! Traits for building and narrowing cage constraint representations.
//!
//! [`Memo`] constructs a representation of all value tuples satisfying a cage's
//! arithmetic constraint. `Narrow` filters that representation when external
//! information (e.g. from grid-level constraints) rules out certain values.
//!
//! Both traits are implemented by `Table`, which
//! stores tuples explicitly, and will be implemented by `Mdd`, which stores
//! them as a multivalued decision diagram.
use crate::Error;
use crate::fill::Fill;
/// A cage constraint representation that can be constructed from an arithmetic operation.
///
/// Implementors store the set of value tuples satisfying the constraint and
/// expose per-position candidate sets via [`fill`](Memo::get).
pub trait Memo: Sized {
    /// Returns the candidate value set for position `index`.
    ///
    /// The candidate set is the union of values that appear at `index`
    /// across all tuples in the representation.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCellCageIndex`] if `index` is out of range.
    fn get(&self, index: usize) -> Result<Fill, Error>;

    /// Returns a new representation containing only the tuples where every
    /// position's value is present in the corresponding `Fill`.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if no tuples survive the filter.
    fn narrow(&self, support: &[Fill]) -> Result<Self, Error>;
}
