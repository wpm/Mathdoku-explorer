//! The [`Puzzle`] type: an `n×n` grid with cage constraints (no cell values).

// `Cage` caches its MDD behind a `OnceLock`, giving it interior mutability, but
// its `Ord`/`Eq`/`Hash` impls depend only on the polyomino and operation — never
// the cache — so using it as a `BTreeSet` key is sound.
#![allow(clippy::mutable_key_type)]

use crate::Error::InvalidGridSize;
use crate::Error::RegionConflict;
use crate::cage::Cage;
use crate::{Error, Polyomino};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

// Serde wire format.
#[derive(Serialize, Deserialize)]
struct PuzzleWire {
    n: usize,
    #[serde(default)]
    cages: BTreeSet<Cage>,
}

/// An `n×n` Mathdoku puzzle defined by its cage constraints.
///
/// A `Puzzle` stores only the structural information — the grid size and the set
/// of cages — without any cell value information. Cell values live in [`Grid`].
///
/// [`Grid`]: crate::Grid
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Puzzle {
    n: usize,
    cages: BTreeSet<Cage>,
}

impl Puzzle {
    /// Creates an empty `n×n` puzzle with no cages.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `n` is not in `1..=9`.
    pub fn new(n: usize) -> Result<Self, Error> {
        if !(1..=9).contains(&n) {
            return Err(InvalidGridSize(n));
        }
        Ok(Self {
            n,
            cages: BTreeSet::new(),
        })
    }

    /// Returns the grid size `n` (puzzle is `n`×`n`).
    pub const fn n(&self) -> usize {
        self.n
    }

    /// Returns an iterator over all cages in this puzzle in polyomino order.
    pub fn cages(&self) -> impl Iterator<Item = &Cage> {
        self.cages.iter()
    }

    /// Returns a new puzzle with `cage` added.
    ///
    /// Returns `Ok(self.clone())` if `cage` is already present. Does not
    /// propagate constraints — call [`Grid::constrain`] separately to apply the
    /// new cage's constraints to a grid.
    ///
    /// [`Grid::constrain`]: crate::Grid::constrain
    ///
    /// # Errors
    /// Returns [`RegionConflict`] if `cage`'s polyomino overlaps an
    /// existing cage's polyomino (but not if the cage is already present).
    pub fn insert_cage(&self, cage: Cage) -> Result<Self, Error> {
        // If the cage is already present, return a clone without error.
        if self.cages.contains(&cage) {
            return Ok(self.clone());
        }
        let polyomino = cage.polyomino();
        if self.intersects_cage(polyomino) {
            return Err(RegionConflict(polyomino.clone()));
        }
        let mut cages = self.cages.clone();
        let _ = cages.insert(cage);
        Ok(Self { n: self.n, cages })
    }

    /// Returns a new puzzle with `cage` removed.
    ///
    /// Returns `self` unchanged if `cage` is not present. Does not re-propagate
    /// constraints — call [`Grid::loosen`] separately to widen the affected cells.
    ///
    /// [`Grid::loosen`]: crate::Grid::loosen
    #[must_use]
    pub fn remove_cage(&self, cage: &Cage) -> Self {
        let mut cages = self.cages.clone();
        let _ = cages.remove(cage);
        Self { n: self.n, cages }
    }

    fn intersects_cage(&self, polyomino: &Polyomino) -> bool {
        self.cages
            .iter()
            .any(|cage| cage.polyomino().intersects(polyomino))
    }
}

impl Serialize for Puzzle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        PuzzleWire {
            n: self.n,
            cages: self.cages.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Puzzle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = PuzzleWire::deserialize(d)?;
        let n = wire.n;
        if !(1..=9).contains(&n) {
            return Err(DeError::custom(format!("invalid grid size {n}")));
        }
        Ok(Self {
            n,
            cages: wire.cages,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::{from_str, to_string};

    use super::*;
    use crate::Target;
    use crate::cage::Cage;
    use crate::operation::Operator::{Add, Given};
    use crate::operation::{Operation, Operator};
    use crate::polyomino::Polyomino;

    fn cage_at(positions: &[(usize, usize)], operator: Operator, target: Target) -> Cage {
        let cells: Vec<crate::Cell> = positions
            .iter()
            .map(|&(r, c)| crate::Cell::new(r, c))
            .collect();
        let poly = Polyomino::from_cells(&cells).unwrap();
        Cage::new(poly, Operation::new(operator, target))
    }

    // --- Puzzle::new ---

    #[test]
    fn new_valid_sizes_succeed() {
        for n in 1..=9 {
            assert!(Puzzle::new(n).is_ok(), "size {n} should succeed");
        }
    }

    #[test]
    fn new_size_zero_returns_err() {
        assert!(matches!(Puzzle::new(0), Err(InvalidGridSize(0))));
    }

    #[test]
    fn new_size_ten_returns_err() {
        assert!(matches!(Puzzle::new(10), Err(InvalidGridSize(10))));
    }

    #[test]
    fn new_has_no_cages() {
        let p = Puzzle::new(4).unwrap();
        assert_eq!(p.cages().count(), 0);
    }

    // --- Puzzle::insert_cage ---

    #[test]
    fn insert_cage_returns_puzzle() {
        let p = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Given, 3);
        let p2 = p.insert_cage(cage).unwrap();
        assert_eq!(p2.n(), 4);
    }

    #[test]
    fn insert_cage_is_non_destructive() {
        let p = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Given, 3);
        let _ = p.insert_cage(cage);
        // Original puzzle unchanged — still has no cages.
        assert_eq!(p.cages().count(), 0);
    }

    #[test]
    fn insert_cage_duplicate_returns_self() {
        let p = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Given, 3);
        let p2 = p.insert_cage(cage.clone()).unwrap();
        let p3 = p2.insert_cage(cage).unwrap();
        assert_eq!(p2, p3);
    }

    #[test]
    fn insert_cage_overlap_returns_region_conflict() {
        // A cage at (0,0)+(0,1) is already present; inserting a cage that
        // shares cell (0,0) with a *different* polyomino is a region conflict.
        let p = Puzzle::new(4)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Add, 3))
            .unwrap();
        // This cage shares cell (0,0) with the existing cage but has a different polyomino.
        let overlapping = cage_at(&[(0, 0)], Given, 1);
        assert!(matches!(p.insert_cage(overlapping), Err(RegionConflict(_))));
    }

    #[test]
    fn insert_cage_accumulates_cages() {
        let p = Puzzle::new(4).unwrap();
        let c1 = cage_at(&[(0, 0)], Given, 1);
        let c2 = cage_at(&[(0, 1)], Given, 2);
        let p3 = p.insert_cage(c1).unwrap().insert_cage(c2).unwrap();
        assert_eq!(p3.cages().count(), 2);
    }

    // --- Puzzle::remove_cage ---

    #[test]
    fn remove_cage_removes_present_cage() {
        let cage = cage_at(&[(0, 0)], Given, 1);
        let p = Puzzle::new(4).unwrap().insert_cage(cage.clone()).unwrap();
        let p2 = p.remove_cage(&cage);
        assert_eq!(p2.cages().count(), 0);
    }

    #[test]
    fn remove_cage_absent_returns_self() {
        let cage = cage_at(&[(0, 0)], Given, 1);
        let p = Puzzle::new(4).unwrap();
        let p2 = p.remove_cage(&cage);
        assert_eq!(p, p2);
    }

    #[test]
    fn remove_cage_is_non_destructive() {
        let cage = cage_at(&[(0, 0)], Given, 1);
        let p = Puzzle::new(4).unwrap().insert_cage(cage.clone()).unwrap();
        let _ = p.remove_cage(&cage);
        assert_eq!(p.cages().count(), 1);
    }

    // --- Puzzle::cages ---

    #[test]
    fn cages_returns_all_inserted_cages() {
        let c1 = cage_at(&[(0, 0)], Given, 1);
        let c2 = cage_at(&[(0, 1)], Add, 2);
        let p = Puzzle::new(4)
            .unwrap()
            .insert_cage(c1.clone())
            .unwrap()
            .insert_cage(c2.clone())
            .unwrap();
        let cages: Vec<_> = p.cages().cloned().collect();
        assert!(cages.contains(&c1));
        assert!(cages.contains(&c2));
    }

    // --- serde round-trip ---

    #[test]
    fn puzzle_round_trips_through_json() {
        let p = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Add, 3))
            .unwrap()
            .insert_cage(cage_at(&[(0, 2)], Given, 2))
            .unwrap();
        let json = to_string(&p).unwrap();
        let restored: Puzzle = from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    #[test]
    fn puzzle_deserialize_invalid_n_returns_err() {
        let json = r#"{"n":0,"cages":[]}"#;
        assert!(from_str::<Puzzle>(json).is_err());
        let json = r#"{"n":10,"cages":[]}"#;
        assert!(from_str::<Puzzle>(json).is_err());
    }
}
