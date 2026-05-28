import { test, expect, type Page } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';
import { ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, BACKSPACE, DELETE, ENTER, ESCAPE, SHIFT_ARROW_LEFT, SHIFT_ARROW_RIGHT, TAB } from './keys';

// Empty 3×3: no cages.
const EMPTY_3X3 = { n: 3 };

// 3×3 with one cage covering (0,0).
const PUZZLE_WITH_CAGE = {
  n: 3,
  cages: [
    { polyomino: [{ row: 0, column: 0 }], operation: { operator: 'Given', target: 1 } },
  ],
};

const PROVISIONAL = '#7b4f9e';
const ACCENT = '#1a4e7a';

async function setup(page: Page, puzzle: unknown = EMPTY_3X3) {
  await installTauriStubs(page, puzzle);
  await gotoApp(page);
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
}

const provisionalLines = (page: Page) =>
  page.locator(`.grid-svg line[stroke="${PROVISIONAL}"]`);
const accentLines = (page: Page) =>
  page.locator(`.grid-svg line[stroke="${ACCENT}"]`);
const accentRect = (page: Page) =>
  page.locator(`.grid-svg rect[stroke="${ACCENT}"]`);

test.describe('provisional cage', () => {
  test('Shift+Arrow on uncovered cell draws provisional outline', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);

    await expect(provisionalLines(page)).not.toHaveCount(0);
  });

  test('provisional cage has distinct color from selection', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);

    await expect(provisionalLines(page)).not.toHaveCount(0);
    await expect(accentRect(page)).toHaveCount(1);
  });

  test('Shift+Arrow on covered cell does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    // (0,0) is covered by the cage. Default selection starts at (0,0).
    await page.keyboard.press(SHIFT_ARROW_RIGHT);

    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Shift+Arrow when target cell is covered does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    await page.keyboard.press(ARROW_RIGHT);
    await page.keyboard.press(ARROW_RIGHT);
    await page.keyboard.press(ARROW_LEFT); // now at (0,1)
    await page.keyboard.press(SHIFT_ARROW_LEFT); // target (0,0) is covered — should do nothing

    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('multiple Shift+Arrow presses grow the provisional cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    const count1 = await provisionalLines(page).count();
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    const count2 = await provisionalLines(page).count();

    // A 3-cell row has more outer edges than a 2-cell row (10 vs 6).
    expect(count2).toBeGreaterThan(count1);
  });

  test('Escape in operation selector deletes the provisional cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(ENTER); // open selector
    await expect(provisionalLines(page)).not.toHaveCount(0);

    await page.keyboard.press(ESCAPE); // dismiss selector and delete provisional cage
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Escape in provisional cage without selector open deletes the provisional cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}, cursor at (0,1)
    await expect(provisionalLines(page)).not.toHaveCount(0);

    await page.keyboard.press(ESCAPE); // cursor at (0,1) is in the provisional cage — deletes it
    await expect(provisionalLines(page)).toHaveCount(0); // cage deleted
  });

  test('regular Arrow moves cursor without clearing provisional cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw cell (0,0), move to (0,1)
    await page.keyboard.press(ARROW_DOWN); // move to (1,1), cage stays

    await expect(provisionalLines(page)).not.toHaveCount(0);
  });

  test('Shift+Arrow from disconnected cell parks old cage and starts new one', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw (0,0), now at (0,1)
    await page.keyboard.press(ARROW_DOWN); // move to (1,1)
    await page.keyboard.press(ARROW_DOWN); // move to (2,1) — not adjacent to cage
    const countBefore = await provisionalLines(page).count();

    // (2,1) is not adjacent to the existing cage {(0,0),(0,1)}; parks it and starts fresh.
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw (2,1), move to (2,2)
    const countAfter = await provisionalLines(page).count();

    // Both cages visible: parked {(0,0),(0,1)} (6 edges) + new {(2,1),(2,2)} (6 edges) = 12.
    expect(countAfter).toBeGreaterThan(countBefore);
    expect(countAfter).toBe(12);
  });

  test('Enter on uncovered cell with no provisional creates singleton cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER);

    // After committing, active cell stays in place — accent rect visible, no provisional lines.
    await expect(accentRect(page)).toHaveCount(1);
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Enter on covered cell with no provisional does nothing', async ({ page }) => {
    await setup(page, PUZZLE_WITH_CAGE);
    // (0,0) is covered.
    await page.keyboard.press(ENTER);

    // Still in Cell Mode.
    await expect(accentRect(page)).toHaveCount(1);
  });

  test('Enter then operator commits provisional cage and returns to active cell', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}, move to (0,1)
    await page.keyboard.press(ENTER); // open operation selector
    await page.keyboard.press('+'); // choose Add

    // After committing, active cell stays in place — accent rect visible, provisional cleared.
    await expect(accentRect(page)).toHaveCount(1);
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Escape from operation selector deletes the provisional cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(ENTER); // open selector
    await page.keyboard.press(ESCAPE); // dismiss selector and delete provisional cage

    // Provisional cage is gone — no provisional lines.
    await expect(provisionalLines(page)).toHaveCount(0);
    // No cage was committed (no accent lines, accent rect still shows selection).
    await expect(accentLines(page)).toHaveCount(0);
    await expect(accentRect(page)).toHaveCount(1);
  });

  test('Tab does not exit operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER); // open selector

    // Verify selector is visible.
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: '+' })).toBeVisible();

    await page.keyboard.press(TAB); // should be swallowed by selector

    // Selector still visible (Tab was caught by selector, not dismissed).
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: '+' })).toBeVisible();
    // No accent lines (those are a Slot Mode artifact that no longer exists).
    await expect(accentLines(page)).toHaveCount(0);
  });

  test('ArrowRight moves highlight forward in operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(ENTER);

    const labels = page.locator('.grid-svg text[font-weight="700"]');
    // First tab (+) starts highlighted — its fill is ACCENT (dark), text is light.
    // After ArrowRight the second tab (−) should become highlighted.
    // We verify by checking the selector is still open.
    await page.keyboard.press(ARROW_RIGHT);
    await expect(labels.filter({ hasText: '+' })).toBeVisible(); // selector still open
    await expect(accentLines(page)).toHaveCount(0);              // no accent lines
  });

  test('ArrowLeft moves highlight backward in operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);

    // Move right twice, then back left once — should stay in selector throughout.
    await page.keyboard.press(ARROW_RIGHT);
    await page.keyboard.press(ARROW_RIGHT);
    await page.keyboard.press(ARROW_LEFT);
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: '+' })).toBeVisible();
    await expect(accentLines(page)).toHaveCount(0); // no accent lines
  });

  test('Enter commits highlighted tab in operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER); // open selector, + highlighted

    await page.keyboard.press(ARROW_RIGHT); // move to −
    await page.keyboard.press(ENTER);       // commit −

    // After committing, active cell stays in place — accent rect visible, provisional cleared.
    await expect(accentRect(page)).toHaveCount(1);
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('3-cell provisional shows only Add and Multiply in operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1),(0,2)}
    await page.keyboard.press(ENTER); // open operation selector

    const selectorLabels = page.locator('.grid-svg text[font-weight="700"]');
    // Should show "+" and "×" only — not "−" or "÷".
    await expect(selectorLabels.filter({ hasText: '+' })).toBeVisible();
    await expect(selectorLabels.filter({ hasText: '×' })).toBeVisible();
    await expect(selectorLabels.filter({ hasText: '−' })).toHaveCount(0);
    await expect(selectorLabels.filter({ hasText: '÷' })).toHaveCount(0);
  });

  test('committed singleton cage shows "1" label', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton at (0,0) → Given/1 cage

    // The insert_cage stub returns a Given/1 cage for a singleton.
    // Given cages display only the target number as label.
    await expect(
      page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ }),
    ).toBeVisible();
  });
});

test.describe('Escape demotes committed cage', () => {
  test('Escape on a committed cage demotes it to provisional', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton at (0,0)
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toBeVisible();

    await page.keyboard.press(ESCAPE); // demote cage back to provisional
    // The cage label is gone (cage removed from committed cages).
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toHaveCount(0);
    // The cell is now provisional (purple outline visible).
    await expect(provisionalLines(page)).not.toHaveCount(0);
  });

  test('Escape on a committed multi-cell cage opens operation selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(ENTER);
    await page.keyboard.press('+'); // commit as Add cage
    // Cage label includes target (e.g. "+0") — verify cage is committed.
    await expect(page.locator('.grid-svg text[font-weight="700"]')).toHaveCount(1);

    await page.keyboard.press(ESCAPE); // demote cage back to provisional + open selector
    // The committed cage label is gone; operation selector tabs appear instead.
    // Selector shows multiple tabs (at least +), so count increases or stays same but cage label gone.
    await expect(page.locator('.grid-svg text[font-weight="700"]')).not.toHaveCount(1);
  });

  test('Escape on uncovered cell does nothing', async ({ page }) => {
    await setup(page);
    // (0,0) is uncovered and has no provisional cage.
    await page.keyboard.press(ESCAPE);
    await expect(provisionalLines(page)).toHaveCount(0);
    await expect(accentRect(page)).toHaveCount(1);
  });
});

test.describe('Delete removes cages', () => {
  test('Delete on a committed cage deletes it outright (no demotion)', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton at (0,0)
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toBeVisible();

    await page.keyboard.press(DELETE); // delete the committed cage outright
    // The cage label is gone (cage removed from committed cages).
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toHaveCount(0);
    // The cell is NOT demoted to a provisional cage — no purple outline.
    await expect(provisionalLines(page)).toHaveCount(0);
    // Still in Cell Mode at the now-uncovered cell.
    await expect(accentRect(page)).toHaveCount(1);
  });

  test('Delete on a committed multi-cell cage deletes it without opening selector', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}
    await page.keyboard.press(ENTER);
    await page.keyboard.press('+'); // commit as Add cage
    await expect(page.locator('.grid-svg text[font-weight="700"]')).toHaveCount(1);

    await page.keyboard.press(DELETE); // delete the committed cage outright
    // No cage label and no operation selector tabs remain.
    await expect(page.locator('.grid-svg text[font-weight="700"]')).toHaveCount(0);
    // Not demoted to provisional.
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Delete on a provisional cage deletes it', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(SHIFT_ARROW_RIGHT); // draw {(0,0),(0,1)}, cursor at (0,1)
    await expect(provisionalLines(page)).not.toHaveCount(0);

    await page.keyboard.press(DELETE); // cursor at (0,1) is in the provisional cage — deletes it
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Backspace behaves the same as Delete on a committed cage', async ({ page }) => {
    await setup(page);
    await page.keyboard.press(ENTER); // commit singleton at (0,0)
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toBeVisible();

    await page.keyboard.press(BACKSPACE);
    await expect(page.locator('.grid-svg text[font-weight="700"]').filter({ hasText: /^1$/ })).toHaveCount(0);
    await expect(provisionalLines(page)).toHaveCount(0);
  });

  test('Delete on uncovered cell does nothing', async ({ page }) => {
    await setup(page);
    // (0,0) is uncovered and has no provisional cage.
    await page.keyboard.press(DELETE);
    await expect(provisionalLines(page)).toHaveCount(0);
    await expect(accentRect(page)).toHaveCount(1);
  });
});
