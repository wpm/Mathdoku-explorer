import { Page } from '@playwright/test';

export interface TauriStubOptions {
  initialPuzzle?: unknown;
  // Path returned by dialog.save / dialog.open pickers.
  saveDialogPath?: string | null;
  openDialogPath?: string | null;
  // Path returned by get_doc_state after a save.
  savedPath?: string | null;
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

  await page.addInitScript(
    ({ puzzle, saveDialogPath, openDialogPath, savedPath }) => {
      let currentPath: string | null = savedPath;

      (window as unknown as Record<string, unknown>)['__TAURI__'] = {
        core: {
          invoke: (cmd: string, args?: unknown) => {
            if (cmd === 'get_puzzle') return Promise.resolve(puzzle);
            if (cmd === 'get_doc_state')
              return Promise.resolve({ dirty: false, path: currentPath });
            if (cmd === 'save_puzzle') {
              const path =
                (args as { path?: string } | undefined)?.path ?? saveDialogPath;
              if (path) currentPath = path;
              return Promise.resolve({ path });
            }
            if (cmd === 'set_window_title') {
              const title =
                (args as { title?: string } | undefined)?.title ?? '';
              document.title = title;
              return Promise.resolve(null);
            }
            if (cmd === 'add_region') {
              // Add the region cells as a new region slot in the puzzle.
              const cells = (args as { cells?: { row: number; column: number }[] } | undefined)?.cells ?? [];
              const currentPuzzle = puzzle as { n: number; slots: unknown[] } | null;
              if (!currentPuzzle) return Promise.resolve(null);
              const newSlot = {
                Region: cells.map(({ row, column }) => ({ row, column })),
              };
              puzzle = { ...currentPuzzle, slots: [...currentPuzzle.slots, newSlot] };
              return Promise.resolve(puzzle);
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
    { puzzle, saveDialogPath, openDialogPath, savedPath },
  );
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
