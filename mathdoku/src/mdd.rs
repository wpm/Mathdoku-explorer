//! Multivalued Decision Diagram (MDD) implementation of [`Memo`].
//!
//! Only commutative (add, multiply) constraints are supported. For non-commutative
//! constraints (subtract, divide), use `Table` instead.
use crate::Error::InvalidCellCageIndex;
use crate::fill::Fill;
use crate::memo::Memo;
use crate::operator::CommutativeOperator;
use crate::{Error, N, T};
use log::debug;
use std::collections::{HashMap, HashSet};

/// A cage constraint stored as a multivalued decision diagram.
///
/// Nodes are keyed by `(depth, accumulated_value, used_sets)` where `used_sets`
/// tracks which values have been placed in each still-open collinear line.
/// Edges are labelled with the cell value chosen at that depth. Valid tuples
/// correspond to paths from the root to a terminal node where `value == target`
/// and `depth == arity`.
///
/// Per-position candidate sets ([`Fill`]s) are derived from surviving edge
/// labels and cached; construction fails with [`EmptyFills`] if no valid
/// tuples exist.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mdd {
    n: N,
    constraint: Constraint,
    /// Collinear-line metadata: for each depth `d`, which line indices include
    /// cell `d`, and what the set of depths for each line is.
    line_meta: LineMeta,
    edges: HashMap<Node, Vec<(N, Node)>>,
    fills: Vec<Fill>,
}

impl Mdd {
    /// Constructs an MDD for all `k`-tuples of values in `1..=n` satisfying
    /// `operator` applied to the tuple equals `target`, with the given
    /// collinear distinctness constraints.
    ///
    /// `lines` is a list of groups of cell positions (0-indexed depths); cells
    /// in the same group share a row or column and must hold distinct values.
    /// Pass an empty slice for arithmetic-only (no collinear constraints).
    ///
    /// # Errors
    /// Returns [`Error::EmptyFills`] if no tuples satisfy the constraint.
    pub fn new(
        n: N,
        k: N,
        operator: CommutativeOperator,
        target: T,
        lines: &[Vec<usize>],
    ) -> Result<Self, Error> {
        let constraint = Constraint {
            operator,
            target,
            arity: T::from(k),
        };
        let line_meta = LineMeta::new(lines, k as usize);
        let num_lines = line_meta.num_lines();
        let root = Node {
            depth: 0,
            value: constraint.unit(),
            used: vec![Fill::default(); num_lines].into_boxed_slice(),
        };
        let mut mdd = Self {
            n,
            constraint,
            line_meta,
            edges: HashMap::new(),
            fills: Vec::new(),
        };
        mdd.subtree(&root);
        mdd.fills = mdd.fills_from_edges()?;
        Ok(mdd)
    }

    /// Recursively builds the MDD rooted at `head`, adding edges for all values
    /// that are not pruned by the constraint's monotonicity bounds or collinear
    /// distinctness.
    ///
    /// An edge is only inserted if the tail node is either a valid terminal
    /// (`depth == arity` and `value == target`) or itself has outgoing edges.
    /// This ensures the diagram contains no dead paths.
    fn subtree(&mut self, head: &Node) {
        if self.edges.contains_key(head) {
            return;
        }
        debug!("{self}");
        let remaining = self.constraint.arity - head.depth - 1;
        let n_t = T::from(self.n);
        let depth_idx = head.depth as usize;
        let lines_at_depth = self.line_meta.lines_at_depth(depth_idx).to_vec();

        for v in 1..=self.n {
            let i = T::from(v);
            if self.constraint.pruned(head.value, i, remaining) {
                break;
            }
            if self.constraint.skipped(head.value, i, remaining, n_t) {
                continue;
            }
            // Collinear distinctness: skip v if already used in any open line at this depth.
            if lines_at_depth
                .iter()
                .any(|&(line_idx, _)| head.used[line_idx].contains(v))
            {
                continue;
            }

            let tail_used = self.line_meta.advance_used(&head.used, depth_idx, v);
            let tail = Node {
                depth: head.depth + 1,
                value: self.constraint.operation(head.value, i),
                used: tail_used,
            };

            // Recursively build tail's subtree before deciding whether to link it.
            let tail_is_terminal = self.is_valid_terminal(&tail);
            let tail_at_arity = self.at_arity(&tail);
            let tail_at_target = self.at_target(&tail);
            if !tail_at_target && !tail_at_arity {
                self.subtree(&tail);
            }

            // Only insert the edge if tail is live: a valid terminal or has children.
            let tail_is_live = tail_is_terminal || self.edges.contains_key(&tail);
            if tail_is_live {
                self.insert_edge(head.clone(), v, tail);
            }
        }
    }

    /// Returns true if `node` is a valid accepting terminal: depth equals arity
    /// and accumulated value equals target.
    const fn is_valid_terminal(&self, node: &Node) -> bool {
        node.depth == self.constraint.arity && node.value == self.constraint.target
    }

    /// Returns a copy of this MDD with edges for forbidden values removed and
    /// dead nodes garbage-collected via downward and upward cascades.
    fn remove_support(&self, forbidden: &HashMap<T, HashSet<N>>) -> Self {
        let mut mdd = Self {
            n: self.n,
            constraint: self.constraint,
            line_meta: self.line_meta.clone(),
            edges: self.edges.clone(),
            fills: Vec::new(),
        };
        let mut q_down: Vec<Node> = Vec::new(); // nodes that may have lost all incoming edges
        let mut q_up: Vec<Node> = Vec::new(); // nodes that may have lost all outgoing edges

        for (&depth, forbidden) in forbidden {
            let heads = mdd.heads_at_depth(depth);
            let (total_arcs, dead_arcs) = heads
                .iter()
                .filter_map(|h| mdd.edges.get(h))
                .flat_map(|es| es.iter())
                .fold((0, 0), |(total, dead), (label, _)| {
                    (total + 1, dead + usize::from(forbidden.contains(label)))
                });

            if dead_arcs > total_arcs / 2 {
                debug!("Layer {depth}: reset ({dead_arcs}/{total_arcs} arcs dead)");
                mdd.reset_layer(&heads, forbidden, &mut q_down, &mut q_up);
            } else {
                debug!("Layer {depth}: delete ({dead_arcs}/{total_arcs} arcs dead)");
                mdd.delete_layer(&heads, forbidden, &mut q_down, &mut q_up);
            }
        }

        mdd.cascade_down(&mut q_down);
        mdd.cascade_up(&mut q_up);
        mdd
    }

    fn heads_at_depth(&self, depth: T) -> Vec<Node> {
        self.edges
            .keys()
            .filter(|n| n.depth == depth)
            .cloned()
            .collect()
    }

    fn tails_of(edges: &HashMap<Node, Vec<(N, Node)>>, heads: &[Node]) -> HashSet<Node> {
        heads
            .iter()
            .filter_map(|h| edges.get(h))
            .flat_map(|es| es.iter())
            .map(|(_, t)| t.clone())
            .collect()
    }

    fn reset_layer(
        &mut self,
        heads: &[Node],
        forbidden: &HashSet<N>,
        q_down: &mut Vec<Node>,
        q_up: &mut Vec<Node>,
    ) {
        let surviving: HashSet<N> = (1..=self.n).filter(|v| !forbidden.contains(v)).collect();
        let tails_before = Self::tails_of(&self.edges, heads);

        let orig: Vec<(Node, Vec<(N, Node)>)> = heads
            .iter()
            .filter_map(|h| self.edges.remove(h).map(|es| (h.clone(), es)))
            .collect();
        for (head, orig_edges) in orig {
            let new_edges: Vec<(N, Node)> = orig_edges
                .into_iter()
                .filter(|(label, _)| surviving.contains(label))
                .collect();
            if !new_edges.is_empty() {
                let _ = self.edges.insert(head, new_edges);
            }
        }

        let tails_after = Self::tails_of(&self.edges, heads);
        q_down.extend(
            tails_before
                .into_iter()
                .filter(|t| !tails_after.contains(t)),
        );
        q_up.extend(
            heads
                .iter()
                .filter(|h| !self.edges.contains_key(*h))
                .cloned(),
        );
    }

    fn delete_layer(
        &mut self,
        heads: &[Node],
        forbidden: &HashSet<N>,
        q_down: &mut Vec<Node>,
        q_up: &mut Vec<Node>,
    ) {
        for head in heads {
            if let Some(es) = self.edges.get_mut(head) {
                let dead_tails: Vec<Node> = es
                    .iter()
                    .filter(|(label, _)| forbidden.contains(label))
                    .map(|(_, t)| t.clone())
                    .collect(); // collect before retain to avoid borrow conflict
                es.retain(|(label, _)| !forbidden.contains(label));
                if es.is_empty() {
                    let _ = self.edges.remove(head);
                    q_up.push(head.clone());
                }
                for tail in dead_tails {
                    let still_reachable = heads.iter().any(|h| {
                        self.edges
                            .get(h)
                            .is_some_and(|es| es.iter().any(|(_, t)| *t == tail))
                    });
                    if !still_reachable {
                        q_down.push(tail);
                    }
                }
            }
        }
    }

    fn cascade_down(&mut self, q: &mut Vec<Node>) {
        while let Some(node) = q.pop() {
            if !self.edges.contains_key(&node) {
                continue;
            }
            let has_incoming = node.depth > 0
                && self
                    .edges
                    .keys()
                    .filter(|h| h.depth == node.depth - 1)
                    .any(|h| self.edges[h].iter().any(|(_, t)| *t == node));
            if !has_incoming {
                let outgoing = self.edges.remove(&node).unwrap_or_default();
                for (_, tail) in outgoing {
                    q.push(tail);
                }
            }
        }
    }

    fn cascade_up(&mut self, q: &mut Vec<Node>) {
        while let Some(node) = q.pop() {
            if self.edges.contains_key(&node) {
                continue;
            }
            let is_terminal =
                node.value == self.constraint.target && node.depth == self.constraint.arity;
            if !is_terminal {
                let heads: Vec<Node> = self
                    .edges
                    .keys()
                    .filter(|h| h.depth + 1 == node.depth)
                    .cloned()
                    .collect();
                for head in heads {
                    if let Some(es) = self.edges.get_mut(&head) {
                        es.retain(|(_, t)| *t != node);
                        if es.is_empty() {
                            let head_clone = head.clone();
                            let _ = self.edges.remove(&head);
                            q.push(head_clone);
                        }
                    }
                }
            }
        }
    }

    fn insert_edge(&mut self, head: Node, value: N, tail: Node) {
        debug!(
            "{:indent$}{head} -{value}→ {tail}",
            "",
            indent = head.depth as usize
        );
        self.edges.entry(head).or_default().push((value, tail));
    }

    fn at_arity(&self, tail: &Node) -> bool {
        let (d, a) = (u64::from(tail.depth), u64::from(self.constraint.arity));
        debug_assert!(d <= a, "depth {d} > arity {a}");
        Self::log_if(d == a, tail.depth, &format!("{tail} Arity limit met"))
    }

    fn at_target(&self, node: &Node) -> bool {
        Self::log_if(
            self.constraint.target_reached(node.value),
            node.depth,
            &format!("{node} Target reached"),
        )
    }

    fn log_if(condition: bool, depth: T, message: &str) -> bool {
        if condition {
            debug!("{:indent$}{message}", "", indent = depth as usize);
        }
        condition
    }

    /// Derives per-position fills by scanning edge labels at each depth.
    ///
    /// Returns `Err(EmptyFills)` if no edges exist at any depth (empty diagram).
    fn fills_from_edges(&self) -> Result<Vec<Fill>, Error> {
        let k = self.constraint.arity as usize;
        if k == 0 {
            return Err(Error::EmptyFills);
        }
        let mut fills = vec![Fill::default(); k];
        for (node, edges) in &self.edges {
            let depth = node.depth as usize;
            if depth < k {
                for &(label, _) in edges {
                    fills[depth] = fills[depth] | Fill::singleton(label);
                }
            }
        }
        if fills.iter().any(|f| f.is_empty()) {
            return Err(Error::EmptyFills);
        }
        Ok(fills)
    }

    #[allow(dead_code)] // used only in tests to verify MDD contents
    pub(crate) fn tuples(&self) -> Vec<Vec<N>> {
        let num_lines = self.line_meta.num_lines();
        let root = Node {
            depth: 0,
            value: self.constraint.unit(),
            used: vec![Fill::default(); num_lines].into_boxed_slice(),
        };
        let mut result = Vec::new();
        self.collect_paths(&root, &mut Vec::new(), &mut result);
        result
    }

    fn collect_paths(&self, head: &Node, path: &mut Vec<N>, result: &mut Vec<Vec<N>>) {
        match self.edges.get(head) {
            None => {
                if head.value == self.constraint.target && head.depth == self.constraint.arity {
                    result.push(path.clone());
                }
            }
            Some(edges) => {
                for (label, tail) in edges {
                    path.push(*label);
                    self.collect_paths(tail, path, result);
                    let _ = path.pop();
                }
            }
        }
    }
}

impl std::fmt::Display for Mdd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MDD({} {} nodes)", self.constraint, self.edges.len())
    }
}

impl Memo for Mdd {
    fn get(&self, index: usize) -> Result<Fill, Error> {
        self.fills
            .get(index)
            .copied()
            .ok_or(InvalidCellCageIndex(index))
    }

    fn narrow(&self, support: &[Fill]) -> Result<Self, Error> {
        let forbidden: HashMap<T, HashSet<N>> = support
            .iter()
            .enumerate()
            .filter_map(|(i, fill)| {
                let excluded: HashSet<N> = (1..=self.n).filter(|v| !fill.contains(*v)).collect();
                if excluded.is_empty() {
                    None
                } else {
                    // i is a cage position index, bounded by k <= 9.
                    #[allow(clippy::cast_possible_truncation)]
                    Some((T::from(i as N), excluded))
                }
            })
            .collect();
        let mut narrowed = self.remove_support(&forbidden);
        narrowed.fills = narrowed.fills_from_edges()?;
        Ok(narrowed)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct Constraint {
    // TODO Can this be ArithmeticConstraint?
    operator: CommutativeOperator,
    target: T,
    arity: T,
}

impl Constraint {
    const fn target_reached(self, v: T) -> bool {
        match self.operator {
            CommutativeOperator::Add => v >= self.target,
            CommutativeOperator::Multiply => v > self.target,
        }
    }

    const fn pruned(self, acc: T, v: T, _remaining: T) -> bool {
        match self.operator {
            CommutativeOperator::Add => acc + v > self.target,
            CommutativeOperator::Multiply => acc * v > self.target,
        }
    }

    const fn skipped(self, acc: T, v: T, remaining: T, n: T) -> bool {
        match self.operator {
            CommutativeOperator::Add => acc + v + remaining * n < self.target,
            CommutativeOperator::Multiply => (acc * v) != 0 && !self.target.is_multiple_of(acc * v),
        }
    }

    const fn operation(self, x: T, y: T) -> T {
        self.operator.apply_to_pair(x, y)
    }

    const fn unit(self) -> T {
        self.operator.identity()
    }
}

impl std::fmt::Display for Constraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = match self.operator {
            CommutativeOperator::Add => '+',
            CommutativeOperator::Multiply => '×',
        };
        write!(f, "{symbol}{} [{}]", self.target, self.arity)
    }
}

/// Precomputed collinear-line metadata for MDD construction.
///
/// For each depth (cell position in polyomino order), stores which line indices
/// that depth belongs to and whether the line closes at that depth.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LineMeta {
    /// For each depth: list of `(line_idx, is_last_in_line)` pairs.
    depth_info: Vec<Vec<(usize, bool)>>,
    num_lines: usize,
}

impl LineMeta {
    fn new(lines: &[Vec<usize>], k: usize) -> Self {
        let num_lines = lines.len();
        let mut depth_info: Vec<Vec<(usize, bool)>> = vec![Vec::new(); k];
        for (line_idx, line) in lines.iter().enumerate() {
            let last_depth = line.iter().copied().max().unwrap_or(0);
            for &depth in line {
                depth_info[depth].push((line_idx, depth == last_depth));
            }
        }
        Self {
            depth_info,
            num_lines,
        }
    }

    const fn num_lines(&self) -> usize {
        self.num_lines
    }

    fn lines_at_depth(&self, depth: usize) -> &[(usize, bool)] {
        self.depth_info.get(depth).map_or(&[], Vec::as_slice)
    }

    /// Returns the updated `used` sets after placing `value` at `depth`.
    ///
    /// Adds `value` to each line containing `depth`, then zeroes out lines
    /// that close at `depth` (all their cells have been placed).
    fn advance_used(&self, used: &[Fill], depth: usize, value: N) -> Box<[Fill]> {
        let mut next = used.to_vec();
        for &(line_idx, closes) in self.lines_at_depth(depth) {
            next[line_idx] = next[line_idx] | Fill::singleton(value);
            if closes {
                next[line_idx] = Fill::default();
            }
        }
        next.into_boxed_slice()
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
struct Node {
    depth: T,
    value: T,
    /// Used-value sets for still-open collinear lines (indexed by line index).
    /// Closed lines are zeroed out so that they don't inflate the key space.
    /// `Fill` doubles as a compact bitset (u16) for tracking which values have
    /// been placed in a line; its bitmap operations serve set-membership here,
    /// not candidate-fill semantics.
    used: Box<[Fill]>,
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({} @ level {})", self.value, self.depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error::EmptyFills;
    use crate::operator::CommutativeOperator::{Add, Multiply};
    use std::sync::OnceLock;

    static LOGGING: OnceLock<()> = OnceLock::new();
    fn setup() {
        let () = *LOGGING.get_or_init(crate::init_debug_logging);
    }

    fn no_lines() -> Vec<Vec<usize>> {
        vec![]
    }

    // ---- get ----

    #[test]
    fn add_fills_are_union_of_column_values() {
        setup();
        let m = Mdd::new(4, 2, Add, 6, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[2, 3, 4]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[2, 3, 4]));
    }

    #[test]
    fn multiply_fills_contain_expected_values() {
        setup();
        let m = Mdd::new(6, 2, Multiply, 6, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[1, 2, 3, 6]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[1, 2, 3, 6]));
    }

    #[test]
    fn commutative_no_solutions_returns_empty_fills_error() {
        setup();
        assert!(matches!(
            Mdd::new(4, 2, Add, 9, &no_lines()),
            Err(EmptyFills)
        ));
    }

    #[test]
    fn fill_out_of_bounds_returns_index_error() {
        setup();
        let m = Mdd::new(4, 2, Add, 5, &no_lines()).unwrap();
        assert!(matches!(m.get(2), Err(InvalidCellCageIndex(2))));
    }

    // ---- narrow ----

    #[test]
    fn narrow_with_full_support_is_identity() {
        setup();
        let m = Mdd::new(4, 2, Add, 5, &no_lines()).unwrap();
        assert_eq!(m.narrow(&[Fill::all(4), Fill::all(4)]).unwrap(), m);
    }

    #[test]
    fn narrow_filters_tuples_and_updates_fills() {
        setup();
        // add to 5 in n=4: (1,4),(2,3),(3,2),(4,1)
        // restrict pos 0 to {1,2} → surviving: (1,4),(2,3)
        let m = Mdd::new(4, 2, Add, 5, &no_lines()).unwrap();
        let narrowed = m
            .narrow(&[Fill::from(&[1, 2]), Fill::from(&[1, 2, 3, 4])])
            .unwrap();
        assert_eq!(narrowed.get(0).unwrap(), Fill::from(&[1, 2]));
        assert_eq!(narrowed.get(1).unwrap(), Fill::from(&[3, 4]));
    }

    #[test]
    fn narrow_eliminating_all_tuples_returns_empty_fills_error() {
        setup();
        let m = Mdd::new(4, 2, Add, 5, &no_lines()).unwrap();
        assert!(matches!(
            m.narrow(&[Fill::from(&[1]), Fill::from(&[1])]),
            Err(EmptyFills)
        ));
    }

    // ---- reset ----

    // ---- display ----

    #[test]
    fn sum_pair_display() {
        setup();
        assert_eq!(
            Mdd::new(3, 2, Add, 4, &no_lines()).unwrap().to_string(),
            "MDD(+4 [2] 4 nodes)"
        );
    }

    #[test]
    fn sum_triple_display() {
        setup();
        assert_eq!(
            Mdd::new(3, 3, Add, 5, &no_lines()).unwrap().to_string(),
            "MDD(+5 [3] 7 nodes)"
        );
    }

    #[test]
    fn sum_triple_larger_n_display() {
        setup();
        assert_eq!(
            Mdd::new(4, 3, Add, 6, &no_lines()).unwrap().to_string(),
            "MDD(+6 [3] 9 nodes)"
        );
    }

    #[test]
    fn product_pair_display() {
        setup();
        assert_eq!(
            Mdd::new(4, 2, Multiply, 6, &no_lines())
                .unwrap()
                .to_string(),
            "MDD(×6 [2] 3 nodes)"
        );
    }

    #[test]
    fn product_triple_display() {
        setup();
        assert_eq!(
            Mdd::new(4, 3, Multiply, 4, &no_lines())
                .unwrap()
                .to_string(),
            "MDD(×4 [3] 7 nodes)"
        );
    }

    // ---- fill values ----

    #[test]
    fn sum_pair_fills() {
        setup();
        let m = Mdd::new(3, 2, Add, 4, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[1, 2, 3]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[1, 2, 3]));
    }

    #[test]
    fn sum_triple_fills() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[1, 2, 3]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[1, 2, 3]));
        assert_eq!(m.get(2).unwrap(), Fill::from(&[1, 2, 3]));
    }

    #[test]
    fn product_pair_fills() {
        setup();
        let m = Mdd::new(4, 2, Multiply, 6, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[2, 3]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[2, 3]));
    }

    #[test]
    fn product_triple_fills() {
        setup();
        let m = Mdd::new(4, 3, Multiply, 4, &no_lines()).unwrap();
        assert_eq!(m.get(0).unwrap(), Fill::from(&[1, 2, 4]));
        assert_eq!(m.get(1).unwrap(), Fill::from(&[1, 2, 4]));
        assert_eq!(m.get(2).unwrap(), Fill::from(&[1, 2, 4]));
    }

    // ---- infeasibility ----

    #[test]
    fn sum_target_out_of_range_is_empty_fills() {
        setup();
        assert!(matches!(
            Mdd::new(3, 3, Add, 1, &no_lines()),
            Err(EmptyFills)
        ));
        assert!(matches!(
            Mdd::new(3, 3, Add, 10, &no_lines()),
            Err(EmptyFills)
        ));
    }

    #[test]
    fn product_target_out_of_range_is_empty_fills() {
        setup();
        assert!(matches!(
            Mdd::new(3, 3, Multiply, 28, &no_lines()),
            Err(EmptyFills)
        ));
    }

    // ---- remove_support ----

    #[test]
    fn remove_support_empty_is_identity() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines()).unwrap();
        assert_eq!(
            sorted_tuples(&m.remove_support(&HashMap::<T, HashSet<N>>::new())),
            sorted_tuples(&m)
        );
    }

    #[test]
    fn remove_support_sum_triple_delete_var0() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(0, &[1])]));
        assert_eq!(
            sorted_tuples(&m),
            vec![vec![2, 1, 2], vec![2, 2, 1], vec![3, 1, 1]]
        );
    }

    #[test]
    fn remove_support_sum_pair_delete_var0() {
        setup();
        let m = Mdd::new(3, 2, Add, 4, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(0, &[2])]));
        assert_eq!(sorted_tuples(&m), vec![vec![1, 3], vec![3, 1]]);
    }

    #[test]
    fn remove_support_product_pair_delete_var0() {
        setup();
        let m = Mdd::new(4, 2, Multiply, 6, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(0, &[3])]));
        assert_eq!(sorted_tuples(&m), vec![vec![2, 3]]);
    }

    #[test]
    fn remove_support_sum_triple_reset_var1() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(1, &[1, 2])]));
        assert_eq!(sorted_tuples(&m), vec![vec![1, 3, 1]]);
    }

    #[test]
    fn remove_support_sum_triple_two_layers() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(0, &[1]), (2, &[1])]));
        assert_eq!(sorted_tuples(&m), vec![vec![2, 1, 2]]);
    }

    #[test]
    fn remove_support_all_removed() {
        setup();
        let m = Mdd::new(3, 3, Add, 5, &no_lines())
            .unwrap()
            .remove_support(&forbidden(&[(1, &[1, 2, 3])]));
        assert_eq!(sorted_tuples(&m), vec![] as Vec<Vec<N>>);
    }

    // ---- reducedness ----

    #[test]
    fn constructed_mdd_is_reduced() {
        setup();
        let cases = [
            (4u8, Add, 5u32, 2u8),
            (6, Add, 10, 3),
            (9, Add, 20, 4),
            (4, Multiply, 6, 2),
            (6, Multiply, 24, 3),
        ];
        for (n, op, target, k) in cases {
            assert_reduced(&Mdd::new(n, k, op, target, &no_lines()).unwrap());
        }
    }

    #[test]
    fn mdd_is_reduced_after_remove_support() {
        setup();
        let m = Mdd::new(4, 3, Add, 6, &no_lines()).unwrap();
        let pruned = m.remove_support(&forbidden(&[(0, &[1])]));
        assert_reduced(&pruned);
    }

    // ---- collinear distinctness ----

    #[test]
    fn domino_add_collinear_excludes_equal_values() {
        setup();
        // +4 domino with both cells in the same row (line = [0, 1]).
        // Arithmetic tuples: (1,3),(2,2),(3,1). (2,2) repeats in the line → excluded.
        let m = Mdd::new(4, 2, Add, 4, &[vec![0, 1]]).unwrap();
        let mut t = m.tuples();
        t.sort();
        assert_eq!(t, vec![vec![1, 3], vec![3, 1]]);
        assert!(!m.get(0).unwrap().contains(2));
        assert!(!m.get(1).unwrap().contains(2));
    }

    #[test]
    fn domino_no_line_retains_equal_value_tuples() {
        setup();
        // Same arithmetic, no collinear constraint: (2,2) survives.
        let m = Mdd::new(4, 2, Add, 4, &no_lines()).unwrap();
        assert!(m.get(0).unwrap().contains(2));
        assert!(m.get(1).unwrap().contains(2));
    }

    #[test]
    fn l_cage_collinear_corner_admits_4_arms_do_not() {
        setup();
        // L-shape: cells at positions 0=(1,1), 1=(1,2), 2=(2,1) in a 4×4 grid.
        // Polyomino sorted order: (1,1)=depth0, (1,2)=depth1, (2,1)=depth2.
        // Collinear lines: row1 = [0,1] (depths 0 and 1), col1 = [0,2] (depths 0 and 2).
        // Target = 6, n = 4.
        //
        // Only corner (depth 0) should admit value 4, via tuple (4,1,1) where
        // the two 1s sit at non-collinear positions 1 and 2.
        let lines = vec![vec![0, 1], vec![0, 2]]; // row line, col line
        let m = Mdd::new(4, 3, Add, 6, &lines).unwrap();
        // corner (depth 0) can be 4
        assert!(
            m.get(0).unwrap().contains(4),
            "corner should admit 4 via (4,1,1)"
        );
        // arms (depths 1 and 2) cannot be 4
        assert!(
            !m.get(1).unwrap().contains(4),
            "arm at depth 1 must not admit 4"
        );
        assert!(
            !m.get(2).unwrap().contains(4),
            "arm at depth 2 must not admit 4"
        );
    }

    // ---- brute-force oracle cross-check ----

    #[test]
    #[ignore = "exhaustive property test; run with --include-ignored on merge to main"]
    fn matches_brute_force_across_n_arity_and_target() {
        setup();
        for n in 3u8..=9 {
            for k in 2u8..=5 {
                let max_sum = T::from(n) * T::from(k) + 1;
                for target in 1..=max_sum {
                    assert_equiv(n, Add, target, k);
                }
                let max_product = T::from(n).pow(u32::from(k)) + 1;
                for target in 1..=max_product {
                    assert_equiv(n, Multiply, target, k);
                }
            }
        }
    }

    // ---- helpers ----

    fn forbidden(pairs: &[(T, &[N])]) -> HashMap<T, HashSet<N>> {
        pairs
            .iter()
            .map(|&(var, vals)| (var, vals.iter().copied().collect()))
            .collect()
    }

    fn sorted_tuples(m: &Mdd) -> Vec<Vec<N>> {
        let mut t = m.tuples();
        t.sort();
        t
    }

    fn ref_tuples(n: N, op: CommutativeOperator, target: T, k: N) -> Vec<Vec<N>> {
        let mut out = Vec::new();
        let mut t = vec![1u8; k as usize];
        loop {
            if op.apply_to_tuple(&t) == target {
                out.push(t.clone());
            }
            let mut i = 0;
            while i < k as usize && t[i] == n {
                t[i] = 1;
                i += 1;
            }
            if i == k as usize {
                break;
            }
            t[i] += 1;
        }
        out.sort();
        out
    }

    fn assert_equiv(n: N, op: CommutativeOperator, target: T, k: N) {
        let expected = ref_tuples(n, op, target, k);
        match Mdd::new(n, k, op, target, &no_lines()) {
            Ok(m) => {
                let mut actual = m.tuples();
                actual.sort();
                assert_eq!(
                    actual, expected,
                    "mismatch for n={n}, op={op:?}, target={target}, k={k}"
                );
            }
            Err(EmptyFills) => {
                assert!(
                    expected.is_empty(),
                    "Mdd returned EmptyFills but expected {expected:?} for n={n}, op={op:?}, target={target}, k={k}"
                );
            }
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }

    fn assert_reduced(m: &Mdd) {
        let mut seen = HashSet::new();
        for node in m.edges.keys() {
            assert!(seen.insert(node.clone()), "duplicate node {node} in MDD");
        }
    }
}
