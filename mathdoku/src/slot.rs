use std::cmp::Ordering;

use crate::{Cage, Cell, Error, Operation, Polyomino, cover::Cover, types::N};

/// A slot in a puzzle: either a claimed [`Polyomino`] region with no operation
/// or a fully specified [`Cage`].
///
/// `Slot` lets the library model incomplete puzzles directly, so the
/// Designer can promote a `Region` to a `Cage` (and demote it back) without
/// reaching for a parallel draft type.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Slot {
    Region(Polyomino),
    Cage(Cage),
}

impl Slot {
    /// Returns the polyomino covered by this slot, regardless of variant.
    pub const fn polyomino(&self) -> &Polyomino {
        match self {
            Self::Region(p) => p,
            Self::Cage(c) => c.polyomino(),
        }
    }

    /// Returns the inner [`Cage`] for the `Cage` variant, or `None` otherwise.
    pub const fn as_cage(&self) -> Option<&Cage> {
        if let Self::Cage(c) = self {
            Some(c)
        } else {
            None
        }
    }

    /// Returns the inner [`Polyomino`] for the `Region` variant, or `None`
    /// otherwise.
    pub const fn as_region(&self) -> Option<&Polyomino> {
        if let Self::Region(p) = self {
            Some(p)
        } else {
            None
        }
    }

    /// Returns a new [`Polyomino`] with `cell` added.
    ///
    /// Idempotent: if `cell` is already present, the polyomino is returned
    /// unchanged.
    ///
    /// # Errors
    /// Returns [`Error::DisconnectedPolyomino`] if `cell` is not
    /// edge-adjacent to any existing cell of the polyomino.
    pub fn insert_cell(&self, cell: Cell) -> Result<Polyomino, Error> {
        self.polyomino().insert(cell)
    }

    /// Returns a new [`Polyomino`] with `cell` removed, or `None` if the
    /// polyomino contained only `cell` (i.e. the slot should be deleted).
    ///
    /// Idempotent: if `cell` is not present the polyomino is returned
    /// unchanged as `Some`.
    ///
    /// # Errors
    /// Returns [`Error::WouldDisconnect`] if removing `cell` leaves the
    /// remaining cells disconnected.
    pub fn remove_cell(&self, cell: Cell) -> Result<Option<Polyomino>, Error> {
        match self.polyomino().remove(cell) {
            Ok(p) => Ok(Some(p)),
            Err(Error::RemovalWouldEmptyPolyomino(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Cover for Slot {
    fn cells(&self) -> impl Iterator<Item = Cell> {
        self.polyomino().cells()
    }
}

// `Ord` and `Eq` deliberately disagree: `Region(p)` and `Cage(c)` with the
// same polyomino compare as `Ordering::Equal` under `cmp` but are NOT `==`.
// This keeps the Designer's tab order stable across promote/demote. Do not
// store `Slot` in a `BTreeSet`/`BTreeMap` keyed on `Self`: a `Region`
// and `Cage` over the same polyomino would collide and only one would
// survive.
impl Ord for Slot {
    fn cmp(&self, other: &Self) -> Ordering {
        self.polyomino().cmp(other.polyomino())
    }
}

impl PartialOrd for Slot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Externally-tagged enum wire format. The two variant bodies are
// asymmetric: `Region` is a sequence (`Polyomino`'s own serde shape) while
// `Cage` is a struct that carries `n` so standalone deserialize can
// recompute `tuples` via `Cage::new`.
impl serde::Serialize for Slot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStructVariant;
        match self {
            Self::Region(p) => s.serialize_newtype_variant("Slot", 0, "Region", p),
            Self::Cage(c) => {
                let mut sv = s.serialize_struct_variant("Slot", 1, "Cage", 3)?;
                sv.serialize_field("polyomino", c.polyomino())?;
                sv.serialize_field("operation", &c.operation())?;
                sv.serialize_field("n", &c.n())?;
                sv.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Slot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        enum Wire {
            Region(Polyomino),
            Cage {
                polyomino: Polyomino,
                operation: Operation,
                n: N,
            },
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Region(p) => Self::Region(p),
            Wire::Cage {
                polyomino,
                operation,
                n,
            } => Self::Cage(Cage::new(n, polyomino, operation)),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::test_utils::{c00, c01, c02, pair, singleton};

    // --- Accessors ---

    #[test]
    fn as_cage_returns_some_for_cage_variant_and_none_for_region() {
        let cage = Cage::new(4, singleton(), Operation::Given(3));
        assert_eq!(Slot::Cage(cage.clone()).as_cage(), Some(&cage));
        assert_eq!(Slot::Region(singleton()).as_cage(), None);
    }

    #[test]
    fn as_region_returns_some_for_region_variant_and_none_for_cage() {
        let p = singleton();
        assert_eq!(Slot::Region(p.clone()).as_region(), Some(&p));
        let cage = Cage::new(4, singleton(), Operation::Given(1));
        assert_eq!(Slot::Cage(cage).as_region(), None);
    }

    #[test]
    fn polyomino_returns_inner_polyomino_for_both_variants() {
        let p = pair();
        assert_eq!(Slot::Region(p.clone()).polyomino(), &p);
        let cage = Cage::new(4, p.clone(), Operation::Add(6));
        assert_eq!(Slot::Cage(cage).polyomino(), &p);
    }

    // --- Cover ---

    #[test]
    fn cover_cells_match_polyomino_cells_for_both_variants() {
        let p = pair();
        let expected: Vec<Cell> = p.cells().collect();
        let region = Slot::Region(p.clone());
        let slot = Slot::Cage(Cage::new(4, p, Operation::Add(6)));
        assert_eq!(region.cells().collect::<Vec<_>>(), expected);
        assert_eq!(slot.cells().collect::<Vec<_>>(), expected);
    }

    // --- Ord / PartialOrd ---

    #[test]
    fn cmp_equal_across_variants_with_same_polyomino() {
        let region = Slot::Region(singleton());
        let cage = Slot::Cage(Cage::new(4, singleton(), Operation::Given(1)));
        // Tab order is stable across promote/demote.
        assert_eq!(region.cmp(&cage), Ordering::Equal);
        // But the variants are not value-equal: documents the intentional
        // Ord/Eq divergence.
        assert_ne!(region, cage);
    }

    #[test]
    fn cmp_orders_by_polyomino_ignoring_variant() {
        let region_small = Slot::Region(singleton());
        let cage_large = Slot::Cage(Cage::new(4, pair(), Operation::Add(3)));
        assert!(region_small < cage_large);

        let cage_small = Slot::Cage(Cage::new(4, singleton(), Operation::Given(1)));
        let region_large = Slot::Region(pair());
        assert!(cage_small < region_large);
    }

    #[test]
    fn partial_cmp_consistent_with_cmp() {
        let a = Slot::Region(singleton());
        let b = Slot::Region(pair());
        assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));
    }

    // --- Serde round-trip ---

    #[test]
    fn region_round_trips_through_json() {
        let original = Slot::Region(pair());
        let json = serde_json::to_string(&original).unwrap();
        let restored: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn cage_round_trips_through_json() {
        let cage = Cage::new(4, pair(), Operation::Add(6));
        let original = Slot::Cage(cage);
        let json = serde_json::to_string(&original).unwrap();
        let restored: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert_eq!(
            restored.as_cage().unwrap().tuples(),
            original.as_cage().unwrap().tuples(),
        );
    }

    #[test]
    fn cage_deserialize_missing_n_returns_err() {
        // `n` is mandatory on the wire for the Cage variant.
        let json = r#"{"Cage":{"polyomino":[{"row":0,"column":0}],"operation":{"Given":3}}}"#;
        assert!(serde_json::from_str::<Slot>(json).is_err());
    }

    // --- insert_cell / remove_cell ---

    #[test]
    fn insert_cell_on_region_adds_cell() {
        let slot = Slot::Region(singleton());
        let p = slot.insert_cell(c01()).unwrap();
        assert!(p.contains(c01()));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn insert_cell_on_cage_adds_cell() {
        let slot = Slot::Cage(Cage::new(4, singleton(), Operation::Given(3)));
        let p = slot.insert_cell(c01()).unwrap();
        assert!(p.contains(c01()));
    }

    #[test]
    fn insert_cell_non_adjacent_returns_err() {
        let slot = Slot::Region(singleton());
        assert!(matches!(
            slot.insert_cell(c02()),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn remove_cell_from_pair_returns_some() {
        let slot = Slot::Region(pair());
        let result = slot.remove_cell(c01()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn remove_cell_from_cage_pair_returns_some() {
        let slot = Slot::Cage(Cage::new(4, pair(), Operation::Add(3)));
        let result = slot.remove_cell(c01()).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn remove_cell_from_singleton_returns_none() {
        let slot = Slot::Region(singleton());
        assert!(slot.remove_cell(c00()).unwrap().is_none());
    }

    #[test]
    fn remove_cell_would_disconnect_returns_err() {
        // row of 3: removing middle disconnects
        let row3 = Polyomino::from_cells(&[c00(), c01(), c02()]).unwrap();
        let slot = Slot::Region(row3);
        assert!(matches!(
            slot.remove_cell(c01()),
            Err(Error::WouldDisconnect(_))
        ));
    }

    // Locks in the wire-format contract: future changes that drift the
    // shape (variant tags, field names, sequence vs. struct for Region) will
    // break this test.
    #[test]
    fn serializes_to_externally_tagged_shape() {
        let region = Slot::Region(singleton());
        assert_eq!(
            serde_json::to_value(&region).unwrap(),
            serde_json::json!({"Region": [{"row": 0, "column": 0}]}),
        );
        let cage = Slot::Cage(Cage::new(4, singleton(), Operation::Given(3)));
        assert_eq!(
            serde_json::to_value(&cage).unwrap(),
            serde_json::json!({
                "Cage": {
                    "polyomino": [{"row": 0, "column": 0}],
                    "operation": {"Given": 3},
                    "n": 4,
                }
            }),
        );
    }
}
