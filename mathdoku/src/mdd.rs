//! Multivalued Decision Diagram (MDD) implementation of [`Memo`].
//!
//! Only commutative (add, multiply) constraints are supported. For non-commutative
//! constraints (subtract, divide), use `Table` instead.
//!
//! The underlying cage dynamic program ([`CageDp`]) has two drivers: the [`Mdd`]
//! builder (compile once, narrow many — for propagation) and the lazy
//! [`CageSolutions`] iterator (one-shot existence and enumeration queries).
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
    dp: CageDp,
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
        let dp = CageDp::new(n, k, operator, target, lines);
        let root = Node {
            depth: 0,
            state: dp.root(),
        };
        let mut mdd = Self {
            dp,
            edges: HashMap::new(),
            fills: Vec::new(),
        };
        mdd.subtree(&root);
        mdd.fills = mdd.fills_from_edges()?;
        Ok(mdd)
    }

    /// Recursively builds the MDD rooted at `head`, adding edges for all values
    /// whose [`CageDp::step`] transition yields a successor state.
    ///
    /// An edge is only inserted if the tail node is either a valid terminal
    /// (`depth == arity` and `value == target`) or itself has outgoing edges.
    /// This ensures the diagram contains no dead paths.
    fn subtree(&mut self, head: &Node) {
        if self.edges.contains_key(head) {
            return;
        }
        debug!("{self}");

        for v in 1..=self.dp.n {
            let state = match self.dp.step(head.depth, &head.state, v) {
                Step::Stop => break,
                Step::Skip => continue,
                Step::Tail(state) => state,
            };
            let tail = Node {
                depth: head.depth + 1,
                state,
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
        self.dp.accept(node.depth, &node.state)
    }

    /// Returns a copy of this MDD with edges for forbidden values removed and
    /// dead nodes garbage-collected via downward and upward cascades.
    fn remove_support(&self, forbidden: &HashMap<T, HashSet<N>>) -> Self {
        let mut mdd = Self {
            dp: self.dp.clone(),
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
        let surviving: HashSet<N> = (1..=self.dp.n).filter(|v| !forbidden.contains(v)).collect();
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
            let is_terminal = self.dp.accept(node.depth, &node.state);
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
        let (d, a) = (u64::from(tail.depth), u64::from(self.dp.constraint.arity));
        debug_assert!(d <= a, "depth {d} > arity {a}");
        Self::log_if(d == a, tail.depth, &format!("{tail} Arity limit met"))
    }

    fn at_target(&self, node: &Node) -> bool {
        Self::log_if(
            self.dp.target_reached(node.state.value),
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
        let k = self.dp.constraint.arity as usize;
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
        let root = Node {
            depth: 0,
            state: self.dp.root(),
        };
        let mut result = Vec::new();
        self.collect_paths(&root, &mut Vec::new(), &mut result);
        result
    }

    fn collect_paths(&self, head: &Node, path: &mut Vec<N>, result: &mut Vec<Vec<N>>) {
        match self.edges.get(head) {
            None => {
                if self.dp.accept(head.depth, &head.state) {
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

    /// Returns the number of accepting tuples (root→terminal paths) in this MDD.
    ///
    /// Computed by a memoized subtree-count fold over the shared DAG, so the
    /// cost is proportional to the number of states and edges, never to the
    /// number of paths.
    pub(crate) fn tuple_count(&self) -> u64 {
        let root = Node {
            depth: 0,
            state: self.dp.root(),
        };
        let mut counts = HashMap::new();
        self.count_paths(&root, &mut counts)
    }

    fn count_paths(&self, head: &Node, counts: &mut HashMap<Node, u64>) -> u64 {
        if let Some(&count) = counts.get(head) {
            return count;
        }
        let count = self.edges.get(head).map_or_else(
            || u64::from(self.dp.accept(head.depth, &head.state)),
            |edges| {
                edges
                    .iter()
                    .map(|(_, tail)| self.count_paths(tail, counts))
                    .sum()
            },
        );
        let _ = counts.insert(head.clone(), count);
        count
    }

    /// Returns the number of distinct value multisets among accepting tuples.
    ///
    /// Folds bottom-up over the DAG, layer by layer: every edge runs from
    /// depth `d` to `d + 1`, so each node's set of suffix multisets (sorted
    /// label vectors on paths from that node to a terminal) is assembled from
    /// already-computed successors. The work is bounded by the diagram size
    /// times the number of distinct multisets, never the number of paths.
    pub(crate) fn multiset_count(&self) -> u64 {
        let root = Node {
            depth: 0,
            state: self.dp.root(),
        };
        if !self.edges.contains_key(&root) {
            return u64::from(self.dp.accept(root.depth, &root.state));
        }
        let mut suffixes: HashMap<Node, HashSet<Vec<N>>> = HashMap::new();
        let depths: HashSet<T> = self.edges.keys().map(|n| n.depth).collect();
        let mut depths: Vec<T> = depths.into_iter().collect();
        depths.sort_unstable_by(|a, b| b.cmp(a));
        for depth in depths {
            for head in self.heads_at_depth(depth) {
                let mut head_suffixes: HashSet<Vec<N>> = HashSet::new();
                for (label, tail) in &self.edges[&head] {
                    match suffixes.get(tail) {
                        // A tail without outgoing edges is a terminal: the MDD
                        // is reduced, so it is always accepting, but check anyway.
                        None => {
                            if self.dp.accept(tail.depth, &tail.state) {
                                let _ = head_suffixes.insert(vec![*label]);
                            }
                        }
                        Some(tail_suffixes) => {
                            for multiset in tail_suffixes {
                                let mut with_label = multiset.clone();
                                let at = with_label.partition_point(|&v| v < *label);
                                with_label.insert(at, *label);
                                let _ = head_suffixes.insert(with_label);
                            }
                        }
                    }
                }
                let _ = suffixes.insert(head, head_suffixes);
            }
        }
        u64::try_from(suffixes[&root].len()).unwrap_or(u64::MAX)
    }
}

impl std::fmt::Display for Mdd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MDD({} {} nodes)", self.dp.constraint, self.edges.len())
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
                let excluded: HashSet<N> = (1..=self.dp.n).filter(|v| !fill.contains(*v)).collect();
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

/// The cage dynamic program: the conjunction of a commutative arithmetic
/// constraint and collinear distinctness, expressed as an explicit
/// `root`/`step`/`accept` transition over [`State`].
///
/// [`Mdd`] construction consumes this DP, but the DP itself is standalone:
/// it can also be driven lazily via [`CageDp::solutions`] without building
/// a diagram.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CageDp {
    n: N,
    constraint: Constraint,
    /// Collinear-line metadata: for each depth `d`, which line indices include
    /// cell `d`, and what the set of depths for each line is.
    line_meta: LineMeta,
}

impl CageDp {
    /// Constructs the DP for all `k`-tuples of values in `1..=n` satisfying
    /// `operator` applied to the tuple equals `target`, with the given
    /// collinear distinctness constraints (see [`Mdd::new`] for the `lines`
    /// format).
    pub fn new(n: N, k: N, operator: CommutativeOperator, target: T, lines: &[Vec<usize>]) -> Self {
        Self {
            n,
            constraint: Constraint {
                operator,
                target,
                arity: T::from(k),
            },
            line_meta: LineMeta::new(lines, k as usize),
        }
    }

    /// The DP state before any value has been placed.
    fn root(&self) -> State {
        State {
            value: self.constraint.unit(),
            used: vec![Fill::default(); self.line_meta.num_lines()].into_boxed_slice(),
        }
    }

    /// One DP transition: the outcome of placing value `v` at cell `depth`
    /// from `state`.
    ///
    /// Returns [`Step::Stop`] when the constraint's monotonicity bound rules
    /// out `v` and every larger value, [`Step::Skip`] when `v` alone is
    /// infeasible (arithmetic skip or collinear distinctness violation), and
    /// [`Step::Tail`] with the successor state otherwise.
    fn step(&self, depth: T, state: &State, v: N) -> Step {
        let remaining = self.constraint.arity - depth - 1;
        let i = T::from(v);
        if self.constraint.pruned(state.value, i, remaining) {
            return Step::Stop;
        }
        if self
            .constraint
            .skipped(state.value, i, remaining, T::from(self.n))
        {
            return Step::Skip;
        }
        // Collinear distinctness: skip v if already used in any open line at this depth.
        let depth_idx = depth as usize;
        if self
            .line_meta
            .lines_at_depth(depth_idx)
            .iter()
            .any(|&(line_idx, _)| state.used[line_idx].contains(v))
        {
            return Step::Skip;
        }
        Step::Tail(State {
            value: self.constraint.operation(state.value, i),
            used: self.line_meta.advance_used(&state.used, depth_idx, v),
        })
    }

    /// Returns true if `state` at `depth` is accepting: every cell has been
    /// placed and the accumulated value equals the target.
    const fn accept(&self, depth: T, state: &State) -> bool {
        depth == self.constraint.arity && state.value == self.constraint.target
    }

    /// The recursion cutoff for drivers of this DP: returns true if the
    /// accumulated `value` has reached the point where no deeper placement
    /// can accept, keeping the operator-aware arithmetic inside the DP.
    const fn target_reached(&self, value: T) -> bool {
        self.constraint.target_reached(value)
    }

    /// Returns a lazy iterator over this DP's accepting tuples whose value at
    /// each depth `d` lies in `support[d]`.
    ///
    /// Construction and narrowing are fused into one guarded traversal, and
    /// the iterator exits early: existence is `solutions(..).next().is_some()`
    /// without ever building a diagram. `support` must have one [`Fill`] per
    /// cage cell.
    pub fn solutions<'a>(&'a self, support: &'a [Fill]) -> CageSolutions<'a> {
        debug_assert_eq!(support.len(), self.constraint.arity as usize);
        CageSolutions {
            dp: self,
            support,
            stack: vec![Frame {
                state: self.root(),
                label: 0,
                next_v: 1,
                found: false,
            }],
            dead: HashSet::new(),
        }
    }
}

/// A lazy depth-first iterator over a [`CageDp`]'s accepting tuples,
/// restricted by a per-depth support.
///
/// Each item is one accepting tuple in cage cell order. The traversal is
/// driven entirely by [`CageDp::step`] / [`CageDp::accept`] plus the support
/// guard, and memoizes states proven to have no accepting descendant in
/// `dead`, keeping exhaustion `O(states)` instead of `O(paths)`.
///
/// Obtained via [`CageDp::solutions`].
#[must_use]
pub struct CageSolutions<'a> {
    dp: &'a CageDp,
    /// Per-depth allowed values; guards each step.
    support: &'a [Fill],
    /// The current root → node path; `stack[d]` holds the [`State`] at depth `d`.
    stack: Vec<Frame>,
    /// States proven to have no accepting descendant under `support`.
    dead: HashSet<Node>,
}

/// One depth of the [`CageSolutions`] DFS path.
struct Frame {
    state: State,
    /// The value on the edge from the parent frame (unused for the root).
    label: N,
    /// The next child value to try; `n + 1` once exhausted.
    next_v: N,
    /// Whether an accepting descendant has been found below this frame.
    found: bool,
}

impl CageSolutions<'_> {
    /// Pops the exhausted top frame: memoizes it as dead if no accepting
    /// descendant was found, otherwise propagates `found` to its parent.
    fn pop_frame(&mut self) {
        // The stack depth is a cage position index, bounded by k <= 9.
        #[allow(clippy::cast_possible_truncation)]
        let depth = (self.stack.len() - 1) as T;
        if let Some(frame) = self.stack.pop() {
            if frame.found {
                if let Some(parent) = self.stack.last_mut() {
                    parent.found = true;
                }
            } else {
                let _ = self.dead.insert(Node {
                    depth,
                    state: frame.state,
                });
            }
        }
    }
}

impl Iterator for CageSolutions<'_> {
    type Item = Vec<N>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let depth = self.stack.len().checked_sub(1)?;
            let frame = self.stack.last_mut()?;
            let v = frame.next_v;
            if v > self.dp.n {
                self.pop_frame();
                continue;
            }
            frame.next_v = v + 1;
            if !self.support[depth].contains(v) {
                continue;
            }
            // depth is a cage position index, bounded by k <= 9.
            #[allow(clippy::cast_possible_truncation)]
            let state = match self.dp.step(depth as T, &frame.state, v) {
                Step::Stop => {
                    frame.next_v = self.dp.n + 1;
                    continue;
                }
                Step::Skip => continue,
                Step::Tail(state) => state,
            };
            #[allow(clippy::cast_possible_truncation)]
            let child = Node {
                depth: (depth + 1) as T,
                state,
            };
            if self.dp.accept(child.depth, &child.state) {
                frame.found = true;
                let mut tuple: Vec<N> = self.stack.iter().skip(1).map(|f| f.label).collect();
                tuple.push(v);
                return Some(tuple);
            }
            if child.depth < self.dp.constraint.arity && !self.dead.contains(&child) {
                self.stack.push(Frame {
                    state: child.state,
                    label: v,
                    next_v: 1,
                    found: false,
                });
            }
        }
    }
}

/// Per-node DP state; depth is positional and lives in [`Node`].
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
struct State {
    /// Accumulated arithmetic value.
    value: T,
    /// Used-value sets for still-open collinear lines (indexed by line index).
    /// Closed lines are zeroed out so that they don't inflate the key space.
    /// `Fill` doubles as a compact bitset (u16) for tracking which values have
    /// been placed in a line; its bitmap operations serve set-membership here,
    /// not candidate-fill semantics.
    used: Box<[Fill]>,
}

/// Result of one [`CageDp::step`] transition.
#[derive(Debug)]
enum Step {
    /// The value and every larger value are infeasible: stop scanning values.
    Stop,
    /// This value alone is infeasible: skip it.
    Skip,
    /// The value is feasible: the successor state.
    Tail(State),
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

/// A hash-cons key for the MDD: the DP [`State`] at a given depth.
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
struct Node {
    depth: T,
    state: State,
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({} @ level {})", self.state.value, self.depth)
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

    /// A cage test case: `(n, k, op, target, lines)`.
    type CageCase = (N, N, CommutativeOperator, T, Vec<Vec<usize>>);

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

    // ---- CageSolutions iterator ----

    #[test]
    fn solutions_enumerates_l_cage_tuples_without_an_mdd() {
        setup();
        // The +6 L-cage in n=4: row line [0,1], column line [0,2].
        // Driven through the lazy iterator alone — no Mdd is constructed.
        let lines = vec![vec![0, 1], vec![0, 2]];
        let dp = CageDp::new(4, 3, Add, 6, &lines);
        let mut tuples: Vec<Vec<N>> = dp.solutions(&full_support(4, 3)).collect();
        tuples.sort();
        assert_eq!(
            tuples,
            vec![
                vec![1, 2, 3],
                vec![1, 3, 2],
                vec![2, 1, 3],
                vec![2, 3, 1],
                vec![3, 1, 2],
                vec![3, 2, 1],
                vec![4, 1, 1],
            ]
        );
    }

    #[test]
    fn solutions_agree_with_mdd_tuples_across_cage_shapes() {
        setup();
        let cases: Vec<CageCase> = vec![
            (4, 2, Add, 4, vec![vec![0, 1]]),              // collinear domino
            (4, 2, Add, 4, no_lines()),                    // free domino
            (4, 3, Add, 6, vec![vec![0, 1], vec![0, 2]]),  // L-cage
            (6, 3, Multiply, 24, vec![vec![0, 1, 2]]),     // 3-in-a-row
            (5, 4, Add, 12, vec![vec![0, 1], vec![2, 3]]), // S-tetromino
        ];
        for (n, k, op, target, lines) in cases {
            let dp = CageDp::new(n, k, op, target, &lines);
            let mut actual: Vec<Vec<N>> = dp.solutions(&full_support(n, k)).collect();
            actual.sort();
            let expected = sorted_tuples(&Mdd::new(n, k, op, target, &lines).unwrap());
            assert_eq!(actual, expected, "n={n} k={k} op={op:?} target={target}");
        }
    }

    #[test]
    fn solutions_existence_matches_mdd_construction_over_grid() {
        setup();
        for n in 2u8..=6 {
            for k in 2u8..=4 {
                let support = full_support(n, k);
                let max_sum = T::from(n) * T::from(k) + 1;
                for target in 1..=max_sum {
                    assert_existence_matches_mdd(n, k, Add, target, &support);
                }
                let max_product = T::from(n).pow(u32::from(k)) + 1;
                for target in 1..=max_product {
                    assert_existence_matches_mdd(n, k, Multiply, target, &support);
                }
            }
        }
    }

    #[test]
    fn solutions_feasibility_under_support_matches_build_and_narrow() {
        setup();
        // Restrict each position to each singleton in turn and check that the
        // early-exit witness agrees with a full build-and-narrow.
        let cases: Vec<CageCase> = vec![
            (4, 3, Add, 6, vec![vec![0, 1], vec![0, 2]]),
            (4, 2, Add, 4, vec![vec![0, 1]]),
            (6, 3, Multiply, 24, vec![vec![0, 1, 2]]),
        ];
        for (n, k, op, target, lines) in cases {
            for position in 0..usize::from(k) {
                for v in 1..=n {
                    let mut support = full_support(n, k);
                    support[position] = Fill::singleton(v);
                    let dp = CageDp::new(n, k, op, target, &lines);
                    let lazy = dp.solutions(&support).next().is_some();
                    let narrowed = Mdd::new(n, k, op, target, &lines)
                        .is_ok_and(|m| m.narrow(&support).is_ok());
                    assert_eq!(
                        lazy, narrowed,
                        "n={n} k={k} op={op:?} target={target} position={position} v={v}"
                    );
                }
            }
        }
    }

    #[test]
    fn solutions_is_exhausted_after_last_tuple() {
        setup();
        let dp = CageDp::new(4, 2, Add, 5, &no_lines());
        let support = full_support(4, 2);
        let mut solutions = dp.solutions(&support);
        assert_eq!(solutions.by_ref().count(), 4); // (1,4),(2,3),(3,2),(4,1)
        assert_eq!(solutions.next(), None);
        assert_eq!(solutions.next(), None);
    }

    // ---- tuple_count / multiset_count ----

    #[test]
    fn counts_match_tuples_oracle_across_cage_shapes() {
        setup();
        let cases: Vec<CageCase> = vec![
            (4, 2, Add, 5, no_lines()),                    // free domino
            (4, 2, Add, 4, vec![vec![0, 1]]),              // collinear domino
            (4, 3, Add, 6, vec![vec![0, 1], vec![0, 2]]),  // L-cage
            (6, 3, Multiply, 24, vec![vec![0, 1, 2]]),     // 3-in-a-row
            (5, 4, Add, 12, vec![vec![0, 1], vec![2, 3]]), // S-tetromino
            (9, 4, Add, 20, vec![vec![0, 1, 2, 3]]),       // 4-in-a-row, 9×9
        ];
        for (n, k, op, target, lines) in cases {
            let m = Mdd::new(n, k, op, target, &lines).unwrap();
            assert_counts_match_tuples(&m, &format!("n={n} k={k} op={op:?} target={target}"));
        }
    }

    #[test]
    fn counts_match_tuples_across_n_k_target() {
        setup();
        for n in 2u8..=6 {
            for k in 2u8..=4 {
                let max_sum = T::from(n) * T::from(k);
                for target in 1..=max_sum {
                    if let Ok(m) = Mdd::new(n, k, Add, target, &no_lines()) {
                        assert_counts_match_tuples(&m, &format!("n={n} k={k} Add {target}"));
                    }
                }
                let max_product = T::from(n).pow(u32::from(k));
                for target in 1..=max_product {
                    if let Ok(m) = Mdd::new(n, k, Multiply, target, &no_lines()) {
                        assert_counts_match_tuples(&m, &format!("n={n} k={k} Multiply {target}"));
                    }
                }
            }
        }
    }

    #[test]
    fn counts_match_tuples_after_narrow() {
        setup();
        // The +6 L-cage, narrowed position by position to each singleton.
        let lines = vec![vec![0, 1], vec![0, 2]];
        let m = Mdd::new(4, 3, Add, 6, &lines).unwrap();
        for position in 0..3 {
            for v in 1..=4 {
                let mut support = full_support(4, 3);
                support[position] = Fill::singleton(v);
                if let Ok(narrowed) = m.narrow(&support) {
                    assert_counts_match_tuples(&narrowed, &format!("position={position} v={v}"));
                }
            }
        }
    }

    #[test]
    fn l_cage_counts_are_two_multisets_seven_tuples() {
        setup();
        // The +6 L-cage in 4×4: six orderings of {1,2,3} plus (4,1,1).
        let lines = vec![vec![0, 1], vec![0, 2]];
        let m = Mdd::new(4, 3, Add, 6, &lines).unwrap();
        assert_eq!(m.tuple_count(), 7);
        assert_eq!(m.multiset_count(), 2);
    }

    fn assert_counts_match_tuples(m: &Mdd, context: &str) {
        let tuples = m.tuples();
        let multisets: HashSet<Vec<N>> = tuples
            .iter()
            .map(|t| {
                let mut s = t.clone();
                s.sort_unstable();
                s
            })
            .collect();
        assert_eq!(
            m.tuple_count(),
            u64::try_from(tuples.len()).unwrap(),
            "tuple count vs oracle: {context}"
        );
        assert_eq!(
            m.multiset_count(),
            u64::try_from(multisets.len()).unwrap(),
            "multiset count vs oracle: {context}"
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

    fn full_support(n: N, k: N) -> Vec<Fill> {
        vec![Fill::all(usize::from(n)); usize::from(k)]
    }

    fn assert_existence_matches_mdd(
        n: N,
        k: N,
        op: CommutativeOperator,
        target: T,
        support: &[Fill],
    ) {
        let dp = CageDp::new(n, k, op, target, &no_lines());
        assert_eq!(
            dp.solutions(support).next().is_some(),
            Mdd::new(n, k, op, target, &no_lines()).is_ok(),
            "n={n} k={k} op={op:?} target={target}"
        );
    }

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
