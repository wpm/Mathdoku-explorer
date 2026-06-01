import { test, expect } from '@playwright/test';
import { gotoApp, waitForGrid } from '../tests/helpers';
import { ENTER, SHIFT_ARROW_RIGHT } from '../tests/keys';

// End-to-end smoke test of the WASM-everything preview build (issue #74).
//
// Unlike every spec under ../tests, this one installs NO window.__TAURI__
// stubs: each puzzle-state command runs in-process against
// mathdoku-designer-core through the `#[cfg(feature = "web")]` ipc bodies and
// the thread-local AppState store. It locks in that the migration actually made
// the preview editor functional — create → select → insert cage → fix toggle —
// rather than just rendering chrome. Driven by playwright.web.config.ts, which
// serves a `trunk serve --features web` build on :1421.
test.describe('web build editor flow', () => {
  test('create puzzle, select cell, insert cage, reach fix toggle', async ({
    page,
  }) => {
    await gotoApp(page);

    // With no puzzle in the thread-local store, get_puzzle returns None and the
    // New-puzzle modal appears. Create a Without-Solution puzzle (new_empty).
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();
    await page
      .getByRole('button', { name: 'No Solution', exact: true })
      .click();
    await waitForGrid(page);

    // Selection (set_active_cell): focus the grid, draw the domino {(0,0),(0,1)}.
    await page.locator('.grid-svg').focus();
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);

    // Insert cage (insert_cage): pick Add, then the feasible target 3 (1 + 2).
    const selectorLabels = page.locator('.grid-svg text[font-weight="700"]');
    await selectorLabels.filter({ hasText: /^\+$/ }).click();
    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeFocused();
    await targetSelect.selectOption('3');

    // The committed +3 label proves insert_cage round-tripped through core.
    await expect(
      page.locator('.grid-svg text').filter({ hasText: /^\+3$/ }),
    ).toBeVisible();

    // The puzzle is still in Without-Solution mode (Fix/Unfix moved to the
    // Puzzle menu / shortcuts; see puzzle_menu_shortcuts.test.ts).
    await expect(page.locator('.puzzle-wrap')).toHaveAttribute(
      'data-solution-mode',
      'without',
    );
  });
});
