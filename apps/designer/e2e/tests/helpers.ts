import { Page } from '@playwright/test';

export interface TauriStubOptions {
  initialPuzzle?: unknown;
  // Path returned by dialog.save / dialog.open pickers.
  saveDialogPath?: string | null;
  openDialogPath?: string | null;
  // Path returned by get_doc_state after a save.
  savedPath?: string | null;
  // When true, the initial state is Without-Solution (solution = null).
  withoutSolution?: boolean;
}

// Stub window.__TAURI__ before page load.
// initialPuzzle: returned by get_puzzle (null = no puzzle loaded).
// saveDialogPath: returned by dialog.save (null = user cancelled).
// openDialogPath: returned by dialog.open (null = user cancelled).
// savedPath: returned by get_doc_state.path after any save.
export async function installTauriStubs(
  page: Page,
  initialPuzzleOrOpts: unknown = null,
  opts: TauriStubOptions = {},
) {
  // Accept legacy positional call: installTauriStubs(page, puzzle)
  const puzzle =
    initialPuzzleOrOpts !== null &&
    typeof initialPuzzleOrOpts === 'object' &&
    !('initialPuzzle' in (initialPuzzleOrOpts as object))
      ? initialPuzzleOrOpts
      : (opts.initialPuzzle ?? initialPuzzleOrOpts);

  const saveDialogPath = opts.saveDialogPath ?? null;
  const openDialogPath = opts.openDialogPath ?? null;
  const savedPath = opts.savedPath ?? null;
  const withoutSolution = opts.withoutSolution ?? false;

  await page.addInitScript(
    ({
      puzzle,
      saveDialogPath,
      openDialogPath,
      savedPath,
      withoutSolution,
    }) => {
      let currentPath: string | null = savedPath;
      // Mode flag: With-Solution serializes a non-null `solution`, Without-Solution null.
      let hasSolution = !withoutSolution;

      // Wrap a bare { n, cages? } puzzle into the State wire format that the
      // Rust backend now returns: { puzzle, solution, active, provisional_cages }.
      // `solution` is null in Without-Solution mode.
      // Cell wire format: [row, col] 1-indexed (matching Rust's Cell(usize,usize) tuple struct).
      type BareP = { n: number; cages?: unknown[] } | null;
      const wrapState = (p: BareP) =>
        p
          ? {
              puzzle: p,
              solution: hasSolution ? { n: p.n } : null,
              active: [1, 1],
              provisional_cages: [],
            }
          : null;

      (window as unknown as Record<string, unknown>)['__TAURI__'] = {
        core: {
          invoke: (cmd: string, args?: unknown) => {
            if (cmd === 'get_puzzle')
              return Promise.resolve(wrapState(puzzle as BareP));
            if (cmd === 'get_doc_state')
              return Promise.resolve({ dirty: false, path: currentPath });
            if (cmd === 'new_empty' || cmd === 'new_latin_square') {
              const n = (args as { n?: number } | undefined)?.n ?? 9;
              hasSolution = cmd === 'new_latin_square';
              puzzle = { n, cages: [] };
              return Promise.resolve(wrapState(puzzle as BareP));
            }
            if (cmd === 'fix') {
              hasSolution = true;
              return Promise.resolve(wrapState(puzzle as BareP));
            }
            if (cmd === 'unfix') {
              hasSolution = false;
              return Promise.resolve(wrapState(puzzle as BareP));
            }
            if (cmd === 'save_puzzle') {
              const path =
                (args as { path?: string } | undefined)?.path ?? saveDialogPath;
              if (path) currentPath = path;
              return Promise.resolve({ path });
            }
            if (cmd === 'set_window_title') {
              document.title =
                (args as { title?: string } | undefined)?.title ?? '';
              return Promise.resolve(null);
            }
            if (cmd === 'remove_cage_at') {
              // polyomino arg is [[row,col],...] 1-indexed tuples.
              const typedArgs = args as
                | { polyomino?: [number, number][] }
                | undefined;
              const removeCells = typedArgs?.polyomino ?? [];
              const currentPuzzle = puzzle as BareP;
              if (!currentPuzzle) return Promise.resolve(null);
              const cages = (currentPuzzle.cages ?? []).filter(
                (cage: unknown) => {
                  const c = cage as { polyomino?: [number, number][] };
                  const cageSet = new Set(
                    (c.polyomino ?? []).map(([r, col]) => `${r},${col}`),
                  );
                  return !(
                    removeCells.length === cageSet.size &&
                    removeCells.every(([r, col]) => cageSet.has(`${r},${col}`))
                  );
                },
              );
              puzzle = { n: currentPuzzle.n, cages };
              return Promise.resolve(wrapState(puzzle as BareP));
            }
            if (cmd === 'insert_cage') {
              // polyomino arg is [[row,col],...] 1-indexed tuples.
              // operator is a string enum variant; target is a number.
              const typedArgs = args as
                | {
                    polyomino?: [number, number][];
                    operator?: string;
                    target?: number | null;
                  }
                | undefined;
              const cells = typedArgs?.polyomino ?? [];
              const operator = typedArgs?.operator ?? 'Given';
              const currentPuzzle = puzzle as BareP;
              if (!currentPuzzle) return Promise.resolve(null);
              // Without-Solution mode supplies the target; With-Solution derives it.
              // Use the maximum valid target so the cage stays feasible even after
              // other cages have narrowed adjacent cell domains.
              // Max sum for k cells with distinct values 1..n: k*n - k*(k-1)/2
              // Max product: n! / (n-k)!
              // Subtract (2 cells): n-1 (largest gap between distinct values)
              // Divide (2 cells): n (n/1 is the largest valid ratio)
              const k = cells.length;
              const n = currentPuzzle.n;
              const maxAdd = k * n - (k * (k - 1)) / 2;
              const maxMul = Array.from({ length: k }, (_, i) => n - i).reduce(
                (a, b) => a * b,
                1,
              );
              // For Given singletons, derive a cell-unique value using the latin
              // square pattern. Cells are 1-indexed so subtract 1 to get 0-indexed coords.
              const givenTarget =
                cells.length === 1
                  ? ((cells[0][0] - 1 + cells[0][1] - 1) % n) + 1
                  : 1;
              const defaultTarget =
                operator === 'Given'
                  ? givenTarget
                  : operator === 'Add'
                    ? maxAdd
                    : operator === 'Multiply'
                      ? maxMul
                      : operator === 'Subtract'
                        ? n - 1
                        : n; // Divide: largest valid ratio is n/1 = n
              const target = typedArgs?.target ?? defaultTarget;
              // Cage wire format: polyomino [[r,c],...] 1-indexed, operation string, target number.
              const newCage = {
                polyomino: cells,
                operation: operator,
                target,
              };
              const cages = [...(currentPuzzle.cages ?? []), newCage];
              puzzle = { n: currentPuzzle.n, cages };
              return Promise.resolve(wrapState(puzzle as BareP));
            }
            return Promise.resolve(null);
          },
        },
        event: {
          listen: () => Promise.resolve(() => {}),
        },
        dialog: {
          save: () => Promise.resolve(saveDialogPath),
          open: () => Promise.resolve(openDialogPath),
        },
      };
    },
    { puzzle, saveDialogPath, openDialogPath, savedPath, withoutSolution },
  );
}

// A 3×3 puzzle with two cages used across multiple test suites.
// Cage 0: cells (0,0),(0,1) — Add(3)
// Cage 1: cell  (0,2)       — Given(3)
// Cell wire format: [row, col] 1-indexed.
// Cage wire format: { polyomino: [[r,c],...], operation: "Op", target: N }
export const PUZZLE_3 = {
  n: 3,
  cages: [
    {
      polyomino: [
        [1, 1],
        [1, 2],
      ],
      operation: 'Add',
      target: 3,
    },
    {
      polyomino: [[1, 3]],
      operation: 'Given',
      target: 3,
    },
  ],
};

// Intercepts window.__TAURI__.core.invoke and records all calls to `commandName`
// into a page-global array at `window[arrayKey]`. Returns the array key.
export async function interceptInvokeCommand(
  page: Page,
  commandName: string,
  arrayKey = '__intercepted_calls__',
): Promise<string> {
  await page.addInitScript(
    ({ commandName, arrayKey }) => {
      (window as unknown as Record<string, unknown[]>)[arrayKey] = [];
      const tauri = (
        window as unknown as {
          __TAURI__: {
            core: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          };
        }
      ).__TAURI__;
      const orig = tauri.core.invoke;
      tauri.core.invoke = (cmd, args) => {
        if (cmd === commandName) {
          (window as unknown as Record<string, unknown[]>)[arrayKey].push(args);
        }
        return orig(cmd, args);
      };
    },
    { commandName, arrayKey },
  );
  return arrayKey;
}

// Navigate to the app and wait for the WASM to mount.
export async function gotoApp(page: Page) {
  await page.goto('/');
  await page.waitForSelector('main.app-main', { timeout: 15_000 });
}

// Wait for the grid SVG to be visible and ready to receive keyboard input.
// Use this before any key-interaction tests to avoid races with WASM startup.
export async function waitForGrid(page: Page) {
  await page.waitForSelector('.grid-svg', { timeout: 15_000 });
}
