import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';

// Empty 3×3: no slots.
const EMPTY_3X3 = { n: 3, slots: [] };

// 3×3 with one cage covering (0,0).
const PUZZLE_WITH_CAGE = {
  n: 3,
  slots: [
    { Cage: { polyomino: [{ row: 0, column: 0 }], operation: { Given: 1 }, n: 3 } },
  ],
};

const PROVISIONAL = '#7b4f9e';
const ACCENT = '#1a4e7a';

async function setup(page: import('@playwright/test').Page, puzzle: { n: number; slots: unknown[] } = EMPTY_3X3) {
  await installTauriStubs(page, puzzle);
  await gotoApp(page);
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
}

test.describe('provisional region', () => {
  test('Shift+Arrow on uncovered cell draws provisional outline', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight');

    const lines = page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`);
    await expect(lines).not.toHaveCount(0);
  });

  test('provisional region has distinct color from selection', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight');

    // Provisional lines exist in purple.
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).not.toHaveCount(0);
    // Accent selection rect also exists (Cell Mode).
    await expect(page.locator(`.grid-svg rect[stroke="${ACCENT}"]`)).toHaveCount(1);
  });

  test('Shift+Arrow on covered cell does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    // (0,0) is covered by the cage. Default selection starts at (0,0).
    await page.keyboard.press('Shift+ArrowRight');

    // No provisional lines should appear.
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).toHaveCount(0);
  });

  test('Shift+Arrow when target cell is covered does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    // Move to (0,1) which is uncovered; target right (0,2) is also uncovered.
    await page.keyboard.press('ArrowRight');
    // Move to (0,2).
    await page.keyboard.press('ArrowRight');
    // Target left (0,1) is uncovered, but (0,0) is covered. Try going left from (0,1):
    await page.keyboard.press('ArrowLeft'); // now at (0,1)
    await page.keyboard.press('Shift+ArrowLeft'); // target (0,0) is covered — should do nothing

    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).toHaveCount(0);
  });

  test('multiple Shift+Arrow presses grow the provisional region', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight');
    const count1 = await page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`).count();
    await page.keyboard.press('Shift+ArrowRight');
    const count2 = await page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`).count();

    // A 3-cell row has more outer edges than a 2-cell row (10 vs 6).
    expect(count2).toBeGreaterThan(count1);
  });

  test('Escape clears the provisional region', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight');
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).not.toHaveCount(0);

    await page.keyboard.press('Escape');
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).toHaveCount(0);
  });

  test('regular Arrow moves cursor without clearing provisional region', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight'); // draw cell (0,0), move to (0,1)
    await page.keyboard.press('ArrowDown'); // move to (1,1), region stays

    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).not.toHaveCount(0);
  });

  test('Shift+Arrow from disconnected cell restarts provisional region', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight'); // draw (0,0), now at (0,1)
    await page.keyboard.press('ArrowDown'); // move to (1,1)
    await page.keyboard.press('ArrowDown'); // move to (2,1) — not adjacent to region
    const countBefore = await page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`).count();

    // (2,1) is not adjacent to the existing region {(0,0)}; should restart.
    await page.keyboard.press('Shift+ArrowRight'); // draw (2,1), move to (2,2)
    const countAfter = await page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`).count();

    // After restart, region is {(2,1),(2,2)}: 6 outer edges (2-cell domino).
    expect(countAfter).toBe(6);
  });

  test('Enter on uncovered cell with no provisional creates singleton region', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Enter');

    // Should enter Slot Mode (accent lines appear, no accent rect).
    await expect(page.locator(`.grid-svg line[stroke="${ACCENT}"]`)).not.toHaveCount(0);
    await expect(page.locator(`.grid-svg rect[stroke="${ACCENT}"]`)).toHaveCount(0);
    // No provisional lines.
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).toHaveCount(0);
  });

  test('Enter on covered cell with no provisional does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    // (0,0) is covered.
    await page.keyboard.press('Enter');

    // Still in Cell Mode.
    await expect(page.locator(`.grid-svg rect[stroke="${ACCENT}"]`)).toHaveCount(1);
  });

  test('Enter commits provisional region and enters Slot Mode', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Shift+ArrowRight'); // draw (0,0), move to (0,1)
    await page.keyboard.press('Enter'); // commit {(0,0)}

    // Slot Mode: accent lines, no accent rect.
    await expect(page.locator(`.grid-svg line[stroke="${ACCENT}"]`)).not.toHaveCount(0);
    await expect(page.locator(`.grid-svg rect[stroke="${ACCENT}"]`)).toHaveCount(0);
    // Provisional cleared.
    await expect(page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`)).toHaveCount(0);
  });

  test('committed region shows "?" label', async ({ page }) => {
    await setup(page);
    await page.keyboard.press('Enter'); // commit singleton at (0,0)

    await expect(page.locator('.grid-svg text').filter({ hasText: '?' })).toBeVisible();
  });
});
