import { defineConfig, devices } from "@playwright/test";

// E2E contra el server real (Rust) que sirve el build de la web en el mismo origen.
// Requisitos: `pnpm -C apps/web build`, `cargo build -p server` y PostgreSQL local (scripts/dev-db.sh).
// En macOS 13 Playwright no trae Chromium: se usa Google Chrome instalado (PW_CHANNEL=chrome por defecto en macOS).
const channel = process.env.PW_CHANNEL ?? (process.platform === "darwin" ? "chrome" : undefined);

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [["list"], ["html", { open: "never", outputFolder: "playwright-report" }]],
  use: {
    baseURL: "http://localhost:8099",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    locale: "es-MX",
    timezoneId: "America/Monterrey",
    ...(channel ? { channel } : {}),
  },
  projects: [
    { name: "movil", use: { ...devices["Pixel 7"], ...(channel ? { channel } : {}) } },
    { name: "escritorio", use: { viewport: { width: 1280, height: 800 } } },
  ],
  webServer: {
    command: "bash e2e/server.sh",
    url: "http://localhost:8099/readyz",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
