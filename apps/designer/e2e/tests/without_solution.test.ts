import { test, expect, type Page } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';
import { ENTER, SHIFT_ARROW_RIGHT } from './keys';

async function setup(page: Page) {
  await installTauriStubs(page, { n: 3 }, { withoutSolution: true });
  await gotoApp(page);
  await waitForGrid(page);
  await page.locator('.grid-svg').focus();
}

const selectorLabels = (page: Page) =>
  page.locator('.grid-svg text[font-weight="700"]');

// Mode marker on .puzzle-wrap ("with"/"without"). Fix/Unfix themselves moved to
// the native Puzzle menu and the CmdOrCtrl+L shortcut; the shortcut path is
// covered in tests-web/puzzle_menu_shortcuts.test.ts (the in-page handler is
// `#[cfg(feature = "web")]`, so it does not exist in this Tauri-path build).
const mode = (page: Page) => page.locator('.puzzle-wrap[data-solution-mode]');

test.describe('new-puzzle modal authoring mode', () => {
  test('No Solution button creates a Without-Solution puzzle', async ({
    page,
  }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();

    await page
      .getByRole('button', { name: 'No Solution', exact: true })
      .click();
    await waitForGrid(page);

    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'without');
  });

  test('Random Solution button creates a With-Solution puzzle', async ({
    page,
  }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();

    await page
      .getByRole('button', { name: 'Random Solution', exact: true })
      .click();
    await waitForGrid(page);

    await expect(mode(page)).toHaveAttribute('data-solution-mode', 'with');
  });
});

test.describe('Without-Solution cage commit', () => {
  test('two-step picker commits a cage with an author-chosen target', async ({
    page,
  }) => {
    await setup(page);

    // Draw the pair {(0,0),(0,1)} and open the operation selector.
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);

    // Step one: the operator strip. An empty 3×3 pair admits an Add target.
    await expect(
      selectorLabels(page).filter({ hasText: /^\+$/ }),
    ).toBeVisible();
    await selectorLabels(page).filter({ hasText: /^\+$/ }).click();

    // Step two: the native target dropdown. {1,2} sums to 3, so 3 is a feasible
    // Add target. Options carry the bare number, so select by its value.
    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeVisible();
    // The dropdown grabs focus the moment it appears.
    await expect(targetSelect).toBeFocused();
    await targetSelect.selectOption('3');

    // The committed cage shows its +3 label and the provisional outline is gone.
    await expect(
      page.locator('.grid-svg text').filter({ hasText: /^\+3$/ }),
    ).toBeVisible();
  });

  test('target dropdown is selectable from the keyboard by typing the number', async ({
    page,
  }) => {
    await setup(page);

    // Draw {(0,0),(0,1)}, open the selector, and choose the Add operator.
    await page.keyboard.press(SHIFT_ARROW_RIGHT);
    await page.keyboard.press(ENTER);
    await selectorLabels(page).filter({ hasText: /^\+$/ }).click();

    // The focused dropdown accepts the target by typing just the number (no operator).
    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeFocused();
    await page.keyboard.press('3');

    await expect(
      page.locator('.grid-svg text').filter({ hasText: /^\+3$/ }),
    ).toBeVisible();
  });
});

test.describe('Without-Solution singleton cages', () => {
  test('typing a permitted digit immediately creates a singleton cage', async ({
    page,
  }) => {
    await setup(page);

    // The active cell starts at (0,0); 2 is a permitted value in an empty 3×3.
    await page.keyboard.press('2');

    // A committed Given cage labelled "2" appears with no selector step. Cage and
    // selector labels are weight 700, isolating it from the cell's domain digits.
    await expect(selectorLabels(page).filter({ hasText: /^2$/ })).toBeVisible();
  });

  test('singleton picker opens on the value dropdown, skipping the operator step', async ({
    page,
  }) => {
    await setup(page);

    // Enter on the empty active cell opens the singleton picker directly on the
    // native value dropdown.
    await page.keyboard.press(ENTER);

    const targetSelect = page.locator('.grid-svg select.target-select');
    await expect(targetSelect).toBeVisible();
    await expect(targetSelect).toBeFocused();

    // The operator strip is skipped (no operator tabs).
    await expect(selectorLabels(page)).toHaveCount(0);

    // The dropdown offers the feasible Given values for an empty 3×3 cell (1–3).
    await expect(targetSelect.locator('option[value="1"]')).toHaveCount(1);
    await expect(targetSelect.locator('option[value="3"]')).toHaveCount(1);

    // Choosing a value commits the singleton Given cage (its label is just the number).
    await targetSelect.selectOption('3');
    await expect(selectorLabels(page).filter({ hasText: /^3$/ })).toBeVisible();
  });
});
