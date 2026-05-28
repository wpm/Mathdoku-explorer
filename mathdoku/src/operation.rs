//! Arithmetic operators, operations, and operator enumeration for cages.
//!
//! A Mathdoku cage constrains its cells with an [`Operation`]: one of the five
//! [`Operator`]s paired with a numeric target drawn from the grid's solved values.
//! The five operators and their arities are:
//!
//! | Operator | Arity | Constraint |
//! |----------|-------|------------|
//! | [`Add`](Operator::Add) | ≥ 1 | cells sum to target |
//! | [`Subtract`](Operator::Subtract) | exactly 2 | cells differ by target |
//! | [`Multiply`](Operator::Multiply) | ≥ 1 | cells multiply to target |
//! | [`Divide`](Operator::Divide) | exactly 2 | cells have ratio equal to target |
//! | [`Given`](Operator::Given) | exactly 1 | cell equals target |
//!
//! The [`operators`] function returns the valid operators for a cage based on its
//! polyomino's size: singletons allow only `Given`; pairs allow all four binary
//! operators; larger cages allow only `Add` and `Multiply`.

use crate::{Polyomino, Target};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::{Display, Formatter};

/// An [`Operator`] paired with a numeric target value imposed on a cage's cells.
///
/// Displayed as the operator symbol followed by the target (e.g. `+5`, `×12`).
/// A [`Given`](Operator::Given) operation displays as just the target with no symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Operation {
    /// The arithmetic operator applied to the cage's cells.
    pub operator: Operator,
    /// The numeric target the operator must reach.
    pub target: Target,
}

impl Operation {
    /// Creates an operation from an operator and a target value.
    pub const fn new(operator: Operator, target: Target) -> Self {
        Self { operator, target }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.operator, self.target)
    }
}

/// The arithmetic operation a cage imposes on its cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operator {
    /// Cells sum to the target.
    Add,
    /// Two cells differ by the target.
    Subtract,
    /// Cells multiply to the target.
    Multiply,
    /// Two cells have a ratio equal to the target.
    Divide,
    /// A single cell is fixed to the target value.
    Given,
}

impl Display for Operator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Add => "+",
            Self::Subtract => "−",
            Self::Multiply => "×",
            Self::Divide => "÷",
            Self::Given => "",
        };
        write!(f, "{s}")
    }
}

/// Returns the operators valid for a cage of the given polyomino's size.
///
/// - 1 cell: [`Operator::Given`] only.
/// - 2 cells: all four binary operators.
/// - 3+ cells: [`Operator::Add`] and [`Operator::Multiply`] only
///   (subtraction and division are undefined for more than two operands).
pub fn operators(polynomial: &Polyomino) -> Vec<Operator> {
    match polynomial.len() {
        1 => vec![Operator::Given],
        2 => vec![
            Operator::Add,
            Operator::Subtract,
            Operator::Multiply,
            Operator::Divide,
        ],
        _ => vec![Operator::Add, Operator::Multiply],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{l_shape, pair, singleton};

    #[test]
    fn operation_display_add() {
        assert_eq!(Operation::new(Operator::Add, 5).to_string(), "+5");
    }

    #[test]
    fn operation_display_subtract() {
        assert_eq!(Operation::new(Operator::Subtract, 2).to_string(), "−2");
    }

    #[test]
    fn operation_display_multiply() {
        assert_eq!(Operation::new(Operator::Multiply, 12).to_string(), "×12");
    }

    #[test]
    fn operation_display_divide() {
        assert_eq!(Operation::new(Operator::Divide, 3).to_string(), "÷3");
    }

    #[test]
    fn operation_display_given_shows_only_target() {
        assert_eq!(Operation::new(Operator::Given, 7).to_string(), "7");
    }

    #[test]
    fn operators_singleton() {
        assert_eq!(operators(&singleton()), vec![Operator::Given]);
    }

    #[test]
    fn operators_pair() {
        let ops = operators(&pair());
        assert!(ops.contains(&Operator::Add));
        assert!(ops.contains(&Operator::Subtract));
        assert!(ops.contains(&Operator::Multiply));
        assert!(ops.contains(&Operator::Divide));
        assert!(!ops.contains(&Operator::Given));
    }

    #[test]
    fn operators_large() {
        let ops = operators(&l_shape());
        assert!(ops.contains(&Operator::Add));
        assert!(ops.contains(&Operator::Multiply));
        assert!(!ops.contains(&Operator::Subtract));
        assert!(!ops.contains(&Operator::Divide));
        assert!(!ops.contains(&Operator::Given));
    }
}
