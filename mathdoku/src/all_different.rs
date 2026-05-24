//! The all-different constraint, filtered to GAC by Régin's algorithm.
//!
//! [`AllDifferent`] forces every cell in a row or column to take a distinct
//! value. [`regin_gac`] is full Régin — maximum matching, SCC condensation, and
//! free-value reachability — achieving generalized arc consistency even when the
//! number of candidate values exceeds the number of variables. A property test
//! cross-checks it against an exhaustive brute-force oracle.

use std::collections::HashMap;

use crate::{
    Cell, Domain, Error,
    constraint::{Constraint, Outcome, PropagationCtx},
    cover::Cover,
    store::Narrowed,
    types::N,
    variable::Variable,
};

/// A constraint that ensures every cell in a row or column contains a different
/// value.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AllDifferent {
    cells: Vec<Cell>,
}

impl AllDifferent {
    /// A row of cells on an `n`×`n` grid that must all differ.
    ///
    /// # Errors
    /// Returns [`Error::IndexOutOfRange`] if `index` is not less than `n`.
    pub fn row(n: usize, index: usize) -> Result<Self, Error> {
        if index >= n {
            return Err(Error::IndexOutOfRange(index, n));
        }
        let cells = (0..n).map(|column| Cell::new(index, column)).collect();
        Ok(Self { cells })
    }

    /// A column of cells on an `n`×`n` grid that must all differ.
    ///
    /// # Errors
    /// Returns [`Error::IndexOutOfRange`] if `index` is not less than `n`.
    pub fn column(n: usize, index: usize) -> Result<Self, Error> {
        if index >= n {
            return Err(Error::IndexOutOfRange(index, n));
        }
        let cells = (0..n).map(|row| Cell::new(row, index)).collect();
        Ok(Self { cells })
    }
}

impl Constraint<Cell> for AllDifferent {
    fn propagate(&self, ctx: &mut PropagationCtx<Cell>) -> Outcome {
        let domains: Vec<Domain> = self.cells.iter().map(|c| ctx.store.get(c.id())).collect();
        let pruned = regin_gac(&domains);
        let mut outcome = Outcome::Unchanged;
        for (cell, domain) in self.cells.iter().zip(pruned) {
            match ctx.store.intersect(cell.id(), domain) {
                Narrowed::Empty => return Outcome::Contradiction,
                Narrowed::Changed => outcome = Outcome::Changed,
                Narrowed::Unchanged => {}
            }
        }
        outcome
    }
}

impl Cover for AllDifferent {
    fn cells(&self) -> impl Iterator<Item = Cell> {
        self.cells.iter().copied()
    }
}

/// Full Régin GAC for all-different: given one domain per variable, returns the
/// pruned domains in the same order. A value survives for a variable iff some
/// assignment of distinct values (one per variable, each within its domain) uses
/// it; if no such complete assignment exists, every domain empties.
#[allow(clippy::similar_names)]
pub fn regin_gac(domains: &[Domain]) -> Vec<Domain> {
    let n = domains.len();
    if n == 0 {
        return vec![];
    }

    let all_values: Vec<N> = domains
        .iter()
        .fold(Domain::default(), |acc, d| acc | *d)
        .iter()
        .collect();
    let num_values = all_values.len();
    let value_index: HashMap<N, usize> = all_values
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, i))
        .collect();
    let indexed_domains: Vec<Vec<usize>> = domains
        .iter()
        .map(|d| d.iter().map(|v| value_index[&v]).collect())
        .collect();

    // Maximum bipartite matching via augmenting paths.
    let mut var_match: Vec<Option<usize>> = vec![None; n];
    let mut val_match: Vec<Option<usize>> = vec![None; num_values];
    let mut visited = vec![false; num_values];
    for var in 0..n {
        visited.fill(false);
        let _ = augment(
            var,
            &indexed_domains,
            &mut var_match,
            &mut val_match,
            &mut visited,
        );
    }

    // An unmatched variable means no system of distinct representatives exists:
    // the constraint is unsatisfiable, so every domain empties.
    if var_match.iter().any(Option::is_none) {
        return vec![Domain::default(); n];
    }

    // Residual digraph. Node layout: variables 0..n, values n..n+num_values.
    // Orientation: matched edges var → val, unmatched edges val → var, so a
    // directed walk from a free value is exactly an alternating path from it.
    let total = n + num_values;
    let mut adj: Vec<Vec<usize>> = vec![vec![]; total];
    for var in 0..n {
        for &vi in &indexed_domains[var] {
            let val_node = n + vi;
            if var_match[var] == Some(vi) {
                adj[var].push(val_node);
            } else {
                adj[val_node].push(var);
            }
        }
    }

    let scc = kosaraju_scc(&adj, total);

    // Mark every node reachable from a free (unmatched) value. An unmatched edge
    // (var, val) lies on an alternating path from a free value iff its value node
    // is reachable here — the step an SCC-only filter omits.
    let mut reachable = vec![false; total];
    let mut stack: Vec<usize> = (0..num_values)
        .filter(|&vi| val_match[vi].is_none())
        .map(|vi| n + vi)
        .collect();
    for &node in &stack {
        reachable[node] = true;
    }
    while let Some(node) = stack.pop() {
        for &next in &adj[node] {
            if !reachable[next] {
                reachable[next] = true;
                stack.push(next);
            }
        }
    }

    // Keep edge (var, val) iff matched, in an alternating cycle (same SCC), or on
    // an alternating path from a free value.
    let mut result = vec![Domain::default(); n];
    for var in 0..n {
        let matched = var_match[var];
        result[var] = indexed_domains[var]
            .iter()
            .filter(|&&vi| matched == Some(vi) || scc[var] == scc[n + vi] || reachable[n + vi])
            .map(|&vi| all_values[vi])
            .collect();
    }
    result
}

#[allow(clippy::similar_names)]
fn augment(
    var: usize,
    indexed_domains: &[Vec<usize>],
    var_match: &mut [Option<usize>],
    val_match: &mut [Option<usize>],
    visited: &mut [bool],
) -> bool {
    for &vi in &indexed_domains[var] {
        if visited[vi] {
            continue;
        }
        visited[vi] = true;
        if val_match[vi]
            .is_none_or(|other| augment(other, indexed_domains, var_match, val_match, visited))
        {
            var_match[var] = Some(vi);
            val_match[vi] = Some(var);
            return true;
        }
    }
    false
}

fn kosaraju_scc(adj: &[Vec<usize>], n: usize) -> Vec<usize> {
    let mut visited = vec![false; n];
    let mut finish_order: Vec<usize> = Vec::with_capacity(n);
    for start in 0..n {
        if !visited[start] {
            finish_order.extend(dfs_finish(start, adj, &mut visited));
        }
    }

    let mut radj: Vec<Vec<usize>> = vec![vec![]; n];
    for (u, neighbors) in adj.iter().enumerate().take(n) {
        for &v in neighbors {
            radj[v].push(u);
        }
    }

    let mut comp = vec![usize::MAX; n];
    let mut label = 0usize;
    for &start in finish_order.iter().rev() {
        if comp[start] == usize::MAX {
            dfs_assign(start, label, &radj, &mut comp);
            label += 1;
        }
    }
    comp
}

fn dfs_finish(start: usize, adj: &[Vec<usize>], visited: &mut [bool]) -> Vec<usize> {
    let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
    let mut order: Vec<usize> = vec![];
    visited[start] = true;
    while let Some((u, idx)) = stack.last_mut() {
        let u = *u;
        if *idx < adj[u].len() {
            let v = adj[u][*idx];
            *idx += 1;
            if !visited[v] {
                visited[v] = true;
                stack.push((v, 0));
            }
        } else {
            order.push(u);
            let _ = stack.pop();
        }
    }
    order
}

fn dfs_assign(start: usize, label: usize, radj: &[Vec<usize>], comp: &mut [usize]) {
    let mut stack = vec![start];
    comp[start] = label;
    while let Some(u) = stack.pop() {
        for &v in &radj[u] {
            if comp[v] == usize::MAX {
                comp[v] = label;
                stack.push(v);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::*;
    use crate::{cache::TuplesCache, store::Store};

    fn row_4() -> AllDifferent {
        AllDifferent::row(4, 2).unwrap()
    }

    fn column_3() -> AllDifferent {
        AllDifferent::column(3, 1).unwrap()
    }

    fn assert_index_out_of_range(f: impl Fn(usize, usize) -> Result<AllDifferent, Error>) {
        assert!(f(0, 0).is_err());
        assert!(matches!(f(3, 3), Err(Error::IndexOutOfRange(3, 3))));
        assert!(matches!(f(3, 5), Err(Error::IndexOutOfRange(5, 3))));
    }

    #[test]
    fn row_contains_correct_cells() {
        itertools::assert_equal(
            row_4().cells(),
            [
                Cell::new(2, 0),
                Cell::new(2, 1),
                Cell::new(2, 2),
                Cell::new(2, 3),
            ],
        );
    }

    #[test]
    fn row_index_out_of_range_returns_err() {
        assert_index_out_of_range(AllDifferent::row);
    }

    #[test]
    fn column_contains_correct_cells() {
        itertools::assert_equal(
            column_3().cells(),
            [Cell::new(0, 1), Cell::new(1, 1), Cell::new(2, 1)],
        );
    }

    #[test]
    fn column_index_out_of_range_returns_err() {
        assert_index_out_of_range(AllDifferent::column);
    }

    #[test]
    fn len_equals_n() {
        assert_eq!(row_4().len(), 4);
        assert_eq!(column_3().len(), 3);
    }

    #[test]
    fn is_empty_is_false_for_nonempty_constraint() {
        assert!(!row_4().is_empty());
        assert!(!column_3().is_empty());
    }

    #[test]
    fn propagate_prunes_and_can_contradict() {
        let mut store = Store::full(2);
        store.set(Cell::new(0, 0).id(), Domain::new([1]));
        store.set(Cell::new(0, 1).id(), Domain::new([1]));
        let mut cache = TuplesCache::default();
        let mut ctx = PropagationCtx::new(&mut store, &mut cache);
        assert_eq!(
            AllDifferent::row(2, 0).unwrap().propagate(&mut ctx),
            Outcome::Contradiction
        );
    }

    #[test]
    fn propagate_unchanged_when_already_consistent() {
        let mut store = Store::full(4);
        let mut cache = TuplesCache::default();
        let mut ctx = PropagationCtx::new(&mut store, &mut cache);
        assert_eq!(
            AllDifferent::row(4, 0).unwrap().propagate(&mut ctx),
            Outcome::Unchanged
        );
    }

    // --- Régin vs the brute-force oracle ---

    /// Exhaustive GAC oracle: a value is kept for a variable iff some complete
    /// assignment of distinct in-domain values uses it.
    fn brute_force_gac(domains: &[Domain]) -> Vec<Domain> {
        fn extend(
            i: usize,
            domains: &[Domain],
            used: u16,
            current: &mut [N],
            support: &mut [Domain],
        ) {
            if i == domains.len() {
                for (slot, &value) in support.iter_mut().zip(current.iter()) {
                    *slot = *slot | Domain::new([value]);
                }
                return;
            }
            for value in domains[i].iter() {
                let bit = 1u16 << value;
                if used & bit == 0 {
                    current[i] = value;
                    extend(i + 1, domains, used | bit, current, support);
                }
            }
        }
        let mut support = vec![Domain::default(); domains.len()];
        let mut current = vec![0u8; domains.len()];
        extend(0, domains, 0u16, &mut current, &mut support);
        support
    }

    fn sorted(fills: &[Domain]) -> Vec<Vec<N>> {
        fills.iter().map(|f| f.iter().collect()).collect()
    }

    #[test]
    fn regin_empty_input() {
        assert!(regin_gac(&[]).is_empty());
    }

    #[test]
    fn regin_prunes_forced_chain() {
        let domains = vec![Domain::new([1, 2]), Domain::new([2]), Domain::new([1, 3])];
        assert_eq!(
            sorted(&regin_gac(&domains)),
            vec![vec![1], vec![2], vec![3]]
        );
    }

    #[test]
    fn regin_infeasible_empties_all() {
        let domains = vec![Domain::new([1]), Domain::new([1])];
        assert_eq!(
            regin_gac(&domains),
            vec![Domain::default(), Domain::default()]
        );
    }

    #[test]
    fn regin_keeps_free_value() {
        // One variable, two domain values: full Régin keeps both.
        assert_eq!(sorted(&regin_gac(&[Domain::new([1, 2])])), vec![vec![1, 2]]);
    }

    #[test]
    fn brute_force_matches_known_cases() {
        assert!(brute_force_gac(&[]).is_empty());
        assert_eq!(
            sorted(&brute_force_gac(&[Domain::new([1, 2]), Domain::new([2])])),
            vec![vec![1], vec![2]]
        );
    }

    fn random_domains(rng: &mut ChaCha8Rng, max_vars: usize, max_values: u8) -> Vec<Domain> {
        let n_vars = rng.random_range(1..=max_vars);
        let n_values = rng.random_range(1..=max_values);
        (0..n_vars)
            .map(|_| {
                loop {
                    let mut fill = Domain::default();
                    for value in 1..=n_values {
                        if rng.random_range(0u8..2) == 1 {
                            fill = fill | Domain::new([value]);
                        }
                    }
                    if !fill.is_empty() {
                        break fill;
                    }
                }
            })
            .collect()
    }

    /// Across thousands of random instances spanning the full ≤8-variable /
    /// ≤8-value regime (including value > variable), full Régin must agree with
    /// the brute-force GAC oracle.
    #[test]
    fn regin_matches_brute_force_oracle() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x5151_2026);
        let mut saw_free_value_case = false;
        for _ in 0..5000 {
            let domains = random_domains(&mut rng, 8, 8);
            let values: Domain = domains.iter().fold(Domain::default(), |acc, d| acc | *d);
            if values.len() > domains.len() {
                saw_free_value_case = true;
            }
            assert_eq!(
                regin_gac(&domains),
                brute_force_gac(&domains),
                "Régin and brute force disagree on {domains:?}"
            );
        }
        assert!(saw_free_value_case);
    }
}
