//! The **cache**: derived viable-tuple sets, memoized.
//!
//! [`viable_tuples`] is a **pure function** of `(cage, projected domains)`. The
//! cache exists for performance only and is never the source of truth:
//!
//! - Static tuples are memoized per cage; viable sets are memoized per `(cage, projected domains)`.
//! - The key projects onto only the cage's cells' current domains, so an entry is reusable across
//!   stores that agree on that subset (and so naturally shared across search-tree nodes that agree
//!   there).
//! - It is never the source of truth: an independent empty cache yields the identical result, so
//!   the cache only changes performance, never observable behavior.

use std::{collections::HashMap, sync::Arc};

use crate::{Domain, Tuple, cage::Cage, store::Store, types::N, variable::Variable};

/// A set of viable ordered tuples for a cage.
pub type TupleSet = Vec<Tuple>;

/// A set of viable unordered multisets for a cage (each inner `Tuple` is sorted).
pub type MultisetSet = Vec<Tuple>;

/// Memo key: a cage plus its cells' current domains, projected to value lists so
/// two stores agreeing on the cage's cells hit the same entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ViableKey {
    cage: Cage,
    projection: Vec<Vec<N>>,
}

/// Derived state, populated lazily. Three memos: a cage's full static tuple set
/// (independent of the store), the viable ordered-tuple subset under a given
/// projection, and the viable unordered-multiset subset under a given projection.
#[derive(Debug, Default, Clone)]
pub struct TuplesCache {
    static_tuples: HashMap<Cage, Arc<[Tuple]>>,
    viable_tuples: HashMap<ViableKey, TupleSet>,
    viable_multisets: HashMap<ViableKey, MultisetSet>,
}

impl TuplesCache {
    #[cfg(test)]
    pub fn viable_tuple_entry_count(&self) -> usize {
        self.viable_tuples.len()
    }

    fn get_static_tuples(&mut self, cage: &Cage) -> Arc<[Tuple]> {
        if let Some(cached) = self.static_tuples.get(cage) {
            return Arc::clone(cached);
        }
        let computed: Arc<[Tuple]> = Arc::from(cage.tuples());
        let _ = self
            .static_tuples
            .insert(cage.clone(), Arc::clone(&computed));
        computed
    }
}

fn projection(cage: &Cage, store: &Store) -> Vec<Vec<N>> {
    cage.cells()
        .map(|cell| store.get(cell.id()).iter().collect())
        .collect()
}

fn filter_viable(statics: &[Tuple], domains: &[Domain]) -> TupleSet {
    statics
        .iter()
        .filter(|tuple| {
            tuple
                .iter()
                .zip(domains)
                .all(|(&value, domain)| !(*domain & Domain::new([value])).is_empty())
        })
        .cloned()
        .collect()
}

/// Returns the viable ordered tuples for `cage` under `store`, memoized in `cache`.
///
/// Pure: for the same cage and the same store contents over the cage's cells it
/// always returns the same set, regardless of cache state.
pub fn viable_tuples<'c>(cage: &Cage, store: &Store, cache: &'c mut TuplesCache) -> &'c TupleSet {
    let key = ViableKey {
        cage: cage.clone(),
        projection: projection(cage, store),
    };
    if !cache.viable_tuples.contains_key(&key) {
        let statics = cache.get_static_tuples(cage);
        let domains: Vec<Domain> = cage.cells().map(|cell| store.get(cell.id())).collect();
        let filtered = filter_viable(&statics, &domains);
        let _ = cache.viable_tuples.insert(key.clone(), filtered);
    }
    &cache.viable_tuples[&key]
}

/// Returns the viable unordered multisets for `cage` under `store`, memoized in `cache`.
///
/// Each entry is a sorted `Tuple`; permutations of the same values appear once.
/// Pure: for the same cage and store projection, it always returns the same set.
pub fn viable_multisets<'c>(
    cage: &Cage,
    store: &Store,
    cache: &'c mut TuplesCache,
) -> &'c MultisetSet {
    let key = ViableKey {
        cage: cage.clone(),
        projection: projection(cage, store),
    };
    if !cache.viable_multisets.contains_key(&key) {
        let tuples = viable_tuples(cage, store, cache);
        let mut seen: std::collections::HashSet<Tuple> = std::collections::HashSet::new();
        let multisets: MultisetSet = tuples
            .iter()
            .filter_map(|t| {
                let mut sorted = t.clone();
                sorted.sort_unstable();
                seen.insert(sorted.clone()).then_some(sorted)
            })
            .collect();
        let _ = cache.viable_multisets.insert(key.clone(), multisets);
    }
    &cache.viable_multisets[&key]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{Cell, Operation, Polyomino};

    fn cage() -> Cage {
        let poly = Polyomino::from_cells(&[Cell::new(0, 0), Cell::new(0, 1)]).unwrap();
        Cage::new(4, poly, Operation::Add(4))
    }

    #[test]
    fn viable_tuples_filters_by_current_domains() {
        let cage = cage();
        let mut store = Store::full(4);
        // Pin (0,0) to {1}: only [1,3] survives for Add(4) on a row pair.
        store.set(Cell::new(0, 0).id(), Domain::new([1]));
        let mut cache = TuplesCache::default();
        let viable = viable_tuples(&cage, &store, &mut cache).clone();
        assert_eq!(viable, vec![vec![1u8, 3]]);
    }

    #[test]
    fn cache_is_a_pure_memo() {
        let cage = cage();
        let store = Store::full(4);

        // A repeated call on the same cache is a hit and yields the same value.
        let mut cache = TuplesCache::default();
        let first = viable_tuples(&cage, &store, &mut cache).clone();
        let second = viable_tuples(&cage, &store, &mut cache).clone();
        assert_eq!(cache.viable_tuple_entry_count(), 1);
        assert_eq!(first, second);

        // An independent, empty cache produces the identical value: the cache
        // affects only performance, never the result.
        let mut fresh = TuplesCache::default();
        assert_eq!(first, *viable_tuples(&cage, &store, &mut fresh));

        // And both equal a fresh, uncached pure computation.
        let domains: Vec<Domain> = cage.cells().map(|c| store.get(c.id())).collect();
        assert_eq!(first, filter_viable(&cage.tuples(), &domains));
    }

    #[test]
    fn distinct_projections_get_distinct_entries() {
        let cage = cage();
        let mut cache = TuplesCache::default();

        let full = Store::full(4);
        let _ = viable_tuples(&cage, &full, &mut cache);

        let mut narrowed = Store::full(4);
        narrowed.set(Cell::new(0, 0).id(), Domain::new([1]));
        let _ = viable_tuples(&cage, &narrowed, &mut cache);

        assert_eq!(cache.viable_tuple_entry_count(), 2);
    }

    #[test]
    fn viable_multisets_deduplicates_permutations() {
        // Add(4) on a same-row pair: tuples are [1,3] and [3,1], one multiset {1,3}.
        let cage = cage();
        let store = Store::full(4);
        let mut cache = TuplesCache::default();
        let multisets = viable_multisets(&cage, &store, &mut cache).clone();
        assert_eq!(multisets.len(), 1);
        assert_eq!(multisets[0], vec![1u8, 3]);
    }

    #[test]
    fn viable_multisets_is_a_pure_memo() {
        let cage = cage();
        let store = Store::full(4);
        let mut cache = TuplesCache::default();
        let first = viable_multisets(&cage, &store, &mut cache).clone();
        let second = viable_multisets(&cage, &store, &mut cache).clone();
        assert_eq!(first, second);
        // A fresh cache gives the same result.
        let mut fresh = TuplesCache::default();
        assert_eq!(first, *viable_multisets(&cage, &store, &mut fresh));
    }

    #[test]
    fn viable_multisets_consistent_with_viable_tuples() {
        // Every multiset entry must appear as a sorted tuple in the tuple set.
        let cage = cage();
        let store = Store::full(4);
        let mut cache = TuplesCache::default();
        let tuples = viable_tuples(&cage, &store, &mut cache).clone();
        let multisets = viable_multisets(&cage, &store, &mut cache).clone();
        let tuple_multisets: std::collections::HashSet<Vec<u8>> = tuples
            .iter()
            .map(|t| {
                let mut s = t.clone();
                s.sort_unstable();
                s
            })
            .collect();
        for ms in &multisets {
            assert!(tuple_multisets.contains(ms));
        }
        assert_eq!(multisets.len(), tuple_multisets.len());
    }
}
