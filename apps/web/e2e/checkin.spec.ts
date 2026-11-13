import { expect, test } from "@playwright/test";
import { devLogin, todayLocal } from "./helpers";

test("check-in: guardar, recargar y ver el mismo dato en el servidor y el dashboard", async ({ page }) => {
  await devLogin(page, 111);
  await page.goto(`/hoy?date=${todayLocal()}`);
  await page.locator("fieldset", { hasText: "Energía (1–5)" }).getByText("4", { exact: true }).click();
  await page.getByLabel("Lo que hice bien hoy 1").fill("Salí a caminar");
  await page.getByRole("button", { name: "Guardar", exact: true }).click();
  await expect(page.getByText("Guardado", { exact: true }).first()).toBeVisible();

  const res = await page.request.get(`/api/v1/checkins/${todayLocal()}`);
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  expect(body.checkin.energy).toBe(4);

  await page.reload();
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" }).locator("input[type=radio]").nth(3)).toBeChecked();

  await page.goto("/dashboard");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByText(/no se pudo|error inesperado/i)).toHaveCount(0);
});

test("check-in sin conexión queda pendiente y se sincroniza al reconectar", async ({ page, context }) => {
  await devLogin(page, 111);
  const date = todayLocal();
  await page.goto(`/hoy?date=${date}`);
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" })).toBeVisible();
  // Espera a que el service worker controle la página (app shell precacheado).
  await page.evaluate(async () => {
    await navigator.serviceWorker?.ready;
  });
  await context.setOffline(true);
  await page.locator("fieldset", { hasText: "Energía (1–5)" }).getByText("2", { exact: true }).click();
  await page.getByRole("button", { name: "Guardar", exact: true }).click();
  await expect(page.getByText("Pendiente de sincronizar").first()).toBeVisible();
  await expect(page.getByText("Guardado", { exact: true })).toHaveCount(0);

  // Reabrir sin red: el shell carga desde el service worker y el borrador sigue ahí.
  await page.reload();
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" }).locator("input[type=radio]").nth(1)).toBeChecked();

  await context.setOffline(false);
  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await expect(page.getByText("Guardado", { exact: true }).first()).toBeVisible({ timeout: 20_000 });
  const body = await (await page.request.get(`/api/v1/checkins/${date}`)).json();
  expect(body.checkin.energy).toBe(2);
});

test("los datos locales no se filtran entre usuarios", async ({ page, context }) => {
  await devLogin(page, 111);
  const date = todayLocal();
  await page.goto(`/hoy?date=${date}`);
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" })).toBeVisible();
  await context.setOffline(true);
  await page.locator("fieldset", { hasText: "Energía (1–5)" }).getByText("5", { exact: true }).click();
  await page.getByRole("button", { name: "Guardar", exact: true }).click();
  await expect(page.getByText("Pendiente de sincronizar").first()).toBeVisible();
  await context.setOffline(false);

  // Cambiar a otra cuenta sin cerrar sesión: el borrador de 111 no debe verse para 222.
  const before = await page.evaluate(async () => (await indexedDB.databases()).map((d) => d.name));
  await page.request.post("/api/v1/auth/logout", { headers: { "x-csrf-token": (await context.cookies()).find((c) => c.name === "vida_csrf")?.value ?? "" } });
  await devLogin(page, 222);
  await page.goto(`/hoy?date=${date}`);
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" })).toBeVisible();
  await expect(page.locator("fieldset", { hasText: "Energía (1–5)" }).locator("input[type=radio]:checked")).toHaveCount(0);
  const other = await (await page.request.get(`/api/v1/checkins/${date}`)).json();
  expect(other.checkin?.energy ?? null).not.toBe(5);
  expect(before.some((n) => n?.startsWith("vida-offline-"))).toBeTruthy();
});
