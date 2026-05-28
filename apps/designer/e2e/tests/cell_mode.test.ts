import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp, waitForGrid } from './helpers';
import { ARROW_DOWN, ARROW_RIGHT } from './keys';

const EMPTY_3 = { n: 3 };
const ACCENT = '#1a4e7a';

// Returns the {x, y} attribute values of the Cell Mode selection rect.
async function selectionRectXY(page: import('@playwright/test').Page) {
  return page.locator(`.grid-svg rect[stroke="${ACCENT}"]`).evaluate((el) => ({
    x: parseFloat(el.getAttribute('x') ?? '0'),
    y: parseFloat(el.getAttribute('y') ?? '0'),
  }));
}

test.describe('cell mode navigation', () => {
  test('grid auto-focuses on mount so arrow keys work immediately', async ({
    page,
  }) => {
    await installTauriStubs(page, EMPTY_3);
    await gotoApp(page);
    await waitForGrid(page);

    // No explicit focus — the grid should auto-focus on mount.
    const before = await selectionRectXY(page);
    await page.keyboard.press(ARROW_DOWN);
    const after = await selectionRectXY(page);

    expect(after.y).toBeGreaterThan(before.y);
    expect(after.x).toBe(before.x);
  });

  test('arrow key moves selection after explicit focus', async ({ page }) => {
    await installTauriStubs(page, EMPTY_3);
    await gotoApp(page);
    await waitForGrid(page);
    await page.locator('.grid-svg').focus();

    const before = await selectionRectXY(page);
    await page.keyboard.press(ARROW_RIGHT);
    const after = await selectionRectXY(page);

    expect(after.x).toBeGreaterThan(before.x);
    expect(after.y).toBe(before.y);
  });
});
