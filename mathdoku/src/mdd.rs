//! Multi-valued Decision Diagram (MDD) representation of cage constraints.
//!
//! An [`Mdd`] is a reduced, ordered DAG representing exactly the valid [`Tuple`]s
//! of a cage: one level per cell (in [`Polyomino::cells`] order), edges labelled by
//! the value placed in that cell, and a single accept terminal that every valid
//! assignment reaches. Failing paths are simply absent — there is no false terminal.
//!
//! [`Mdd::build`] is a single depth-first pass that interleaves the arithmetic and
//! collinearity (all-different within a shared row or column) constraints into the
//! search, then hash-conses each node on return. Two nodes at the same level with
//! identical edge maps are *equivalent* and are merged to one canonical node, so the
//! result is the unique reduced ordered MDD for the cell ordering. Following Knuth,
//! "equivalent" denotes this node-merging relation, reserving "isomorphic" for graph
//! isomorphism.
//!
//! [`Mdd::support`] computes per-cell GAC support in `O(|edges|)` via a top-down
//! reachability sweep followed by a bottom-up co-reachability sweep (MDD-4R), which
//! is strictly faster than enumerating paths when cages are large.
//!
//! ## References
//!
//! - Bryant, R. E. (1986). [Graph-Based Algorithms for Boolean Function
//!   Manipulation](https://www.cs.cmu.edu/~bryant/pubdir/ieeetc86.pdf). *IEEE
//!   Transactions on Computers*, C-35(8), 677–691. The original reduced ordered BDD
//!   paper. Establishes that one post-order pass with hash-consing on
//!   `(variable, sorted (label, child_id))` produces the canonical reduced form, and
//!   that the reduced form is unique for a fixed variable ordering. The construction
//!   in [`Mdd::build`] is the multi-valued generalization of Bryant's reduce.
//! - Srinivasan, A., Ham, T., Malik, S., & Brayton, R. K. (1990). Algorithms for
//!   discrete function manipulation. *Proceedings of ICCAD 1990*, 92–95. The original
//!   Multi-valued Decision Diagram paper, extending Bryant's BDD framework from binary
//!   to multi-valued variables.
//! - Knuth, D. E. (2011). *The Art of Computer Programming, Volume 4A*, §7.1.4.
//!   Comprehensive textbook treatment of BDDs. Knuth carefully uses "equivalent" for
//!   the node-merging relation and reserves "isomorphic" for graph isomorphism —
//!   this module follows that convention.
//! - Cheng, K. C. K., & Yap, R. H. C. (2010). [An MDD-based generalized arc
//!   consistency algorithm for positive and negative table constraints and some global
//!   constraints](https://www.comp.nus.edu.sg/~ryap/papers/cmdd.pdf). *Constraints*,
//!   15(2), 265–304. Frames table constraints as MDDs and introduces the MDD-4R
//!   algorithm used by [`Mdd::support`].

// Targets and bounds are compared in `M`; the small `usize` level/remaining counts
// and the `u32` exponent are widened or narrowed without meaningful loss for n ≤ 9.
#![allow(clippy::cast_possible_truncation)]

use std::collections::HashMap;

use crate::operation::{Operation, Operator};
use crate::{Polyomino, Target, Tuple, Value, Values};

/// Index of a node within [`Mdd::nodes`].
type NodeId = usize;

/// A node in the MDD. The accept terminal is the unique node whose `level` equals
/// the cage size and whose `edges` are empty; every other node has at least one edge.
#[derive(Debug, Clone)]
struct Node {
    level: usize,
    edges: Vec<(Value, NodeId)>,
}

/// A reduced ordered MDD over the valid tuples of a cage.
#[derive(Debug, Clone)]
pub struct Mdd {
    nodes: Vec<Node>,
    root: Option<NodeId>,
    k: usize,
}

impl Mdd {
    /// Builds the reduced ordered MDD of all valid tuples for `polyomino` under
    /// `operation` in an `n`×`n` grid.
    ///
    /// Cells are visited in [`Polyomino::cells`] (row-major) order. At each level the
    /// candidate values `1..=n` are tried in ascending order, pruned by collinearity
    /// and by the operator's arithmetic bounds before recursing, and the resulting
    /// node is hash-consed so equivalent subgraphs are shared.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn build(n: usize, polyomino: &Polyomino, operation: Operation) -> Self {
        let cells = polyomino.cells();
        let k = cells.len();
        let shares = (0..k)
            .map(|i| {
                (0..i)
                    .filter(|&j| cells[j].row == cells[i].row || cells[j].column == cells[i].column)
                    .collect()
            })
            .collect();
        let mut builder = Builder {
            n,
            k,
            operator: operation.operator,
            target: operation.target,
            shares,
            nodes: Vec::new(),
            intern: HashMap::new(),
            terminal: 0,
        };
        builder.terminal = builder.intern(k, Vec::new());
        // Subtract and Divide are binary operators: they relate exactly two cells. A
        // cage of any other size admits no tuples, matching the reference enumerator
        // (whose two-element multisets have no length-k permutations for k != 2).
        let two_cell_only = matches!(builder.operator, Operator::Subtract | Operator::Divide);
        let root = if two_cell_only && k != 2 {
            None
        } else {
            let init_acc = match builder.operator {
                Operator::Multiply => 1,
                _ => 0,
            };
            let mut assignment = Vec::with_capacity(k);
            builder.dfs(0, &mut assignment, init_acc)
        };
        Self {
            nodes: builder.nodes,
            root,
            k,
        }
    }

    /// Enumerates every valid tuple by walking each root-to-terminal path.
    pub(crate) fn tuples(&self) -> impl Iterator<Item = Tuple> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            let mut path = Vec::with_capacity(self.k);
            self.collect_paths(root, &mut path, &mut out);
        }
        out.into_iter()
    }

    /// Appends to `out` every tuple reachable from the node `id`, extending the
    /// current `path` along each outgoing edge.
    fn collect_paths(&self, id: NodeId, path: &mut Tuple, out: &mut Vec<Tuple>) {
        let node = &self.nodes[id];
        if node.level == self.k {
            out.push(path.clone());
            return;
        }
        for &(label, child) in &node.edges {
            path.push(label);
            self.collect_paths(child, path, out);
            let _ = path.pop();
        }
    }

    /// Computes the per-cell support of the cage under the current cell `values`.
    ///
    /// `values` holds one [`Values`] set per cell, in [`Polyomino::cells`] order;
    /// the returned vector has the same length. Position `i` is the set of values that
    /// appear at cell `i` in at least one root-to-terminal path every label of which
    /// lies in its cell's value set. When no such path exists — including when any value
    /// set is empty — every returned set is empty, so infeasibility propagates.
    ///
    /// Runs in `O(|edges|)` via a top-down reachability sweep followed by a bottom-up
    /// co-reachability sweep (a variant of Perez & Régin's MDD-4R), rather than the
    /// `O(|paths|)` cost of filtering [`Mdd::tuples`].
    pub(crate) fn support(&self, values: &[Values]) -> Vec<Values> {
        debug_assert_eq!(
            values.len(),
            self.k,
            "support expects one value set per cell"
        );
        let Some(root) = self.root else {
            return vec![Values::default(); self.k];
        };

        // Node ids grouped by level; every edge runs from level `i` to level `i + 1`,
        // so a single increasing (decreasing) pass settles each sweep.
        let mut levels: Vec<Vec<NodeId>> = vec![Vec::new(); self.k + 1];
        for (id, node) in self.nodes.iter().enumerate() {
            levels[node.level].push(id);
        }

        // Top-down: the root is reachable, and a node is reachable when some allowed
        // edge from a reachable node lands on it.
        let mut reachable = vec![false; self.nodes.len()];
        reachable[root] = true;
        for i in 0..self.k {
            for &u in &levels[i] {
                if reachable[u] {
                    for &(label, child) in &self.nodes[u].edges {
                        if values[i].contains(label) {
                            reachable[child] = true;
                        }
                    }
                }
            }
        }

        // Bottom-up: the terminal is co-reachable, and a node is co-reachable when some
        // allowed edge from it lands on a co-reachable node.
        let mut coreachable = vec![false; self.nodes.len()];
        for &t in &levels[self.k] {
            coreachable[t] = true;
        }
        for i in (0..self.k).rev() {
            for &u in &levels[i] {
                coreachable[u] = self.nodes[u]
                    .edges
                    .iter()
                    .any(|&(label, child)| values[i].contains(label) && coreachable[child]);
            }
        }

        // Support: union the label of every edge bridging a reachable node to a
        // co-reachable node within its cell's value set.
        let mut support = vec![Values::default(); self.k];
        for i in 0..self.k {
            for &u in &levels[i] {
                if reachable[u] {
                    for &(label, child) in &self.nodes[u].edges {
                        if values[i].contains(label) && coreachable[child] {
                            support[i] = support[i] | Values::singleton(label);
                        }
                    }
                }
            }
        }
        support
    }
}

/// Mutable state threaded through the depth-first construction.
struct Builder {
    n: usize,
    k: usize,
    operator: Operator,
    target: Target,
    /// `shares[i]` holds the indices `j < i` of earlier cells sharing a row or
    /// column with cell `i` — the cells whose values constrain cell `i`.
    shares: Vec<Vec<usize>>,
    nodes: Vec<Node>,
    intern: HashMap<(usize, Vec<(Value, NodeId)>), NodeId>,
    terminal: NodeId,
}

impl Builder {
    /// Interns a node by `(level, sorted edges)`, returning the existing canonical id
    /// if an equivalent node was already created, or a fresh id otherwise.
    fn intern(&mut self, level: usize, mut edges: Vec<(Value, NodeId)>) -> NodeId {
        edges.sort_unstable();
        let key = (level, edges.clone());
        if let Some(&id) = self.intern.get(&key) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Node { level, edges });
        let _ = self.intern.insert(key, id);
        id
    }

    /// Explores cell `level` given the values already placed in `assignment` and the
    /// running accumulator `acc` (sum for [`Operator::Add`], product for
    /// [`Operator::Multiply`]). Returns the canonical node id, or `None` if no value
    /// leads to the accept terminal ("dead").
    fn dfs(&mut self, level: usize, assignment: &mut Vec<Value>, acc: Target) -> Option<NodeId> {
        if level == self.k {
            return Some(self.terminal);
        }
        let mut edges: Vec<(Value, NodeId)> = Vec::new();
        for v in 1u8..=(self.n as Value) {
            if self.shares[level].iter().any(|&j| assignment[j] == v) {
                continue;
            }
            let remaining = self.k - level - 1;
            let next_acc = match self.operator {
                Operator::Add => {
                    let new_sum = acc + Target::from(v);
                    if new_sum + remaining as Target > self.target {
                        break; // min reachable total already exceeds the target
                    }
                    if new_sum + remaining as Target * (self.n as Target) < self.target {
                        continue; // max reachable total still below the target
                    }
                    new_sum
                }
                Operator::Multiply => {
                    let new_product = acc * Target::from(v);
                    if new_product > self.target {
                        break; // product already exceeds the target
                    }
                    if !self.target.is_multiple_of(new_product) {
                        continue; // target is not a multiple of the running product
                    }
                    if new_product * (self.n as Target).pow(remaining as u32) < self.target {
                        continue; // max reachable product still below the target
                    }
                    new_product
                }
                Operator::Subtract => match assignment.last() {
                    Some(&first)
                        if Target::from(first).abs_diff(Target::from(v)) != self.target =>
                    {
                        continue;
                    }
                    _ => acc,
                },
                Operator::Divide => match assignment.last() {
                    Some(&first)
                        if Target::from(first) != Target::from(v) * self.target
                            && Target::from(v) != Target::from(first) * self.target =>
                    {
                        continue;
                    }
                    _ => acc,
                },
                Operator::Given if Target::from(v) != self.target => continue,
                Operator::Given => acc,
            };
            assignment.push(v);
            let child = self.dfs(level + 1, assignment, next_acc);
            let _ = assignment.pop();
            if let Some(child) = child {
                edges.push((v, child));
            }
        }
        if edges.is_empty() {
            None
        } else {
            Some(self.intern(level, edges))
        }
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use std::collections::HashSet;

    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::*;
    use crate::operation::operators_for;
    use crate::test_utils::{cells, col_pair, l_shape, pair, singleton};

    impl Mdd {
        const fn is_feasible(&self) -> bool {
            self.root.is_some()
        }
    }

    fn square() -> Polyomino {
        Polyomino::from_cells(&cells(&[(0, 0), (0, 1), (1, 0), (1, 1)])).unwrap()
    }

    /// Sorted, deduplicated tuples produced by the MDD for the given cage.
    fn mdd_tuples(n: usize, polyomino: &Polyomino, op: Operation) -> Vec<Tuple> {
        let mut ts: Vec<Tuple> = Mdd::build(n, polyomino, op).tuples().collect();
        ts.sort();
        ts.dedup();
        ts
    }

    /// Sorted, deduplicated tuples produced by an independent brute-force oracle:
    /// every assignment of `1..=n` to the cage's cells that satisfies the operator's
    /// arithmetic and the all-different rule within each shared row or column. This
    /// shares no machinery with [`Mdd::build`], so agreement is a real cross-check.
    fn ref_tuples(n: usize, polyomino: &Polyomino, op: &Operation) -> Vec<Tuple> {
        let cells = polyomino.cells();
        let k = cells.len();

        let collinear_ok = |t: &[Value]| {
            (0..k).all(|i| {
                (0..i).all(|j| {
                    (cells[i].row != cells[j].row && cells[i].column != cells[j].column)
                        || t[i] != t[j]
                })
            })
        };

        let satisfies = |t: &[Value]| match op.operator {
            Operator::Add => t.iter().map(|&v| Target::from(v)).sum::<Target>() == op.target,
            Operator::Multiply => {
                t.iter().map(|&v| Target::from(v)).product::<Target>() == op.target
            }
            Operator::Given => k == 1 && Target::from(t[0]) == op.target,
            Operator::Subtract => {
                k == 2 && Target::from(t[0]).abs_diff(Target::from(t[1])) == op.target
            }
            Operator::Divide => {
                k == 2 && {
                    let (a, b) = (Target::from(t[0]), Target::from(t[1]));
                    a == b * op.target || b == a * op.target
                }
            }
        };

        // Enumerate every k-tuple over `1..=n` like an odometer.
        let mut out = Vec::new();
        let mut t = vec![1u8; k];
        loop {
            if collinear_ok(&t) && satisfies(&t) {
                out.push(t.clone());
            }
            let mut i = 0;
            while i < k && t[i] == n as Value {
                t[i] = 1;
                i += 1;
            }
            if i == k {
                break;
            }
            t[i] += 1;
        }

        out.sort();
        out.dedup();
        out
    }

    /// Asserts the MDD and the reference enumerator agree, and that feasibility
    /// matches tuple non-emptiness.
    fn assert_equiv(n: usize, polyomino: &Polyomino, op: &Operation) {
        let mdd = Mdd::build(n, polyomino, op.clone());
        let expected = ref_tuples(n, polyomino, op);
        let actual = mdd_tuples(n, polyomino, op.clone());
        assert_eq!(
            actual,
            expected,
            "mismatch for n={n}, op={op}, cells={:?}",
            polyomino.cells()
        );
        assert_eq!(
            mdd.is_feasible(),
            !expected.is_empty(),
            "feasibility mismatch for n={n}, op={op}, cells={:?}",
            polyomino.cells()
        );
    }

    /// Targets worth testing for `operator` in an `n`×`n` grid holding `k` cells. The
    /// ranges run one past the largest reachable value to also exercise infeasibility.
    fn targets(operator: &Operator, n: usize, k: usize) -> Vec<Target> {
        match operator {
            Operator::Add => (1..=(n as Target) * k as Target + 1).collect(),
            Operator::Multiply => (1..=(n as Target).pow(k as u32) + 1).collect(),
            Operator::Subtract => (0..=n as Target).collect(),
            Operator::Divide => (1..=n as Target).collect(),
            Operator::Given => (1..=n as Target + 1).collect(),
        }
    }

    // --- property test: equivalence with the reference enumerator ---

    #[test]
    #[ignore = "exhaustive property test; run with --include-ignored on merge to main"]
    fn mdd_matches_reference_across_shapes_operators_and_grids() {
        let shapes = [singleton(), pair(), col_pair(), l_shape(), square()];
        for shape in &shapes {
            let k = shape.len();
            for n in 3..=9 {
                for operator in operators_for(shape) {
                    for target in targets(&operator, n, k) {
                        assert_equiv(n, shape, &Operation::new(operator.clone(), target));
                    }
                }
            }
        }
    }

    // --- reducedness ---

    /// The MDD is reduced iff no two distinct nodes at the same level share an edge map.
    fn assert_reduced(mdd: &Mdd) {
        let mut seen: HashSet<(usize, Vec<(Value, NodeId)>)> = HashSet::new();
        for node in &mdd.nodes {
            assert!(
                seen.insert((node.level, node.edges.clone())),
                "duplicate node at level {} with edges {:?}",
                node.level,
                node.edges
            );
        }
    }

    #[test]
    fn constructed_mdd_is_reduced() {
        let cases = [
            (4, pair(), Operation::new(Operator::Add, 5)),
            (6, l_shape(), Operation::new(Operator::Multiply, 24)),
            (4, square(), Operation::new(Operator::Multiply, 24)),
            (9, square(), Operation::new(Operator::Add, 20)),
        ];
        for (n, shape, op) in cases {
            assert_reduced(&Mdd::build(n, &shape, op));
        }
    }

    // --- Given (k = 1) ---

    #[test]
    fn given_in_range_yields_single_tuple() {
        let mdd = Mdd::build(4, &singleton(), Operation::new(Operator::Given, 3));
        assert!(mdd.is_feasible());
        assert_eq!(mdd.tuples().collect::<Vec<_>>(), vec![vec![3]]);
    }

    #[test]
    fn given_out_of_range_is_infeasible() {
        let mdd = Mdd::build(4, &singleton(), Operation::new(Operator::Given, 5));
        assert!(!mdd.is_feasible());
        assert_eq!(mdd.tuples().count(), 0);
    }

    // --- Add ---

    #[test]
    fn add_row_pair_matches_reference() {
        assert_equiv(4, &pair(), &Operation::new(Operator::Add, 5));
    }

    #[test]
    fn add_column_pair_matches_reference() {
        assert_equiv(4, &col_pair(), &Operation::new(Operator::Add, 5));
    }

    #[test]
    fn add_l_shape_matches_reference() {
        assert_equiv(6, &l_shape(), &Operation::new(Operator::Add, 10));
    }

    // --- Multiply ---

    #[test]
    fn multiply_row_pair_matches_reference() {
        assert_equiv(6, &pair(), &Operation::new(Operator::Multiply, 6));
    }

    #[test]
    fn multiply_column_pair_matches_reference() {
        assert_equiv(6, &col_pair(), &Operation::new(Operator::Multiply, 6));
    }

    #[test]
    fn multiply_l_shape_matches_reference() {
        assert_equiv(6, &l_shape(), &Operation::new(Operator::Multiply, 24));
    }

    // --- Subtract / Divide are binary: non-pair cages yield no tuples ---

    #[test]
    fn subtract_non_pair_is_infeasible() {
        // The reference enumerator has no length-3 permutations of a 2-element multiset,
        // so a 3-cell Subtract cage admits no tuples; the MDD must agree (not a chain).
        let mdd = Mdd::build(4, &l_shape(), Operation::new(Operator::Subtract, 1));
        assert!(!mdd.is_feasible());
        assert_eq!(mdd.tuples().count(), 0);
        assert_equiv(4, &l_shape(), &Operation::new(Operator::Subtract, 1));
    }

    #[test]
    fn divide_non_pair_is_infeasible() {
        let mdd = Mdd::build(6, &l_shape(), Operation::new(Operator::Divide, 2));
        assert!(!mdd.is_feasible());
        assert_eq!(mdd.tuples().count(), 0);
        assert_equiv(6, &l_shape(), &Operation::new(Operator::Divide, 2));
    }

    // --- 2×2 square: the smallest case with real row/column merging ---

    #[test]
    fn square_multiply_matches_reference() {
        assert_equiv(4, &square(), &Operation::new(Operator::Multiply, 24));
    }

    #[test]
    fn square_add_matches_reference() {
        assert_equiv(4, &square(), &Operation::new(Operator::Add, 10));
    }

    /// Node count of the minimal prefix-sharing trie of `ts`: one node per distinct
    /// prefix (including the empty root and every full tuple as its own leaf). The
    /// reduced MDD additionally merges shared *suffixes*, so it can only be smaller.
    fn trie_node_count(ts: &[Tuple]) -> usize {
        let mut prefixes: HashSet<&[Value]> = HashSet::new();
        for t in ts {
            for len in 0..=t.len() {
                let _ = prefixes.insert(&t[..len]);
            }
        }
        prefixes.len()
    }

    #[test]
    fn square_merges_equivalent_nodes() {
        // A 2×2 square shares a row or column between every pair of cells, so the
        // construction merges equivalent subgraphs (most visibly the single accept
        // terminal that all tuples share). The reduced MDD therefore holds strictly
        // fewer nodes than the equivalent prefix-sharing trie.
        let op = Operation::new(Operator::Add, 10);
        let mdd = Mdd::build(4, &square(), op.clone());
        assert_reduced(&mdd);
        let tuples = ref_tuples(4, &square(), &op);
        assert!(tuples.len() > 1);
        assert!(
            mdd.nodes.len() < trie_node_count(&tuples),
            "expected the reduced MDD ({} nodes) to be smaller than the trie ({} nodes)",
            mdd.nodes.len(),
            trie_node_count(&tuples)
        );
    }

    // --- support ---

    /// Reference per-cell support: the per-position union of values over every valid
    /// tuple consistent with `values`. Runs in `O(|paths|)`, the cost the MDD avoids.
    fn brute_force_support(mdd: &Mdd, values: &[Values]) -> Vec<Values> {
        let mut support = vec![Values::default(); values.len()];
        for tuple in mdd.tuples() {
            if tuple.iter().zip(values).all(|(&v, d)| d.contains(v)) {
                for (i, &v) in tuple.iter().enumerate() {
                    support[i] = support[i] | Values::singleton(v);
                }
            }
        }
        support
    }

    /// `k` random value sets over `1..=n`, each value independently included with
    /// probability one half — so some value sets may come out empty.
    fn random_support_values(rng: &mut ChaCha8Rng, n: usize, k: usize) -> Vec<Values> {
        (0..k)
            .map(|_| {
                (1u8..=(n as Value)).fold(Values::default(), |acc, v| {
                    if rng.random_range(0u8..2) == 1 {
                        acc | Values::singleton(v)
                    } else {
                        acc
                    }
                })
            })
            .collect()
    }

    #[test]
    fn support_matches_brute_force_oracle() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x5044_2026);
        let shapes = [singleton(), pair(), col_pair(), l_shape(), square()];
        for shape in &shapes {
            let k = shape.len();
            for n in 3..=6 {
                for operator in operators_for(shape) {
                    let ts = targets(&operator, n, k);
                    for _ in 0..6 {
                        let target = ts[rng.random_range(0..ts.len())];
                        let mdd = Mdd::build(n, shape, Operation::new(operator.clone(), target));
                        for _ in 0..8 {
                            let values = random_support_values(&mut rng, n, k);
                            assert_eq!(
                                mdd.support(&values),
                                brute_force_support(&mdd, &values),
                                "support mismatch for n={n}, op={operator}, target={target}, \
                                 values={values:?}, cells={:?}",
                                shape.cells()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn support_with_empty_value_set_is_all_empty() {
        // A feasible 2×2 Add cage, but one cell's value set is knocked out entirely.
        let mdd = Mdd::build(4, &square(), Operation::new(Operator::Add, 10));
        assert!(mdd.is_feasible());
        let values = vec![
            Values::all(4),
            Values::default(),
            Values::all(4),
            Values::all(4),
        ];
        let support = mdd.support(&values);
        assert_eq!(support.len(), 4);
        assert!(support.iter().all(|s| s.is_empty()));
    }

    #[test]
    fn support_full_value_sets_is_natural_support() {
        // With every value set `1..=n` no edge is filtered, so support is exactly the
        // natural per-cell support: each level's labels on a root-to-terminal path,
        // equivalently the per-position union over all valid tuples.
        let n = 6;
        let shape = l_shape();
        let k = shape.len();
        let mdd = Mdd::build(n, &shape, Operation::new(Operator::Multiply, 24));
        let values = vec![Values::all(n); k];
        let natural = brute_force_support(&mdd, &values);
        assert_eq!(mdd.support(&values), natural);
        // Sanity: the cage is non-trivial — at least one cell supports several values.
        assert!(natural.iter().any(|s| s.len() > 1));
    }
}
