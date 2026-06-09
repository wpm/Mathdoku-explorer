import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp } from './helpers';
import { ESCAPE } from './keys';

const PUZZLE = { n: 4 };

test.describe('startup', () => {
  test('get_puzzle returning a puzzle renders the grid', async ({ page }) => {
    await installTauriStubs(page, PUZZLE);
    await gotoApp(page);
    await expect(page.locator('.grid-svg')).toBeVisible();
  });

  test('get_puzzle returning null shows Size Modal instead of grid', async ({
    page,
  }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(page.locator('.grid-svg')).toHaveCount(0);
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();
  });

  test('startup Size Modal has no Cancel button and Escape does not dismiss it', async ({
    page,
  }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();

    await expect(page.locator('button', { hasText: 'Cancel' })).toHaveCount(0);

    await page.keyboard.press(ESCAPE);
    await expect(
      page.locator('p').filter({ hasText: 'New puzzle' }),
    ).toBeVisible();
  });
});
