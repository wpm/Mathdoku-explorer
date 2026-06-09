//! Régin's generalized arc-consistency (GAC) algorithm for all-different.
//!
//! The entry point is [`regin_gac`], which prunes the candidate fills of a set of
//! variables so that every surviving value participates in at least one complete
//! assignment of distinct values. The algorithm runs in `O(n + e)` time (where `n`
//! is the number of variables and `e` the total number of candidate values) by
//! reducing GAC to a maximum bipartite matching followed by a strongly connected
//! components (SCC) decomposition of the residual digraph.
//!
//! ## Algorithm outline
//!
//! 1. **Maximum bipartite matching** — find a system of distinct representatives
//!    (one value per variable) via augmenting paths. If no perfect matching exists
//!    the constraint is unsatisfiable and every fill is emptied.
//! 2. **Residual digraph** — orient matched edges var → val and unmatched edges
//!    val → var. A directed path in this graph is an *alternating path*.
//! 3. **SCC decomposition** — decompose the residual digraph into strongly connected
//!    components using Kosaraju's two-pass DFS algorithm.
//! 4. **Reachability from free values** — mark all nodes reachable from unmatched
//!    ("free") values by forward DFS in the residual digraph.
//! 5. **Pruning** — an edge (var, val) is kept iff it is matched, both endpoints
//!    share an SCC (alternating cycle), or the value node is reachable from a free
//!    value (alternating path from a free value). All other values are pruned.
//!
//! ## References
//!
//! - Jean-Charles Régin, "A filtering algorithm for constraints of difference in
//!   CSPs", *Proceedings of the 12th National Conference on Artificial Intelligence
//!   (AAAI-94)*, 1994, pp. 362–367.
//! - Micha Sharir, "A strong-connectivity algorithm and its applications in data
//!   flow analysis", *Computers & Mathematics with Applications*, vol. 7, 1981,
//!   pp. 67–72. (The two-pass DFS SCC algorithm, discovered independently by
//!   S. Rao Kosaraju in 1978 but unpublished.)

#![allow(clippy::similar_names)] // var/val, ip/jp/kp are standard idioms in matching/SCC algorithms

use crate::N;
use crate::fill::Fill;
use std::collections::HashMap;

/// Full Régin GAC for all-different.
///
/// Given one fill per variable, returns the pruned fills in the same order.
/// A value survives for a variable iff some assignment of distinct values (one
/// per variable, each within its fill) uses it; if no such complete assignment
/// exists, every fill empties.
pub fn regin_gac(fills: &[Fill]) -> Vec<Fill> {
    let n = fills.len();
    if n == 0 {
        return vec![];
    }

    let union = fills
        .iter()
        .copied()
        .fold(Fill::default(), |acc, f| acc | f);
    let all_values: Vec<N> = union.values();
    let num_values = all_values.len();
    let value_index: HashMap<N, usize> = all_values
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, i))
        .collect();
    let real_n = n;
    // When there are more candidate values than variables (k < num_values), add
    // virtual "not-assigned" variables — one per excess value — each able to absorb
    // any value. This ensures a perfect matching over all values exists whenever the
    // real variables can be matched, eliminating spurious free values that would
    // otherwise keep every value reachable and prevent SCC-based pruning.
    let virtual_count = num_values.saturating_sub(n);
    let all_value_indices: Vec<usize> = (0..num_values).collect();
    let mut indexed_values: Vec<Vec<usize>> = fills
        .iter()
        .map(|f| f.values().iter().map(|v| value_index[v]).collect())
        .collect();
    for _ in 0..virtual_count {
        indexed_values.push(all_value_indices.clone());
    }
    let n = indexed_values.len();

    // Maximum bipartite matching via augmenting paths.
    let mut var_match: Vec<Option<usize>> = vec![None; n];
    let mut val_match: Vec<Option<usize>> = vec![None; num_values];
    let mut visited = vec![false; num_values];
    for var in 0..n {
        visited.fill(false);
        let _ = augment(
            var,
            &indexed_values,
            &mut var_match,
            &mut val_match,
            &mut visited,
        );
    }

    // An unmatched real variable means no system of distinct representatives exists:
    // the constraint is unsatisfiable, so every fill empties.
    if var_match[..real_n].iter().any(Option::is_none) {
        return vec![Fill::default(); real_n];
    }

    // Residual digraph. Node layout: variables 0..n, values n..n+num_values.
    // Orientation: matched edges var → val, unmatched edges val → var, so a
    // directed walk from a free value is exactly an alternating path from it.
    let total = n + num_values;
    let mut adj: Vec<Vec<usize>> = vec![vec![]; total];
    for var in 0..n {
        for &vi in &indexed_values[var] {
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
    // an alternating path from a free value. Only emit results for real variables.
    let mut result = vec![Fill::default(); real_n];
    for var in 0..real_n {
        let matched = var_match[var];
        let surviving: Vec<N> = indexed_values[var]
            .iter()
            .filter(|&&vi| matched == Some(vi) || scc[var] == scc[n + vi] || reachable[n + vi])
            .map(|&vi| all_values[vi])
            .collect();
        result[var] = Fill::from(&surviving);
    }
    result
}

/// Tries to augment the matching by finding an alternating path from `var` to a
/// free value. Returns `true` and updates `var_match`/`val_match` on success.
fn augment(
    var: usize,
    indexed_values: &[Vec<usize>],
    var_match: &mut [Option<usize>],
    val_match: &mut [Option<usize>],
    visited: &mut [bool],
) -> bool {
    for &vi in &indexed_values[var] {
        if visited[vi] {
            continue;
        }
        visited[vi] = true;
        if val_match[vi]
            .is_none_or(|other| augment(other, indexed_values, var_match, val_match, visited))
        {
            var_match[var] = Some(vi);
            val_match[vi] = Some(var);
            return true;
        }
    }
    false
}

/// Returns a component label per node such that two nodes share a label iff they
/// are in the same SCC. Uses Kosaraju's two-pass algorithm: forward DFS to build
/// a finish-time ordering, then reverse-graph DFS in reverse finish order.
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

/// Iterative DFS from `start`; returns nodes in finish order (post-order).
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

/// Iterative DFS on the reversed graph from `start`; assigns `label` to all
/// reachable unvisited nodes.
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
mod tests {
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn regin_empty_input() {
        assert!(regin_gac(&[]).is_empty());
    }

    #[test]
    fn regin_prunes_forced_chain() {
        let fills = vec![Fill::from(&[1, 2]), Fill::from(&[2]), Fill::from(&[1, 3])];
        assert_eq!(
            regin_gac(&fills),
            vec![Fill::from(&[1]), Fill::from(&[2]), Fill::from(&[3])]
        );
    }

    #[test]
    fn regin_infeasible_empties_all() {
        let fills = vec![Fill::from(&[1]), Fill::from(&[1])];
        assert_eq!(regin_gac(&fills), vec![Fill::default(), Fill::default()]);
    }

    #[test]
    fn regin_keeps_free_value() {
        assert_eq!(regin_gac(&[Fill::from(&[1, 2])]), vec![Fill::from(&[1, 2])]);
    }

    // k < n tests: fewer variables than candidate values. Values that can only be
    // assigned to a variable by forcing a duplicate elsewhere must be pruned.

    #[test]
    fn regin_two_vars_four_values_no_pruning() {
        // x,y ∈ {1,2,3,4}: any two distinct values work — nothing prunable.
        let fills = vec![Fill::from(&[1, 2, 3, 4]), Fill::from(&[1, 2, 3, 4])];
        assert_eq!(
            regin_gac(&fills),
            vec![Fill::from(&[1, 2, 3, 4]), Fill::from(&[1, 2, 3, 4])]
        );
    }

    #[test]
    fn regin_one_var_two_values_no_pruning() {
        // Single variable with {1,2}: both values are feasible assignments.
        assert_eq!(regin_gac(&[Fill::from(&[1, 2])]), vec![Fill::from(&[1, 2])]);
    }

    #[test]
    fn regin_two_vars_one_forced_prunes_partner() {
        // x ∈ {1}, y ∈ {1,2,3,4}: x=1 forces y ≠ 1, so 1 is pruned from y.
        let result = regin_gac(&[Fill::from(&[1]), Fill::from(&[1, 2, 3, 4])]);
        assert_eq!(result[0], Fill::from(&[1]));
        assert!(
            !result[1].contains(1),
            "1 should be pruned from y since x=1"
        );
    }

    #[test]
    fn regin_two_vars_overlap_forces_distinct() {
        // x ∈ {1,2}, y ∈ {1,2}: with k=2 and only 2 values, both must be used —
        // same as the k==n case. No extra pruning beyond what standard Regin does.
        let result = regin_gac(&[Fill::from(&[1, 2]), Fill::from(&[1, 2])]);
        assert_eq!(result[0], Fill::from(&[1, 2]));
        assert_eq!(result[1], Fill::from(&[1, 2]));
    }

    #[test]
    fn regin_matches_brute_force_oracle() {
        let mut rng = ChaCha8Rng::seed_from_u64(0xABCD_EF01);
        for _ in 0..5000 {
            let fills = random_fills(&mut rng, 8, 8);
            assert_eq!(
                regin_gac(&fills),
                brute_force_gac(&fills),
                "Régin and brute force disagree on {fills:?}"
            );
        }
    }

    fn brute_force_gac(fills: &[Fill]) -> Vec<Fill> {
        fn extend(
            i: usize,
            fills: &[Fill],
            used: &std::collections::BTreeSet<N>,
            current: &mut Vec<N>,
            support: &mut Vec<std::collections::BTreeSet<N>>,
        ) {
            if i == fills.len() {
                for (slot, &value) in support.iter_mut().zip(current.iter()) {
                    let _ = slot.insert(value);
                }
                return;
            }
            for value in fills[i].values() {
                if !used.contains(&value) {
                    current.push(value);
                    let mut used2 = used.clone();
                    let _ = used2.insert(value);
                    extend(i + 1, fills, &used2, current, support);
                    let _ = current.pop();
                }
            }
        }
        let mut support: Vec<std::collections::BTreeSet<N>> =
            vec![std::collections::BTreeSet::new(); fills.len()];
        let mut current: Vec<N> = vec![];
        extend(
            0,
            fills,
            &std::collections::BTreeSet::new(),
            &mut current,
            &mut support,
        );
        support
            .into_iter()
            .map(|s| Fill::from(&s.into_iter().collect::<Vec<_>>()))
            .collect()
    }

    fn random_fills(rng: &mut ChaCha8Rng, max_vars: usize, max_value: N) -> Vec<Fill> {
        let n_vars = rng.random_range(1..=max_vars);
        let n_values = rng.random_range(1..=max_value);
        (0..n_vars)
            .map(|_| {
                loop {
                    let values: Vec<N> = (1..=n_values)
                        .filter(|_| rng.random_range(0u8..2) == 1)
                        .collect();
                    if !values.is_empty() {
                        break Fill::from(&values);
                    }
                }
            })
            .collect()
    }
}
