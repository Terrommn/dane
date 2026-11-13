import { expect, test } from "@playwright/test";
import { TELEGRAM_STUB, signedInitData } from "./helpers";

test("Mini App: initData firmado autentica con Bearer en memoria y conserva la fecha objetivo", async ({ page, context }) => {
  await page.route("https://telegram.org/js/telegram-web-app.js*", (r) =>
    r.fulfill({ status: 200, contentType: "application/javascript", body: TELEGRAM_STUB }),
  );
  const initData = signedInitData(111);
  await page.goto(`/hoy?date=2026-11-10#tgWebAppData=${encodeURIComponent(initData)}&tgWebAppVersion=8.0`);
  await expect(page).toHaveURL(/\/hoy\?date=2026-11-10/);
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" })).toBeVisible();
  await expect(page.getByText(/10 de noviembre|2026-11-10/).first()).toBeVisible();
  // Sin cookies de sesión: el token Bearer vive solo en memoria.
  const cookies = await context.cookies();
  expect(cookies.find((c) => c.name === "vida_session")).toBeUndefined();
  const stored = await page.evaluate(() => JSON.stringify({ ...localStorage }) + JSON.stringify({ ...sessionStorage }));
  expect(stored).not.toMatch(/token|bearer/i);
});

test("Mini App: initData alterado no inicia sesión", async ({ page }) => {
  await page.route("https://telegram.org/js/telegram-web-app.js*", (r) =>
    r.fulfill({ status: 200, contentType: "application/javascript", body: TELEGRAM_STUB }),
  );
  const forged = signedInitData(111).replace("%22id%22%3A111", "%22id%22%3A222");
  await page.goto(`/hoy#tgWebAppData=${encodeURIComponent(forged)}&tgWebAppVersion=8.0`);
  await expect(page.getByText(/no se pudo|verificar|vuelve a abrir/i).first()).toBeVisible();
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" })).toHaveCount(0);
});
