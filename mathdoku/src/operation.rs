use strum::EnumIter;

use crate::types::M;

/// An arithmetic operation required by a [`Cage`](crate::Cage).
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Operation {
    /// Cells sum to the target.
    Add(M),
    /// Two cells differ by the target.
    Subtract(M),
    /// Cells multiply to the target.
    Multiply(M),
    /// Two cells have a ratio equal to the target.
    Divide(M),
    /// A single cell is fixed to the target value.
    Given(M),
}

impl Operation {
    /// Returns the operation's target value.
    pub const fn target(&self) -> M {
        match *self {
            Self::Add(t)
            | Self::Subtract(t)
            | Self::Multiply(t)
            | Self::Divide(t)
            | Self::Given(t) => t,
        }
    }
}

/// The operator portion of an [`Operation`], without an associated target.
///
/// Used by `Cage::valid_operators` to enumerate the operators legal for a cage
/// shape, and by `Cage::valid_targets` to select an operator for which to
/// enumerate legal targets.
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Hash, EnumIter, serde::Serialize, serde::Deserialize,
)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Given,
}

impl Operator {
    /// Returns the operator of `operation`.
    pub const fn of(operation: Operation) -> Self {
        match operation {
            Operation::Add(_) => Self::Add,
            Operation::Subtract(_) => Self::Subtract,
            Operation::Multiply(_) => Self::Multiply,
            Operation::Divide(_) => Self::Divide,
            Operation::Given(_) => Self::Given,
        }
    }
}

/// A feasible operator paired with every target that produces a non-empty
/// tuple set for a given polyomino on a given grid size.
///
/// Returned by [`crate::Polyomino::feasible_options`].
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CageOption {
    pub op: Operator,
    pub targets: Vec<M>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn operator_round_trips_through_json() {
        for op in Operator::iter() {
            let json = serde_json::to_string(&op).unwrap();
            let restored: Operator = serde_json::from_str(&json).unwrap();
            assert_eq!(op, restored);
        }
    }

    #[test]
    fn operation_target_returns_associated_value() {
        assert_eq!(Operation::Add(7).target(), 7);
        assert_eq!(Operation::Subtract(3).target(), 3);
        assert_eq!(Operation::Multiply(12).target(), 12);
        assert_eq!(Operation::Divide(4).target(), 4);
        assert_eq!(Operation::Given(9).target(), 9);
    }

    #[test]
    fn cage_option_round_trips_through_json() {
        let opt = CageOption {
            op: Operator::Add,
            targets: vec![3, 4, 5],
        };
        let json = serde_json::to_string(&opt).unwrap();
        let restored: CageOption = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, restored);
    }
}
