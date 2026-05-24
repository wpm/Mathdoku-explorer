import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';

// 3×3 puzzle: two cages.
// Slot 0: cells (0,0),(0,1) — horizontal domino, Add 3
// Slot 1: cell  (0,2)       — singleton, Given 3
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

const ACCENT = '#1a4e7a';

// Enter Slot Mode from the default Cell Mode start state.
async function enterSlotMode(page: import('@playwright/test').Page) {
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
  await page.keyboard.press('Tab');
}

test.describe('slot selection overlay', () => {
  test('Tab enters Slot Mode: accent lines appear', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);

    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).not.toHaveCount(0);
  });

  test('Shift+Tab in Cell Mode enters Slot Mode', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    await page.keyboard.press('Shift+Tab');

    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).not.toHaveCount(0);
    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(0);
  });

  test('Cell Mode shows accent rect, not accent lines', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);

    // Still in Cell Mode — overlay is a rect, not lines.
    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(1);
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(0);
  });

  test('Escape in Slot Mode does nothing', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);
    await page.keyboard.press('Escape');

    // Still in Slot Mode: lines present, no accent rect.
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).not.toHaveCount(0);
    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(0);
  });

  test('ArrowDown in Slot Mode returns to Cell Mode', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);
    await page.keyboard.press('ArrowDown');

    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(1);
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(0);
  });

  test('two-cell horizontal slot: 6 outer edges, no inner shared edge', async ({
    page,
  }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    // Start at (0,0); Tab enters Slot Mode selecting slot 0 (the domino).
    await enterSlotMode(page);

    // A 2-cell horizontal domino has 6 outer edges (3 per cell minus 1 shared).
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(6);
  });

  test('Tab advances to next slot', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page);
    // Advance to slot 1 (singleton).
    await page.keyboard.press('Tab');

    // Singleton has 4 outer edges.
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(4);
  });

  test('Tab wraps from last slot back to first', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // slot 0
    await page.keyboard.press('Tab'); // slot 1
    await page.keyboard.press('Tab'); // wraps to slot 0 (domino)

    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(6);
  });

  test('Shift+Tab moves to previous slot', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // slot 0
    await page.keyboard.press('Tab'); // slot 1

    await page.keyboard.press('Shift+Tab'); // back to slot 0 (domino)
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(6);
  });

  test('Shift+Tab wraps from first slot to last', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await enterSlotMode(page); // slot 0

    await page.keyboard.press('Shift+Tab'); // wraps to slot 1 (singleton)
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(4);
  });

  test('Tab does nothing in Cell Mode when there are no slots', async ({
    page,
  }) => {
    await installTauriStubs(page, { n: 3, slots: [] });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    await page.keyboard.press('Tab');

    // Still in Cell Mode: accent rect present, no accent lines.
    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(1);
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(0);
  });

  test('Shift+Tab does nothing in Cell Mode when there are no slots', async ({
    page,
  }) => {
    await installTauriStubs(page, { n: 3, slots: [] });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();
    await page.keyboard.press('Shift+Tab');

    const accentRect = page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);
    await expect(accentRect).toHaveCount(1);
    const accentLines = page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
    await expect(accentLines).toHaveCount(0);
  });
});
