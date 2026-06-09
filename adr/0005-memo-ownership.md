# ADR-0005: Cage owns its memo; Puzzle owns cage lifecycle

**Status:** Proposed
**Date:** 2026-06-04
**Deciders:** Bill McNeill (Mathdoku owner)

## Context

The mdk rewrite (`mathdoku/src/mdk`) gives every cage a **memo**: a precomputed structure that answers "which values can still fill this cage's cells" without re-enumerating assignments. Commutative cages (add, multiply) memoize with an `Mdd`; non-commutative cages (subtract, divide) memoize with a `DominoTable`; given cages need only the fixed value itself.

A memo is pure derived data. It is computed from exactly three inputs — the grid size `n`, the cage's polyomino, and its operation — and it has a lifecycle unlike everything else in the model:

- It is **never serialized**. A saved puzzle stores grid, polyominos, and operations; memos are rebuilt from those on load.
- It is **replaced during propagation**. `Memo::remove` is functional — it returns a new memo with candidates pruned — so constraint propagation produces new memo values rather than mutating in place.
- It **cannot be built without `n`**, and `n` belongs to the puzzle, not the cage.

Meanwhile the puzzle's cage container is a lookup table, `Cell → Cage`, in which every cell of a multi-cell cage must resolve to the *same logical cage*. A naive `HashMap<Cell, Cage>` stores k independent clones of a k-cell cage, each with its own memo; propagation would then have to find and update all k copies in lockstep, an invariant maintained by discipline rather than by the compiler.

These forces pull the memo in opposite directions. Data locality says the memo belongs on the `Cage` — each cage has exactly one, and every memo read starts from a cage. Lifecycle says it belongs to the `Puzzle` — only the puzzle knows `n`, only the puzzle can rebuild memos on deserialization, and only the puzzle orchestrates the propagation that replaces them. A subsidiary technical force: serde cannot pass context (such as `n`) into a `Deserialize` impl without `DeserializeSeed`, so any design in which a `Cage` deserializes *itself* back into possession of a memo is fighting the serialization framework.

The puzzle is therefore: place the memo so that reads are direct, the k-copies consistency problem cannot arise, deserialization rebuilds memos without serde contortions, and the "a cage always has a valid memo" invariant is enforced by construction rather than checked at runtime.

## Decision

**The `Cage` owns its memo as a non-optional field. The `Puzzle` owns every `Cage` behind an `Arc`, and is the only place a `Cage` can be constructed.** Ownership of the data and ownership of the lifecycle are deliberately split between the two objects.

Concretely:

`Puzzle` stores `HashMap<Cell, Arc<Cage>>`. All k cells of a cage map to the same `Arc`, so the k-copies problem is structurally impossible: there is one cage object, and the cells alias it. The map remains an ordinary mutable lookup table — user-driven cage insertion and removal are a loop over the polyomino's cells inserting or removing the shared `Arc`.

`Cage` construction requires `n` and builds the memo eagerly. The constructor is `pub(crate)`; the public path to creating a cage is `Puzzle::insert(polyomino, operation)`, which supplies `n` from its own state. Because construction is the only way to obtain a `Cage` and construction always builds the memo, the memo field needs no `Option`: a `Cage` without a memo is unrepresentable. With `Cage` as an enum over operator class (per the operator-hierarchy design), each variant owns its memo *type* directly — `Mdd` in the commutative variant, `DominoTable` in the non-commutative variant, no memo in the given variant — so "wrong memo kind for this operator" is also unrepresentable, and the `CageMemo` wrapper enum disappears.

Memo replacement follows a **replace-never-mutate** rule. `Arc` provides shared immutable access; propagation builds a new `Cage` carrying the pruned memo and re-points all k cells at the new `Arc`. (`Arc::make_mut` is explicitly off the table: with refcount > 1 it clones, silently un-sharing the cage across its cells.)

Serialization is implemented on `Puzzle`, not `Cage`, via a wire type. Serializing the cell map naively would write each cage k times and deserialize into k separate `Arc`s, recreating the duplication problem on the wire. Instead `Puzzle`'s `Serialize` emits each unique cage once — the cage's lexicographically-first cell serves as an anchor for deduplication — as `(polyomino, operation)` pairs with no memo. `Puzzle`'s `Deserialize` reconstructs each `Cage` through the normal constructor, which rebuilds the memo from `n`. The memo's never-serialized property is thus a consequence of the wire type's shape, not a `#[serde(skip)]` annotation that leaves a hole behind.

This decision covers the persistent model only. Solver search state — the trailed sparse-set structures of ADR-0004 — is transient state built *from* the model when search begins, mutated in place under `&mut`, and dropped when search ends. It lives outside the `Arc` entirely; the conflict between trail-based in-place mutation and `Arc`'s shared immutability never arises because the two never hold the same data.

## Options Considered

### Option A: Cage owns memo non-optionally; Puzzle gates construction and holds `Arc<Cage>` — *chosen*

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low — one `Arc`, one wire type, one `pub(crate)` constructor |
| Memo reads | Direct — `cage.memo`, no indirection, no `Option` |
| k-copies consistency | Structural — cells alias one object |
| Serde | Custom impl on `Puzzle` only; `Cage` needs none |
| Invariant enforcement | By construction — memo-less or wrong-memo cages unrepresentable |

**Pros:** Memo reads are a field access. The "always has a valid memo" invariant is compiler-enforced via the gated constructor. `Eq`/`Ord`/`Debug` on `Cage` describe the whole value honestly. Cage insertion/removal stays a plain map operation. Serde complexity is concentrated in one wire type on `Puzzle`.

**Cons:** Memo replacement rebuilds the whole `Cage` (clone polyomino, copy operation, re-point k cells) even though only the memo changed. The replace-never-mutate rule is a convention the compiler does not check — `Arc::make_mut` compiles and is wrong. Custom serde on `Puzzle` is hand-written code that derives would otherwise provide.

### Option B: Puzzle owns memos in a parallel arena

`Cage` is a pure value (polyomino + operation). `Puzzle` stores `cages: Vec<Cage>`, `memos: Vec<CageMemo>`, and `cell_to_cage: HashMap<Cell, usize>`.

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium — index bookkeeping replaces the map's directness |
| Memo reads | Indirect — cell → index → memos[i] |
| k-copies consistency | Structural — indices alias one slot |
| Serde | Trivial — `Cage` derives; memos vec skipped and rebuilt |
| Invariant enforcement | Runtime — `memos.len() == cages.len()` is checked, not typed |

**Pros:** `Cage` derives everything, including serde. Memo lifecycle code (rebuild on load, replace on propagation) lives entirely in `Puzzle`, matching the lifecycle ownership. Throwaway memos for feasibility queries (`possible_operations`, `possible_targets`) need no cage at all.

**Cons:** Cage removal either leaves holes in the arena or invalidates indices; user-driven insert/remove — a first-class operation in Designer — makes stable indexing genuinely fiddly rather than theoretically fiddly. The cages/memos parallel-array invariant is enforced by discipline. Every memo read pays the indirection. The memo type is divorced from the operator class, so "wrong memo kind" returns as a representable state.

### Option C: Cage owns `Option<CageMemo>`; plain `Cage` map values (status quo)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to write, high to use — every read handles `None` |
| Memo reads | Indirect — `Option` check, then `CageMemo` dispatch |
| k-copies consistency | None — k independent clones per k-cell cage |
| Serde | `#[serde(skip)]` on memo; deserialized cages have `None` |
| Invariant enforcement | None — memo-less cages are the post-deserialization norm |

**Pros:** No `Arc`, no wire type, derives mostly work.

**Cons:** The `Option` is a maybe-initialized state that infects every read path — `Puzzle::get` already needs a grid fallback for the `None` case. `PartialEq`/`Ord`/`Debug` must pretend the memo field doesn't exist. The k-copies problem is live: propagation must locate and consistently update every clone. Deserialization produces structurally valid but functionally degraded puzzles, and nothing forces the memos to ever be rebuilt.

## Trade-off Analysis

The decisive comparison is A versus B, and it is a trade of **construction-time discipline against operation-time discipline**. Option A pays once, at the boundary: a gated constructor and a hand-written wire type, both in one file, after which the type system holds the invariants. Option B pays continuously: every insert, remove, and propagation step must keep two vectors and an index map coherent, and the compiler verifies none of it. Since cage insertion and removal are user-facing operations in Designer — not rare setup steps — the continuous cost lands on the hot path of exactly the code that changes most.

Option A's real weakness, rebuild-on-replace, is bounded: cages are small (k ≤ 5 in practice), and if profiling ever shows the polyomino clone mattering, `Arc<Polyomino>` inside the cage makes the rebuild O(1) without changing the design. Option B's weaknesses — index stability under removal, representable invalid states — are not bounded by a local fix; they are the design.

Option C is the null option and is dominated: it has Option B's representable-invalid-states problem plus a consistency problem neither A nor B has.

## Consequences

- Every memo read is a direct field access on a cage that provably has one. `Puzzle::get` loses its grid-fallback branch.
- `Cage::new` moves out of the public API; `Puzzle::insert(polyomino, operation)` is the public construction path and the place arity rules (non-commutative ⇒ domino, given ⇒ singleton) are checked.
- `Puzzle` grows a hand-written serde impl and a wire type; `Cage` and its memo types need no serde at all.
- Propagation code must follow replace-never-mutate. A clippy lint or a doc comment on the map field should warn against `Arc::make_mut`; this is the one invariant the compiler does not hold.
- Snapshot-style cloning of `Puzzle` (e.g., for feasibility queries that extend the puzzle with a candidate cage) is cheap: cloning the map clones `Arc`s, not cages.
- The solver's trailed search state (ADR-0004) is built from, and never aliased into, the `Arc`'d model. If a future design wants the *interactive* propagation path to share machinery with search, that integration happens by constructing solver state from the model, not by threading `&mut` into the `Arc`.
- If memo construction ever becomes expensive enough that eager rebuild-on-load hurts (large `n`, many cages), lazy memo construction would reintroduce the maybe-initialized state this ADR exists to eliminate; the correct lever at that point is making memo construction cheaper or parallel, not optional.
