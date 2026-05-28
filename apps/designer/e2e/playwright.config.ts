import { defineConfig } from '@playwright/test';
import path from 'path';

// Optional escape hatch: if you have a pre-installed chromium binary (offline
// runs, alternate revision, etc.) point this env var at it and Playwright will
// launch that instead of its bundled download. Unset means default behavior.
const chromiumExecutable = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;

export default defineConfig({
  testDir: './tests',
  timeout: 30000,
  use: {
    baseURL: 'http://localhost:1420',
    browserName: 'chromium',
    // Keep a trace (and its screenshots) only for tests that fail, so red CI
    // runs upload a debuggable artifact while green runs stay lean.
    trace: 'retain-on-failure',
    launchOptions: {
      args: ['--no-sandbox'],
      ...(chromiumExecutable ? { executablePath: chromiumExecutable } : {}),
    },
  },
  // Auto-start trunk if it isn't already serving on :1420. The initial wasm
  // build takes ~1 min from cold so give it a generous timeout.
  webServer: {
    command: 'trunk serve --port 1420',
    cwd: path.resolve(__dirname, '..'),
    url: 'http://localhost:1420',
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
