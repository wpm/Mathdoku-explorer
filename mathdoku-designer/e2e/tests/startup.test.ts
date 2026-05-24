import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp } from './helpers';

const PUZZLE = { n: 4, slots: [] };

test.describe('startup', () => {
  test('get_puzzle returning a puzzle renders the grid', async ({ page }) => {
    await installTauriStubs(page, PUZZLE);
    await gotoApp(page);
    await expect(page.locator('.grid-svg')).toBeVisible();
  });

  test('get_puzzle returning null leaves no grid', async ({ page }) => {
    await installTauriStubs(page, null);
    await gotoApp(page);
    await expect(page.locator('.grid-svg')).toHaveCount(0);
  });
});
