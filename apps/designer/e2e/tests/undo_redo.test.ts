import { test, expect, type Page } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';
import { ENTER, SHIFT_ARROW_RIGHT } from './keys';

const EMPTY_3X3 = { n: 3 };

async function setup(page: Page) {
  await installTauriStubs(page, EMPTY_3X3);
  await gotoApp(page);
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
}

// Returns all cage label text elements (font-weight 700 inside .grid-svg).
const cageLabels = (page: Page) =>
  page.locator('.grid-svg text[font-weight="700"]');

test.describe('undo / redo', () => {
  test('Cmd+Z undoes a committed singleton cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton at (0,0) — auto-Given, no selector

    await expect(cageLabels(page)).not.toHaveCount(0);

    await page.keyboard.press('Meta+z');

    await expect(cageLabels(page)).toHaveCount(0);
  });

  test('Cmd+Shift+Z redoes after undo', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton

    await page.keyboard.press('Meta+z');
    await expect(cageLabels(page)).toHaveCount(0);

    await page.keyboard.press('Meta+Shift+z');
    await expect(cageLabels(page)).not.toHaveCount(0);
  });

  test('Cmd+Z with empty undo stack does nothing', async ({ page }) => {
    await setup(page);
    // No cages committed — undo stack is empty.
    await page.keyboard.press('Meta+z');

    // Grid is still visible and functional.
    await expect(page.locator('.grid-svg')).toBeVisible();
    await expect(cageLabels(page)).toHaveCount(0);
  });

  test('committing after undo clears redo stack', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit cage 1 at (0,0)

    await page.keyboard.press('Meta+z'); // undo cage 1 → redo stack now has cage 1
    await expect(cageLabels(page)).toHaveCount(0);

    // Commit a new cage at (0,1) — this should clear the redo stack.
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press(ENTER); // commit cage 2 at (0,1)
    const countAfterCommit = await cageLabels(page).count();

    // Ctrl+Shift+Z should be a no-op now (redo stack cleared by the new commit).
    await page.keyboard.press('Meta+Shift+z');
    await expect(cageLabels(page)).toHaveCount(countAfterCommit);
  });

  test('multiple undos remove cages one at a time', async ({ page }) => {
    await setup(page);
    // Commit cage at (0,0).
    await page.keyboard.press(ENTER);
    // Move to (0,1) and commit a 2-cell cage.
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,1),(0,2)}
    await page.keyboard.press(ENTER);
    await page.keyboard.press('+'); // choose Add

    const countAfterTwo = await cageLabels(page).count();
    expect(countAfterTwo).toBe(2);

    await page.keyboard.press('Meta+z'); // undo second cage
    await expect(cageLabels(page)).toHaveCount(1);

    await page.keyboard.press('Meta+z'); // undo first cage
    await expect(cageLabels(page)).toHaveCount(0);
  });

  test('undo then redo restores both cages', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit cage 1

    await page.keyboard.press('Meta+z');
    await expect(cageLabels(page)).toHaveCount(0);

    await page.keyboard.press('Meta+Shift+z');
    await expect(cageLabels(page)).toHaveCount(1);
  });
});
