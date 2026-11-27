import { defineConfig } from "@playwright/test";
import { existsSync } from "node:fs";

const systemChromium =
  process.env.CHROMIUM ??
  (process.env.CI
    ? undefined
    : existsSync("/usr/bin/chromium")
      ? "/usr/bin/chromium"
      : undefined);

export default defineConfig({
  testDir: "./e2e",
  timeout: 240_000,
  expect: { timeout: 20_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [
    ["list"],
    [
      "html",
      { outputFolder: "../target/e2e/playwright-report", open: "never" },
    ],
  ],
  outputDir: "../target/e2e/playwright-results",
  use: {
    viewport: { width: 1280, height: 800 },
    launchOptions: {
      executablePath: systemChromium,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
});
