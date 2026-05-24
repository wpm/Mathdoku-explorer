import { test, expect } from '@playwright/test';
import { installTauriStubs, gotoApp } from './helpers';

// A 3×3 puzzle with two cages and no regions.
// Cage 0: cells (0,0),(0,1) — Add 3
// Cage 1: cell  (0,2)       — Given 3
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

test.describe('grid rendering', () => {
  test('grid SVG is present', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await expect(page.locator('.grid-svg')).toBeVisible();
  });

  test('grid has n² background rects (one per cell)', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    // Background rects: 1 outer + n² cells = 1 + 9 = 10, plus border rect.
    // Easier: just confirm more than 9 rects exist.
    const rects = page.locator('.grid-svg rect');
    await expect(rects).toHaveCount(12); // 1 bg + 9 cells + 1 border + 1 selection overlay = 12
  });

  test('caged cells have a palette fill color', async ({ page }) => {
    const PALETTE = new Set(['#cfe4f2', '#d7ecd5', '#f7ecc6', '#f6d9d3']);
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);

    const hasCage = await page.evaluate(
      (palette) => {
        const rects = Array.from(document.querySelectorAll('.grid-svg rect'));
        return rects.some((r) =>
          palette.includes(r.getAttribute('fill') ?? ''),
        );
      },
      [...PALETTE],
    );

    expect(hasCage).toBe(true);
  });

  test('Add cage label shows "+<target>"', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    await expect(
      page.locator('.grid-svg text').filter({ hasText: '+3' }),
    ).toBeVisible();
  });

  test('Given cage label shows only the number', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    // Cage labels have font-weight="700"; candidate digits do not.
    await expect(
      page
        .locator('.grid-svg text[font-weight="700"]')
        .filter({ hasText: /^3$/ }),
    ).toBeVisible();
  });

  test('Subtract cage label shows "−<target>"', async ({ page }) => {
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [
              { row: 1, column: 0 },
              { row: 1, column: 1 },
            ],
            operation: { Subtract: 2 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);
    await expect(
      page.locator('.grid-svg text').filter({ hasText: '−2' }),
    ).toBeVisible();
  });

  test('Multiply cage label shows "×<target>"', async ({ page }) => {
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [
              { row: 1, column: 0 },
              { row: 1, column: 1 },
            ],
            operation: { Multiply: 6 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);
    await expect(
      page.locator('.grid-svg text').filter({ hasText: '×6' }),
    ).toBeVisible();
  });

  test('Divide cage label shows "÷<target>"', async ({ page }) => {
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [
              { row: 1, column: 0 },
              { row: 1, column: 1 },
            ],
            operation: { Divide: 2 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);
    await expect(
      page.locator('.grid-svg text').filter({ hasText: '÷2' }),
    ).toBeVisible();
  });

  test('region slot shows "?" label', async ({ page }) => {
    const puzzle = {
      n: 3,
      slots: [
        {
          Region: [
            { row: 2, column: 0 },
            { row: 2, column: 1 },
          ],
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);
    await expect(
      page.locator('.grid-svg text').filter({ hasText: '?' }),
    ).toBeVisible();
  });

  test('adjacent cages get different fill colors', async ({ page }) => {
    // Two single-cell cages side by side in a 3×3.
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [{ row: 0, column: 0 }],
            operation: { Given: 1 },
            n: 3,
          },
        },
        {
          Cage: {
            polyomino: [{ row: 0, column: 1 }],
            operation: { Given: 2 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);

    const PALETTE = ['#cfe4f2', '#d7ecd5', '#f7ecc6', '#f6d9d3'];
    const fills: string[] = await page.evaluate((palette) => {
      const rects = Array.from(document.querySelectorAll('.grid-svg rect'));
      return rects
        .map((r) => r.getAttribute('fill') ?? '')
        .filter((f) => palette.includes(f));
    }, PALETTE);

    // At least two distinct palette colors are used.
    const unique = new Set(fills);
    expect(unique.size).toBeGreaterThanOrEqual(2);
  });

  test('candidate digits appear for unconstrained cells', async ({ page }) => {
    // A puzzle with no cages — all cells should show all candidate digits.
    const puzzle = { n: 3, slots: [] };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);

    // Candidate texts are rendered as <text> elements; at least one digit
    // from 1 to 3 should appear multiple times.
    const texts = await page.locator('.grid-svg text').allTextContents();
    const digits = texts.filter((t) => ['1', '2', '3'].includes(t.trim()));
    expect(digits.length).toBeGreaterThan(0);
  });

  test('grid renders gridlines (line elements)', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    const lines = page.locator('.grid-svg line');
    // 3×3 grid: (n-1)*n = 6 horizontal + 6 vertical segments = 12 total.
    await expect(lines).toHaveCount(12);
  });

  test('singleton domain renders larger than multi-value domain digits', async ({
    page,
  }) => {
    // Cell (0,0) is a Given(2) cage — domain collapses to {2}.
    // All other cells are unconstrained — the domain is {1,2,3}.
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [{ row: 0, column: 0 }],
            operation: { Given: 2 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);

    const fontSizes: number[] = await page.evaluate(() => {
      return Array.from(document.querySelectorAll('.grid-svg text'))
        .map((el) => parseFloat(el.getAttribute('font-size') ?? '0'))
        .filter((s) => s > 0);
    });

    const maxSize = Math.max(...fontSizes);
    const minSize = Math.min(...fontSizes);
    expect(maxSize).toBeGreaterThan(minSize);
  });

  test('singleton domain digit is darker than multi-value domain digits', async ({
    page,
  }) => {
    const puzzle = {
      n: 3,
      slots: [
        {
          Cage: {
            polyomino: [{ row: 0, column: 0 }],
            operation: { Given: 2 },
            n: 3,
          },
        },
      ],
    };
    await installTauriStubs(page, puzzle);
    await gotoApp(page);

    const fills: string[] = await page.evaluate(() => {
      return Array.from(document.querySelectorAll('.grid-svg text')).map(
        (el) => el.getAttribute('fill') ?? '',
      );
    });

    // Singleton uses INK (#26221b), multi-value uses INK3 (#8b8476).
    expect(fills).toContain('#26221b');
    expect(fills).toContain('#8b8476');
  });

  test('outer border rect is present', async ({ page }) => {
    await installTauriStubs(page, PUZZLE_3);
    await gotoApp(page);
    // The border rect has fill="none" and INK stroke (#26221b).
    // The selection overlay also has fill="none" but uses the accent color.
    const border = page.locator(
      '.grid-svg rect[fill="none"][stroke="#26221b"]',
    );
    await expect(border).toHaveCount(1);
  });
});
