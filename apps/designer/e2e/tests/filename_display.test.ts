import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid, interceptInvokeCommand } from './helpers';

const EMPTY_3 = { n: 3 };
const TITLE_CALLS = '__title_calls__';

test.describe('filename display', () => {
  test('app calls set_window_title with basename when puzzle has a backing file', async ({
    page,
  }) => {
    await installTauriStubs(page, EMPTY_3, {
      savedPath: '/home/user/my-puzzle.mathdoku',
    });
    await interceptInvokeCommand(page, 'set_window_title', TITLE_CALLS);
    await gotoApp(page);
    await waitForGrid(page);

    const titleCalls = await page.evaluate(
      (key) =>
        (window as unknown as Record<string, { title?: string }[]>)[key].map(
          (a) => a?.title ?? '',
        ),
      TITLE_CALLS,
    );
    expect(titleCalls).toContain('my-puzzle.mathdoku');
  });

  test('app does not call set_window_title with a filename when no file is backing the puzzle', async ({
    page,
  }) => {
    await installTauriStubs(page, EMPTY_3);
    await interceptInvokeCommand(page, 'set_window_title', TITLE_CALLS);
    await gotoApp(page);
    await waitForGrid(page);

    const titleCalls = await page.evaluate(
      (key) =>
        (window as unknown as Record<string, { title?: string }[]>)[key].map(
          (a) => a?.title ?? '',
        ),
      TITLE_CALLS,
    );
    // May be called with empty string (clearing the title), but never with a filename.
    expect(titleCalls.filter((t) => t !== '')).toHaveLength(0);
  });
});
