import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid, PUZZLE_3 } from './helpers';
import { ARROW_DOWN, TAB } from './keys';

test.describe('cage stats', () => {
  test('no stats shown when active cell is not in any cage', async ({
    page,
  }) => {
    // Start at (0,0) in Cell Mode — that cell IS in a cage. Navigate away.
    await installTauriStubs(page, {
      n: 3,
      cages: [
        {
          polyomino: [{ row: 0, column: 0 }],
          operation: { operator: 'Given', target: 1 },
        },
      ],
    });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    // Move to (1,0) — not in any cage.
    await page.keyboard.press(ARROW_DOWN);

    await expect(page.locator('.cage-stats')).toBeHidden();
  });

  test('stats shown when active cell is in a cage', async ({
    page,
  }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);

    // Default start at (0,0), which is in the Add(3) domino.
    // Add(3) over a 3×3 row pair: 2 tuples, 1 multiset.
    await expect(page.locator('.cage-stats')).toContainText('1 Multiset');
    await expect(page.locator('.cage-stats')).toContainText('2 Tuples');
  });

  test('Tab to next cage anchor updates stats', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    // Start at (0,0) in cage 0 (Add domino). Tab moves to cage 1 anchor (0,2).
    await page.keyboard.press(TAB);

    // Singleton Given(3): exactly 1 viable tuple and 1 multiset.
    await expect(page.locator('.cage-stats')).toContainText('1 Multiset');
    await expect(page.locator('.cage-stats')).toContainText('1 Tuple');
  });

  test('"Tuple" is singular when count is 1', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    await page.keyboard.press(TAB); // Given(3) singleton → 1 Tuple

    await expect(page.locator('.cage-stats')).toContainText('1 Tuple');
    await expect(page.locator('.cage-stats')).not.toContainText('1 Tuples');
  });

  test('"Tuple" is plural when count is > 1', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);

    // Default start at (0,0) in Add(3) domino → 2 Tuples.
    await expect(page.locator('.cage-stats')).toContainText('2 Tuples');
    await expect(page.locator('.cage-stats')).not.toContainText('2 Tuple,');
  });

  test('cage stats text is aligned with inner grid border', async ({
    page,
  }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

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
