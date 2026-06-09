# ADR-0006: Commutative cage memos enforce collinear distinctness

**Status:** Accepted
**Date:** 2026-06-08
**Deciders:** Bill McNeill (Mathdoku owner)

## Context

ADR-0005 established that every cage owns a memo — derived data answering "which values can still fill this cage's cells" — and that commutative cages (add, multiply) memoize with an `Mdd`. That memo currently encodes only the **arithmetic** constraint. Projected to per-cell candidate fills, an arithmetic-only memo silently drops a constraint the cage cannot ignore: cells of a cage that share a row or column must hold distinct values.

The omission is visible at every scale. The smallest commutative cage is a domino, whose two cells are always collinear; a `+4` domino has arithmetic tuples `(1,3)`, `(2,2)`, `(3,1)`, but `(2,2)` places equal values in one line and is illegal. The motivating failure is the `+6` L-cage `A=(1,1)`, `B=(1,2)`, `C=(2,1)` (the failing test behind this work): the arithmetic memo lets every cell be `{1,2,3,4}`, so an arm reaches 4 via the tuple `(1,4,1)` — but that places two 1s in column 1. Only the corner `A` may legitimately be 4, through `(4,1,1)`, where the two 1s sit at the non-collinear arms.

When the puzzle is fully caged, grid-level all-different over complete rows and columns subsumes the cage's internal distinctness. But the cases that matter here are **partial** puzzles: interactive authoring in Designer, and the partial boards examined while generating a unique-solution puzzle. There the grid all-different sees too few cells to rule the illegal value out, and it hides in the cage's fills.

Four forces are at play.

**The missing constraint is a conjunction, not a second constraint to run alongside.** Enforcing GAC on the arithmetic relation and GAC on the all-different relation separately, iterated to a fixpoint, is strictly weaker than GAC on their conjunction. In the L-cage, the implication "`B=4` forces `A=C=1`" lives in the arithmetic relation and is invisible to a domain-only all-different; the resulting column collision is invisible to a domain-only projection of the arithmetic memo. Neither propagator can prune the arm's 4 at any number of iterations. The distinctness must therefore live **inside** the memo's relation.

**The current correct behavior is enumerative.** The existing collinear-aware path materializes the surviving tuple set and filters it for distinctness before projecting — exactly the tuple explosion the `Mdd` exists to avoid. This is the proximate cause of the multi-minute `possible_operations` feasibility test on a ten-cell cage.

**Representing the conjunction exactly can be expensive.** A fat block (e.g. a 3×4 cage) couples a sum with several overlapping row and column all-differents — partial-Latin-rectangle reasoning, whose exact relation is combinatorially large. Crucially, width and pruning value are anti-correlated: the relation is widest at loose targets, which is exactly where it prunes nothing, and narrow at tight targets, where it prunes hard.

**The reduction machinery should stay encapsulated.** The 4R reduction and the `remove_support` narrowing in `mdd.rs` are the intricate part of the codebase; a fix must not spread MDD internals into cage logic.

A secondary force is a preference for non-novel mechanisms with a known literature.

## Decision

**A commutative cage's memo represents the conjunction of its arithmetic constraint and its internal collinear all-different as a single relation, compiled as a state-based dynamic-programming MDD.** A node's identity is its DP state — `(depth, accumulated arithmetic value, used values per still-open collinear line)` — rather than `(depth, value)`. Distinctness is enforced during construction; `narrow` and per-cell projection remain enumeration-free.

The decision has three parts.

**Representation.** A "line" is a row or column the cage occupies with two or more members — one all-different scope. Placing a value at a cell writes it into the used-value set of each line the cell joins; an edge is forbidden when its value is already used by an open line; a line drops out of the node's identity once its last cell is placed. The used-sets must be part of the node key, not a side annotation: two paths that reach the same accumulated value while having used different values in an open line have different legal futures and must not merge. This is the single change that restores GAC — the rest of the diagram is as before.

**Framing as a DP over a hidden engine.** The 4R reduction and `remove_support` narrowing stay one object, generalized over an abstract DP state and kept behind a narrow interface; the cage-specific part is just the state and its transition. Arithmetic-only cages are the *same* engine with no lines — collinearity is data, not a separate code path. Every domino contributes exactly one line of size two, so two-cell add/multiply cages are the smallest instance of this mechanism (the `+4` domino drops `(2,2)`) rather than a special case. Non-commutative dominoes are collinear as well, but `|a−b| ≥ 1` and `max/min ≥ 2` already force the two values to differ, so their `Table` memo needs no distinctness step. The cell visit order is the polyomino's natural iteration order.

**Scope of exactness.** The exact conjunction is built for cages whose collinear structure keeps the diagram small — the common case, covering dominoes, L and T shapes, and snakes, where at most one or two lines are open at a time. Fat blocks are a known limit, recorded here and deferred; this ADR does not commit the unbounded exact builder to them.

This is textbook state-based decision-diagram compilation (Andersen, Hadžić, Hooker, Tiedemann, *A Constraint Store Based on Multivalued Decision Diagrams*, CP 2007; Hooker, *Decision Diagrams and Dynamic Programming*, CPAIOR 2013), not a new algorithm. A walkthrough of the L-cage compilation is in `algorithms/collinear_mdd_animation.html`.

## Options Considered

### Option A: Conjunction as a state-based DP-MDD — *chosen*

| Dimension | Assessment |
|-----------|------------|
| Completeness | Full — GAC on `sum ∧ distinctness` |
| Runtime narrow | Enumeration-free — structural prune + edge-scan projection |
| Construction width | Small for thin cages; blows up for fat blocks (deferred valve) |
| Encapsulation | 4R reduction hidden behind the engine's DP interface |
| Novelty | Established technique |

**Pros:** Fixes the bug at the memo level, for every cage size, with the smallest cages handled by the same mechanism. Narrow and projection never enumerate. One engine serves arithmetic-only and conjoined cages. The intricate reduction stays hidden.
**Cons:** Introduces a new correctness-critical object — the DP state — that must be a sufficient statistic. Exact construction width is unbounded on fat blocks until a relaxation valve exists.

### Option B: Decomposed propagators (arithmetic memo ⊕ Régin all-different, iterated)

| Dimension | Assessment |
|-----------|------------|
| Completeness | **Incomplete** — cannot prune the L-cage 4 at any fixpoint |
| Runtime narrow | Cheap |
| Construction width | None beyond today |
| Encapsulation | Reuses `regin_gac` as-is |
| Novelty | None |

**Pros:** Cheap and reuses the existing all-different propagator.
**Cons:** Provably too weak — decomposition GAC is strictly weaker than conjunction GAC, so the motivating bug survives untouched. This option is the one whose failure justifies the ADR.

### Option C: Conjunction by enumeration (status-quo workaround, or an explicit conjoined table)

| Dimension | Assessment |
|-----------|------------|
| Completeness | Full |
| Runtime narrow | Enumerates the tuple set |
| Construction width | Materializes the conjoined relation |
| Encapsulation | Mixes tuple filtering into cage logic |
| Novelty | None |

**Pros:** Simple and obviously correct; for a tiny cage the conjoined tuple list is small and a plain table is the easiest thing that works.
**Cons:** Pays exactly the enumeration the MDD exists to avoid, on the hot path; cost is unbounded for large or loose cages; it is already the source of the minutes-long feasibility test.

## Trade-off Analysis

The A-versus-B comparison is about correctness, and it is not close: B is cheaper but cannot solve the problem, because decomposition weakness is a property of the relation, not a tuning parameter. The A-versus-C comparison is about scale. Both are correct, but C pays the tuple enumeration the memo was built to avoid, and pays it every time it narrows; A pays only in construction width, and only for fat or loose cages — precisely the cages where, by the width/pruning anti-correlation, there is almost nothing to prune, so a future width cap forfeits little. For the thin cages that dominate real puzzles, A's diagram never widens and C's enumeration would also be cheap, so A matches C where C is fine and stays bounded everywhere C is not.

A's genuine costs are a new risk surface (the DP state must be a sufficient statistic) and a deferred limit (fat-block width). The first is bounded by tests; the second is bounded by a known, literature-standard escape hatch. C's central cost — unbounded enumeration on the propagation path — is bounded by neither.

A note on storage rather than relation: because distinctness removes many tuples, the conjoined relation is often far smaller than the arithmetic-only one, so for a small cage an explicit list can be the right *storage*, and the engine degrades to exactly that when its merging buys nothing. That is an implementation detail the `Memo` trait already hides; the decision here is the conjoined **relation** and its DP compilation, not the container a given cage stores it in.

## Consequences

- Per-cell fills become GAC-correct for partial puzzles; the L-cage failure is fixed at the memo level, and dominoes get distinctness from the same mechanism with no special case.
- `narrow` and projection become enumeration-free. The enumerative collinear path in cage propagation, and the post-narrow re-derivation of fills by listing tuples, are removed in favor of reading fills off surviving edges.
- One engine serves both arithmetic-only and conjoined cages; arithmetic-only is the no-lines case of the same code.
- The 4R reduction and `remove_support` narrowing stay encapsulated behind the engine's DP interface; cage logic never touches nodes, edges, or cascades.
- The new correctness-critical element is the DP state: it must include the used-set of every open line and drop a line once closed. This is the entire bug class, and property tests across cage shapes (domino, L, T, block, snake) are the guard.
- Fat blocks remain a known limit. The unbounded exact builder must not be handed a fat block once the generator can produce them; the resolution is a future relaxed / limited-width MDD — a width cap with intersection-merge of the used-sets, of which arithmetic-only is the width-one limit — recorded as its own decision.
- Ordering the cells to reduce the number of simultaneously-open lines is a future width lever tied to that deferred work, not a correctness concern; the present design uses the polyomino's natural order.
- This builds on ADR-0005 without disturbing it: the cage still owns its memo and the puzzle still gates construction. ADR-0006 fixes only what the commutative memo's relation is and how it is compiled. Downstream decisions — short-circuiting the feasibility queries (`possible_operations`, `possible_targets`) and counting solutions for unique-puzzle generation — depend on this representation but are settled separately.
