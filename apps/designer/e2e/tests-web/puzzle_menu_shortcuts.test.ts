import { test, expect, type Page } from '@playwright/test';
import { gotoApp, waitForGrid } from '../tests/helpers';
import { ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT } from '../tests/keys';

// Fix / Unfix keyboard shortcuts on the WASM-everything preview build (issue
// #73). The footer Fix/Unfix button is gone; Fix/Unfix now live in a native
// Puzzle menu (desktop) and behind CmdOrCtrl+L / CmdOrCtrl+Shift+L on both
// targets. The in-page keydown handler is `#[cfg(feature = "web")]`, so these
// shortcuts only exist in this build — which is why the coverage lives here and
// not under ../tests (the Tauri-path build, where the menu accelerator drives
// Fix/Unfix and there is no in-page handler to exercise from the browser).
//
// Mode is read from `[data-solution-mode]` on .puzzle-wrap ("with"/"without"),
// the canonical marker now that the button no longer reports it.

const FIX = 'Meta+l';
const UNFIX = 'Meta+Shift+l';

const mode = (page: Page) => page.locator('.puzzle-wrap[data-solution-mode]');

// Create a fresh n×n puzzle from the first-launch New-puzzle modal. The modal
// defaults to 9×9, so the size is selected explicitly first.
async function newPuzzle(
  page: Page,
  n: number,
  variant: 'No Solution' | 'Random Solution',
) {
  await gotoApp(page);
  await expect(
    page.locator('p').filter({ hasText: 'New puzzle' }),
  ).toBeVisible();
  await page.locator('select').selectOption(String(n));
  await page.getByRole('button', { name: variant, exact: true }).click();
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
}

// Fill every cell of an empty 3×3 with singleton Given cages forming the Latin
// square
//   1 2 3
//   2 3 1
//   3 1 2
// Typing a feasible digit on an uncovered cell commits a Given there (no
// selector). Full coverage with a unique completion is what enables Fix.
async function fillUnique3x3(page: Page) {
  // Row 0, left to right from the starting cell (0,0).
  await page.keyboard.press('1');
  await page.keyboard.press(ARROW_RIGHT);
  await page.keyboard.press('2');
  await page.keyboard.press(ARROW_RIGHT);
  await page.keyboard.press('3');
  // Drop to (1,2) and fill row 1 right to left.
  await page.keyboard.press(ARROW_DOWN);
  await page.keyboard.press('1');
  await page.keyboard.press(ARROW_LEFT);
  await page.keyboard.press('3');
  await page.keyboard.press(ARROW_LEFT);
  await page.keyboard.press('2');
  // Drop to (2,0) and fill row 2 left to right.
  await page.keyboard.press(ARROW_DOWN);
  await page.keyboard.press('3');
  await page.keyboard.press(ARROW_RIGHT);
  await page.keyboard.press('1');
  await page.keyboard.press(ARROW_RIGHT);
  await page.keyboard.press('2');
}

test.describe('Fix shortcut (CmdOrCtrl+L)', () => {
  test('fixes the solution when a unique completion exists', async ({
    page,
  }) => {
    await newPuzzle(page, 3, 'No Solution');
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');

    await fillUnique3x3(page);
    // The fully-covered, uniquely-solvable puzzle reports its solution count.
    // Waiting on it also guarantees the Fix-enable predicate has resolved.
    await expect(page.locator('.solution-count')).toContainText('1 solution', {
      timeout: 5000,
    });

    await page.keyboard.press(FIX);
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'with');
  });

  test('is a no-op when no unique solution exists', async ({ page }) => {
    await newPuzzle(page, 3, 'No Solution');
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');

    // An empty puzzle has many completions, so Fix is not offered.
    await page.keyboard.press(FIX);
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');
  });
});

test.describe('Unfix shortcut (CmdOrCtrl+Shift+L)', () => {
  test('unfixes a With-Solution puzzle', async ({ page }) => {
    await newPuzzle(page, 3, 'Random Solution');
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'with');

    await page.keyboard.press(UNFIX);
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');
  });

  test('is a no-op in Without-Solution mode', async ({ page }) => {
    await newPuzzle(page, 3, 'No Solution');
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');

    await page.keyboard.press(UNFIX);
    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');
  });
});
