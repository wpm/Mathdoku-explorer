import { test, expect, type Page } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';
import { ENTER, SHIFT_ARROW_RIGHT } from './keys';

const fixButton = (page: Page) => page.getByRole('button', { name: 'Fix Solution', exact: true });
const unfixButton = (page: Page) => page.getByRole('button', { name: 'Unfix Solution', exact: true });
const selectorLabels = (page: Page) => page.locator('.grid-svg text[font-weight="700"]');

test.describe('new-puzzle modal authoring mode', () => {
  test('Empty button creates a Without-Solution puzzle', async ({ page }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(page.locator('p').filter({ hasText: 'New puzzle' })).toBeVisible();

    await page.getByRole('button', { name: 'Empty', exact: true }).click();
    await waitForGrid(page);

    // Without-Solution mode offers the Fix control, not Unfix.
    await expect(fixButton(page)).toBeVisible();
    await expect(unfixButton(page)).toHaveCount(0);
  });

  test('With Solution button creates a With-Solution puzzle', async ({ page }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(page.locator('p').filter({ hasText: 'New puzzle' })).toBeVisible();

    await page.getByRole('button', { name: 'With Solution', exact: true }).click();
    await waitForGrid(page);

    await expect(unfixButton(page)).toBeVisible();
    await expect(fixButton(page)).toHaveCount(0);
  });
});

test.describe('fix / unfix mode switching', () => {
  test('Unfix drops the solution and Fix snapshots it back', async ({ page }) => {
    // A fully-given 3×3 puzzle: unique solution, so Fix Solution is enabled after unfix.
    const given3x3 = {
      n: 3,
      cages: [
        { polyomino: [{ row: 0, column: 0 }], operation: { operator: 'Given', target: 1 } },
        { polyomino: [{ row: 0, column: 1 }], operation: { operator: 'Given', target: 2 } },
        { polyomino: [{ row: 0, column: 2 }], operation: { operator: 'Given', target: 3 } },
        { polyomino: [{ row: 1, column: 0 }], operation: { operator: 'Given', target: 2 } },
        { polyomino: [{ row: 1, column: 1 }], operation: { operator: 'Given', target: 3 } },
        { polyomino: [{ row: 1, column: 2 }], operation: { operator: 'Given', target: 1 } },
        { polyomino: [{ row: 2, column: 0 }], operation: { operator: 'Given', target: 3 } },
        { polyomino: [{ row: 2, column: 1 }], operation: { operator: 'Given', target: 1 } },
        { polyomino: [{ row: 2, column: 2 }], operation: { operator: 'Given', target: 2 } },
      ],
    };
    await installTauriStubs(page, given3x3);
    await gotoApp(page);
    await waitForGrid(page);

    await expect(unfixButton(page)).toBeVisible();
    await unfixButton(page).click();
    // After unfix the cages remain; unique solution means Fix Solution becomes enabled.
    await expect(fixButton(page)).toBeVisible();
    await expect(fixButton(page)).toBeEnabled();

    await fixButton(page).click();
    await expect(unfixButton(page)).toBeVisible();
  });
});

test.describe('Without-Solution cage commit', () => {
  test('two-step picker commits a cage with an author-chosen target', async ({ page }) => {
    await installTauriStubs(page, { n: 3 }, { withoutSolution: true });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

    // Draw the pair {(0,0),(0,1)} and open the operation selector.
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);

    // Step one: the operator strip. An empty 3×3 pair admits an Add target.
    await expect(selectorLabels(page).filter({ hasText: /^\+$/ })).toBeVisible();
    await selectorLabels(page).filter({ hasText: /^\+$/ }).click();

    // Step two: the native target dropdown. {1,2} sums to 3, so 3 is a feasible
    // Add target. Options carry the bare number, so select by its value.
    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeVisible();
    // The dropdown grabs focus the moment it appears.
    await expect(targetSelect).toBeFocused();
    await targetSelect.selectOption('3');

    // The committed cage shows its +3 label and the provisional outline is gone.
    await expect(page.locator('.grid-svg text').filter({ hasText: /^\+3$/ })).toBeVisible();
  });

  test('target dropdown is selectable from the keyboard by typing the number', async ({ page }) => {
    await installTauriStubs(page, { n: 3 }, { withoutSolution: true });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

    // Draw {(0,0),(0,1)}, open the selector, and choose the Add operator.
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);
    await selectorLabels(page).filter({ hasText: /^\+$/ }).click();

    // The focused dropdown accepts the target by typing just the number (no operator).
    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeFocused();
    await page.keyboard.press('3');

    await expect(page.locator('.grid-svg text').filter({ hasText: /^\+3$/ })).toBeVisible();
  });
});

test.describe('Without-Solution singleton cages', () => {
  test('typing a permitted digit immediately creates a singleton cage', async ({ page }) => {
    await installTauriStubs(page, { n: 3 }, { withoutSolution: true });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

    // The active cell starts at (0,0); 2 is a permitted value in an empty 3×3.
    await page.keyboard.press('2');

    // A committed Given cage labelled "2" appears with no selector step. Cage and
    // selector labels are weight 700, isolating it from the cell's domain digits.
    await expect(selectorLabels(page).filter({ hasText: /^2$/ })).toBeVisible();
  });

  test('singleton picker opens on the value dropdown, skipping the operator step', async ({ page }) => {
    await installTauriStubs(page, { n: 3 }, { withoutSolution: true });
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

    // Enter on the empty active cell opens the singleton picker directly on the
    // native value dropdown.
    await page.keyboard.press(ENTER);

    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeVisible();
    await expect(targetSelect).toBeFocused();

    // The operator strip is skipped (no operator tabs) and there is no '#' header.
    await expect(selectorLabels(page)).toHaveCount(0);
    await expect(targetSelect.locator('option').filter({ hasText: '#' })).toHaveCount(0);

    // The dropdown offers the feasible Given values for an empty 3×3 cell (1–3).
    await expect(targetSelect.locator('option[value="1"]')).toHaveCount(1);
    await expect(targetSelect.locator('option[value="3"]')).toHaveCount(1);

    // Choosing a value commits the singleton Given cage (its label is just the number).
    await targetSelect.selectOption('3');
    await expect(page.locator('.grid-svg text').filter({ hasText: /^3$/ })).toBeVisible();
  });
});
