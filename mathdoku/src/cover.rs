//! The [`Cover`] trait: an ordered set of grid cells.

use crate::Cell;

/// A set of [`Cell`]s in row-major order.
pub trait Cover {
    /// The covering [`Cell`]s in row-major order.
    ///
    /// Implementations must be cheap to call repeatedly; the default `len` and
    /// `is_empty` methods each invoke `cells()` independently.
    fn cells(&self) -> impl Iterator<Item = Cell>;

    // Concrete types provide faster inherent `len`s that shadow this default;
    // the trait method remains for generic `<T: Cover>` callers and is exercised
    // through `AllDifferent` in tests.
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.cells().count()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.cells().next().is_none()
    }
}
