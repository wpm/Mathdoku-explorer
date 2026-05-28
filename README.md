# Mathdoku

[![CI](https://github.com/wpm/Mathdoku/actions/workflows/ci.yml/badge.svg)](https://github.com/wpm/Mathdoku/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wpm/Mathdoku/branch/main/graph/badge.svg)](https://codecov.io/gh/wpm/Mathdoku)

A Rust workspace for generating, solving, and designing
[Mathdoku](https://en.wikipedia.org/wiki/KenKen) (KenKen) puzzles.

> **Want to _play_ Mathdoku?** End-user gameplay documentation will live on the
> project website (coming soon). This README is for contributors and for
> developers consuming the `mathdoku` crate.

## Workspace layout

| Path | Crate | Description |
|------|-------|-------------|
| `mathdoku/` | `mathdoku` | Core library: puzzle representation, constraint propagation, solver, and generator. |
| `apps/designer/` | `mathdoku-designer-ui` | Leptos/WASM UI for the desktop designer. |
| `apps/designer/core/` | `mathdoku-designer-core` | Platform-independent designer logic. |
| `apps/designer/src-tauri/` | `mathdoku-designer-tauri` | Tauri desktop shell. |
| `adr/` | — | Architecture Decision Records. |

Only `mathdoku` is intended for publication to crates.io. The designer crates
are marked `publish = false`.

## Prerequisites

- A stable Rust toolchain. `mathdoku` sets `rust-version = "1.87"`; match or
  exceed it.
- For the designer: the `wasm32-unknown-unknown` target, [Trunk], and the
  [Tauri CLI] for the desktop shell.
- For the end-to-end tests: Node 22+ and the Playwright Chromium browser
  (`apps/designer/e2e/`).

[Trunk]: https://trunkrs.dev/
[Tauri CLI]: https://tauri.app/

## Using the `mathdoku` library

The crate is not yet published to crates.io, so depend on it via git for now:

```toml
[dependencies]
mathdoku = { git = "https://github.com/wpm/Mathdoku" }
```

Generate a random puzzle and solve it:

```rust
use mathdoku::{generate, Grid};

let mut rng = rand::rng();
let puzzle = generate(4, &mut rng)?;        // random 4×4 cage structure
let empty = Grid::new(4)?;                  // grid with every cell holding all candidates
let solved = empty
    .solutions(&puzzle)
    .next()
    .expect("a generated puzzle has at least one solution")?;
```

See the crate-level documentation (`cargo doc -p mathdoku --open`) for the full
API, including programmatic puzzle construction with `Puzzle::new` /
`Puzzle::insert_cage`.

## Building and testing

The authoritative command set lives in [`.github/workflows/ci.yml`] and the
shared [`.githooks/pre-commit`] hook. The essentials:

```sh
# Core library
cargo build -p mathdoku
cargo test --lib -p mathdoku
cargo doc --no-deps -p mathdoku

# Designer (run from its directory)
cd apps/designer && cargo test
```

Some library tests are marked `#[ignore]` because they are slow; run them with
`cargo test --lib -p mathdoku -- --include-ignored`.

[`.github/workflows/ci.yml`]: .github/workflows/ci.yml
[`.githooks/pre-commit`]: .githooks/pre-commit

## Running the designer

- **Web preview** (client-side rendering): from `apps/designer/`, run
  `trunk serve` and open <http://localhost:1420>.
- **Desktop app**: with the [Tauri CLI] installed, run `cargo tauri dev` from
  `apps/designer/src-tauri/`.

## Contributing

- Enable the shared git hooks so your commits run the same checks as CI:
  `git config core.hooksPath .githooks`.
- The workspace enforces a strict lint policy (`clippy::all`, `pedantic`,
  `nursery`, plus denied `unwrap`/`expect`/`panic`/`todo` paths). The
  `mathdoku` crate has not yet opted into the full workspace lints pending an
  error-handling cleanup; see issue #59.
- Significant design decisions are recorded as ADRs under `adr/`. Add a new one
  when proposing an architecturally significant change.
- Note user-facing library changes in [`mathdoku/CHANGELOG.md`] under the
  `[Unreleased]` section.

[`mathdoku/CHANGELOG.md`]: mathdoku/CHANGELOG.md

## License

Licensed under the [MIT License](LICENSE).
