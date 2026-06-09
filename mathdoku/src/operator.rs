//! Arithmetic operators and cage operations for Mathdoku constraints.
use crate::{N, T};
use std::cmp::{max, min};
use std::ops::Div;

/// An arithmetic operation paired with a target value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticConstraint {
    /// A commutative operation and a target.
    /// Used by [`Tuples::commutative`] and [`Table::commutative`] (test utilities).
    #[allow(dead_code)]
    CommutativeConstraint(CommutativeOperator, T),
    /// A non-commutative operation and a target.
    NonCommutativeConstraint(NonCommutativeOperator, T),
}

/// A commutative, monotonically non-decreasing cage operation.
///
/// Because applying the operator to a longer tuple can only increase the result,
/// partial results can be used to prune the search for valid tuples.
///
/// Two apply methods serve different purposes:
/// - [`apply_to_tuple`](Self::apply_to_tuple) evaluates a complete tuple of cell values — used
///   for validation and table construction.
/// - [`apply_to_pair`](Self::apply_to_pair) extends an accumulated `T` by one more `T` step
///   — used during MDD traversal, where the running total is built up one node at a time.
///
/// [`NonCommutativeOperator`] has no equivalent of `apply_to_pair` because subtract and divide
/// are only valid on 2-cell cages and are solved via `Table` rather than MDD traversal, so no
/// step-by-step accumulator is needed.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum CommutativeOperator {
    Add,
    Multiply,
}
impl CommutativeOperator {
    /// Applies this operator to a tuple of values, returning the result.
    #[must_use]
    pub fn apply_to_tuple(self, ns: &[N]) -> T {
        match self {
            Self::Add => ns.iter().map(|&v| T::from(v)).sum(),
            Self::Multiply => ns.iter().map(|&v| T::from(v)).product(),
        }
    }

    /// Applies this operator to a single pair `(x, y)`.
    #[must_use]
    pub const fn apply_to_pair(self, x: T, y: T) -> T {
        match self {
            Self::Add => x + y,
            Self::Multiply => x * y,
        }
    }

    /// Returns the identity element for this operator (`0` for add, `1` for multiply).
    ///
    /// Used as the per-slot minimum bound when pruning tuple search: a partial
    /// result extended by `remaining` copies of the dual identity gives the
    /// tightest reachable lower bound on the final result.
    #[must_use]
    pub const fn identity(self) -> T {
        match self {
            Self::Add => 0,
            Self::Multiply => 1,
        }
    }

    /// Returns the dual operator (`Multiply` for `Add`, `Add` for `Multiply`).
    ///
    /// The dual's identity is the minimum value each remaining slot can contribute,
    /// forming the ring relationship used in tuple pruning.
    #[must_use]
    pub const fn dual(self) -> Self {
        match self {
            Self::Add => Self::Multiply,
            Self::Multiply => Self::Add,
        }
    }
}

/// A non-commutative cage operator whose result depends on operand order.
///
/// Applied to a pair `(a, b)` without regard to order — subtract uses absolute
/// difference and divide uses `max / min` — so the result is order-independent
/// even though the operator is not commutative in the algebraic sense.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NonCommutativeOperator {
    Subtract,
    Divide,
}

impl NonCommutativeOperator {
    /// Applies this operator to `(a, b)`, returning the result.
    ///
    /// Subtract returns `|a - b|`. Divide returns `max(a, b) / min(a, b)`
    /// using integer division.
    #[must_use]
    pub fn apply(self, a: N, b: N) -> T {
        match self {
            Self::Subtract => T::from(a.abs_diff(b)),
            Self::Divide => T::from(max(a, b).div(min(a, b))),
        }
    }
}
