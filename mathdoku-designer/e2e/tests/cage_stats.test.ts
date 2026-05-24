import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';

// 3×3 puzzle: two cages.
// Slot 0: cells (0,0),(0,1) — horizontal domino, Add(3) → 2 Tuples, 1 Multiset
// Slot 1: cell  (0,2)       — singleton, Given(3)      → 1 Tuple,  1 Multiset
const PUZZLE_3 = {
  n: 3,
  slots: [
    {
      Cage: {
        polyomino: [
          { row: 0, column: 0 },
          { row: 0, column: 1 },
        ],
        operation: { Add: 3 },
        n: 3,
      },
    },
    {
      Cage: {
        polyomino: [{ row: 0, column: 2 }],
        operation: { Given: 3 },
        n: 3,
      },
    },
  ],
};

async function enterSlotMode(page: import('@playwright/test').Page) {
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
  await page.keyboard.press('Tab');
}

test.describe('cage stats', () => {
  test('no stats shown in Cell Mode when no cage selected', async ({
    page,
  }) => {
    // Start at (0,0) in Cell Mode — that cell IS in a cage, so stats should show.
    // Navigate to a cell NOT in any cage to confirm stats disappear.
    await installTauriStubs(page, {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [{ row: 0, column: 0 }],
            operation: { Given: 1 },
            n: 3,
          },
        },
      ],
    });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    // Move to (1,0) — not in any cage.
    await page.keyboard.press('ArrowDown');

    await expect(page.locator('.cage-stats')).toBeHidden();
  });

  test('Cell Mode: stats shown when active cell is in a cage', async ({
    page,
  }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);

    // Default start: Cell Mode at (0,0), which is in the Add(3) domino.
    // Add(3) over a 3×3 row pair: 2 tuples, 1 multiset.
    await expect(page.locator('.cage-stats')).toContainText('1 Multiset');
    await expect(page.locator('.cage-stats')).toContainText('2 Tuples');
  });

  test('Slot Mode: stats shown for selected cage slot', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // selects slot 0: Add(3) domino

    await expect(page.locator('.cage-stats')).toContainText('1 Multiset');
    await expect(page.locator('.cage-stats')).toContainText('2 Tuples');
  });

  test('Slot Mode: advancing to next slot updates stats', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // slot 0
    await page.keyboard.press('Tab'); // slot 1: Given(3) singleton

    // Singleton: exactly 1 viable tuple and 1 multiset.
    await expect(page.locator('.cage-stats')).toContainText('1 Multiset');
    await expect(page.locator('.cage-stats')).toContainText('1 Tuple');
  });

  test('"Tuple" is singular when count is 1', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);
    await page.keyboard.press('Tab'); // Given(3) singleton → 1 Tuple

    await expect(page.locator('.cage-stats')).toContainText('1 Tuple');
    await expect(page.locator('.cage-stats')).not.toContainText('1 Tuples');
  });

  test('"Tuple" is plural when count is > 1', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // Add(3) domino → 2 Tuples

    await expect(page.locator('.cage-stats')).toContainText('2 Tuples');
    await expect(page.locator('.cage-stats')).not.toContainText('2 Tuple,');
  });

  test('cage stats text is aligned with inner grid border', async ({
    page,
  }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);

    const svgBox = await page.locator('.grid-svg').boundingBox();
    // The SVG viewBox is 600 units wide with MARGIN=14 before the grid border.
    // padding-left on .cage-stats shifts text to match; verify via computed style.
    const paddingLeft = await page.evaluate(() => {
      const el = document.querySelector('.cage-stats') as HTMLElement;
      return parseFloat(window.getComputedStyle(el).paddingLeft);
    });
    const expectedPadding = svgBox!.width * (14 / 600);
    expect(paddingLeft).toBeCloseTo(expectedPadding, 0);
  });
});
