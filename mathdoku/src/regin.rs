//! Régin's generalized arc-consistency (GAC) algorithm for all-different.
//!
//! The entry point is [`regin_gac`], which prunes the values of a set of
//! variables so that every surviving value participates in at least one
//! complete assignment of distinct values. The algorithm runs in
//! `O(n + e)` time (where `n` is the number of variables and `e` the total
//! number of candidate values) by reducing GAC to a maximum bipartite matching
//! followed by a strongly connected-components decomposition of the residual digraph.
//!
//! Reference: Jean-Charles Régin, "A filtering algorithm for constraints of
//! difference in CSPs", *AAAI-94*, 1994, pp. 362–367.

#![allow(clippy::similar_names)] // var/val, ip/jp/kp are standard idioms in matching/SCC algorithms

use crate::Values;
use crate::cell::Value;
use std::collections::HashMap;

/// Full Régin GAC for all-different.
///
/// Given one value set per variable, returns the pruned value sets in the same order.
/// A value survives for a variable iff some assignment of distinct values (one
/// per variable, each within its value set) uses it; if no such complete
/// assignment exists, every value set empties.
pub fn regin_gac(values: &[Values]) -> Vec<Values> {
    let n = values.len();
    if n == 0 {
        return vec![];
    }

    let all_values: Vec<Value> = values
        .iter()
        .fold(Values::default(), |acc, d| acc | *d)
        .values();
    let num_values = all_values.len();
    let value_index: HashMap<Value, usize> = all_values
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, i))
        .collect();
    let indexed_values: Vec<Vec<usize>> = values
        .iter()
        .map(|d| d.values().iter().map(|v| value_index[v]).collect())
        .collect();

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

    // An unmatched variable means no system of distinct representatives exists:
    // the constraint is unsatisfiable, so every value set empties.
    if var_match.iter().any(Option::is_none) {
        return vec![Values::default(); n];
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
    // an alternating path from a free value.
    let mut result = vec![Values::default(); n];
    for var in 0..n {
        let matched = var_match[var];
        let vals: Vec<Value> = indexed_values[var]
            .iter()
            .filter(|&&vi| matched == Some(vi) || scc[var] == scc[n + vi] || reachable[n + vi])
            .map(|&vi| all_values[vi])
            .collect();
        result[var] = vals
            .iter()
            .fold(Values::default(), |acc, &v| acc | Values::singleton(v));
    }
    result
}

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
mod tests {
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    // --- Régin vs the brute-force oracle ---

    /// Exhaustive GAC oracle: a value is kept for a variable iff some complete
    /// assignment of distinct allowed values uses it.
    fn brute_force_gac(values: &[Values]) -> Vec<Values> {
        fn extend(
            i: usize,
            values: &[Values],
            used: u16,
            current: &mut [Value],
            support: &mut [Values],
        ) {
            if i == values.len() {
                for (slot, &value) in support.iter_mut().zip(current.iter()) {
                    *slot = *slot | Values::new(&[value]).unwrap();
                }
                return;
            }
            for value in values[i].values() {
                let bit = 1u16 << value;
                if used & bit == 0 {
                    current[i] = value;
                    extend(i + 1, values, used | bit, current, support);
                }
            }
        }
        let mut support = vec![Values::default(); values.len()];
        let mut current = vec![0u8; values.len()];
        extend(0, values, 0u16, &mut current, &mut support);
        support
    }

    fn sorted(fills: &[Values]) -> Vec<Vec<Value>> {
        fills.iter().map(|f| f.values()).collect()
    }

    #[test]
    fn regin_empty_input() {
        assert!(regin_gac(&[]).is_empty());
    }

    #[test]
    fn regin_prunes_forced_chain() {
        let values = vec![
            Values::new(&[1, 2]).unwrap(),
            Values::new(&[2]).unwrap(),
            Values::new(&[1, 3]).unwrap(),
        ];
        assert_eq!(sorted(&regin_gac(&values)), vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn regin_infeasible_empties_all() {
        let values = vec![Values::new(&[1]).unwrap(), Values::new(&[1]).unwrap()];
        assert_eq!(
            regin_gac(&values),
            vec![Values::default(), Values::default()]
        );
    }

    #[test]
    fn regin_keeps_free_value() {
        // One variable, two candidate values: full Régin keeps both.
        assert_eq!(
            sorted(&regin_gac(&[Values::new(&[1, 2]).unwrap()])),
            vec![vec![1, 2]]
        );
    }

    #[test]
    fn brute_force_matches_known_cases() {
        assert!(brute_force_gac(&[]).is_empty());
        assert_eq!(
            sorted(&brute_force_gac(&[
                Values::new(&[1, 2]).unwrap(),
                Values::new(&[2]).unwrap()
            ])),
            vec![vec![1], vec![2]]
        );
    }

    fn random_values(rng: &mut ChaCha8Rng, max_vars: usize, max_values: u8) -> Vec<Values> {
        let n_vars = rng.random_range(1..=max_vars);
        let n_values = rng.random_range(1..=max_values);
        (0..n_vars)
            .map(|_| {
                loop {
                    let mut fill = Values::default();
                    for value in 1..=n_values {
                        if rng.random_range(0u8..2) == 1 {
                            fill = fill | Values::new(&[value]).unwrap();
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
            let values = random_values(&mut rng, 8, 8);
            let union: Values = values.iter().fold(Values::default(), |acc, d| acc | *d);
            if union.len() > values.len() {
                saw_free_value_case = true;
            }
            assert_eq!(
                regin_gac(&values),
                brute_force_gac(&values),
                "Régin and brute force disagree on {values:?}"
            );
        }
        assert!(saw_free_value_case);
    }
}
