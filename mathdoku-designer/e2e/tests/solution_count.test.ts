import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';

// Minimal complete 2×2 puzzle with a unique solution.
// c1: (0,0) Given(1), c2: (0,1) Given(2), c3: (1,0)(1,1) Add(3)
const COMPLETE_2X2 = {
  n: 2,
  slots: [
    { Cage: { polyomino: [{ row: 0, column: 0 }], operation: { Given: 1 }, n: 2 } },
    { Cage: { polyomino: [{ row: 0, column: 1 }], operation: { Given: 2 }, n: 2 } },
    { Cage: { polyomino: [{ row: 1, column: 0 }, { row: 1, column: 1 }], operation: { Add: 3 }, n: 2 } },
  ],
};

// Incomplete 3×3: only two cells covered; most cells are in region slots.
const INCOMPLETE_3X3 = {
  n: 3,
  slots: [
    { Cage: { polyomino: [{ row: 0, column: 0 }], operation: { Given: 1 }, n: 3 } },
  ],
};

// Brand new empty 9×9: no slots at all.
const EMPTY_9X9 = { n: 9, slots: [] };

test.describe('solution count', () => {
  test('solution count shown for a complete puzzle', async ({ page }) => {
    await installTauriStubs(page, COMPLETE_2X2);
    await gotoApp(page);
    await waitForGrid(page);

    // Wait for the async solver to finish and set the count.
    await expect(page.locator('.solution-count')).toContainText('solution', { timeout: 5000 });
  });

  test('solution count is right-aligned with the puzzle', async ({ page }) => {
    await installTauriStubs(page, COMPLETE_2X2);
    await gotoApp(page);
    await waitForGrid(page);
    await expect(page.locator('.solution-count')).toContainText('solution', { timeout: 5000 });

    // The element is pushed right via margin-left: auto in a flex row.
    // Verify its right edge aligns with the SVG's right edge.
    const svgBox = await page.locator('.grid-svg').boundingBox();
    const countBox = await page.locator('.solution-count').boundingBox();
    expect(countBox!.x + countBox!.width).toBeCloseTo(svgBox!.x + svgBox!.width, 0);
  });

  test('solution count text is aligned with inner grid border', async ({ page }) => {
    await installTauriStubs(page, COMPLETE_2X2);
    await gotoApp(page);
    await waitForGrid(page);
    await expect(page.locator('.solution-count')).toContainText('solution', { timeout: 5000 });

    const svgBox = await page.locator('.grid-svg').boundingBox();
    // padding-right mirrors padding-left on cage-stats: 14/600 of SVG width.
    const paddingRight = await page.evaluate(() => {
      const el = document.querySelector('.solution-count') as HTMLElement;
      return parseFloat(window.getComputedStyle(el).paddingRight);
    });
    const expectedPadding = svgBox!.width * (14 / 600);
    expect(paddingRight).toBeCloseTo(expectedPadding, 0);
  });

  test('solution count not shown for an incomplete puzzle', async ({ page }) => {
    await installTauriStubs(page, INCOMPLETE_3X3);
    await gotoApp(page);
    await waitForGrid(page);

    await expect(page.locator('.solution-count')).toBeHidden();
  });

  test('solution count not shown and no hang for brand-new empty 9×9', async ({ page }) => {
    await installTauriStubs(page, EMPTY_9X9);
    await gotoApp(page);
    await waitForGrid(page);

    // Grid should render quickly; no solution-count div should appear.
    await expect(page.locator('.solution-count')).toBeHidden();
  });

  test('"solution" is singular for exactly 1 solution', async ({ page }) => {
    await installTauriStubs(page, COMPLETE_2X2);
    await gotoApp(page);
    await waitForGrid(page);

    await expect(page.locator('.solution-count')).toContainText('1 solution', { timeout: 5000 });
    await expect(page.locator('.solution-count')).not.toContainText('1 solutions');
  });
});
