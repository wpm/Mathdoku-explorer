import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';

const EMPTY_3 = { n: 3 };

test.describe('filename display', () => {
  test('app calls set_window_title with basename when puzzle has a backing file', async ({
    page,
  }) => {
    await installTauriStubs(page, EMPTY_3, {
      savedPath: '/home/user/my-puzzle.mathdoku',
    });

    // Record all set_window_title calls made by the app.
    await page.addInitScript(() => {
      (window as unknown as Record<string, string[]>)['__title_calls__'] = [];
      const orig = (
        window as unknown as {
          __TAURI__: {
            core: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          };
        }
      ).__TAURI__.core.invoke;
      (
        window as unknown as {
          __TAURI__: {
            core: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          };
        }
      ).__TAURI__.core.invoke = (cmd, args) => {
        if (cmd === 'set_window_title') {
          (window as unknown as Record<string, string[]>)[
            '__title_calls__'
          ].push((args as { title?: string })?.title ?? '');
        }
        return orig(cmd, args);
      };
    });

    await gotoApp(page);
    await waitForGrid(page);

    const titleCalls = await page.evaluate(
      () => (window as unknown as Record<string, string[]>)['__title_calls__'],
    );
    expect(titleCalls).toContain('my-puzzle.mathdoku');
  });

  test('app does not call set_window_title with a filename when no file is backing the puzzle', async ({
    page,
  }) => {
    await installTauriStubs(page, EMPTY_3);

    await page.addInitScript(() => {
      (window as unknown as Record<string, string[]>)['__title_calls__'] = [];
      const orig = (
        window as unknown as {
          __TAURI__: {
            core: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          };
        }
      ).__TAURI__.core.invoke;
      (
        window as unknown as {
          __TAURI__: {
            core: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          };
        }
      ).__TAURI__.core.invoke = (cmd, args) => {
        if (cmd === 'set_window_title') {
          (window as unknown as Record<string, string[]>)[
            '__title_calls__'
          ].push((args as { title?: string })?.title ?? '');
        }
        return orig(cmd, args);
      };
    });

    await gotoApp(page);
    await waitForGrid(page);

    const titleCalls = await page.evaluate(
      () => (window as unknown as Record<string, string[]>)['__title_calls__'],
    );
    // May be called with empty string (clearing the title), but never with a filename.
    expect(titleCalls.filter((t) => t !== '')).toHaveLength(0);
  });
});
