import { expect, test } from "@playwright/test";
import { devLogin, expectNoHorizontalOverflow } from "./helpers";

// Revisión visual: cada pantalla carga sin error, sin "módulo en integración" y sin scroll horizontal.
const ROUTES = [
  "/hoy", "/objetivos", "/objetivos/planificar", "/habitos", "/dashboard", "/semanal", "/tareas", "/inbox",
  "/finanzas", "/finanzas/movimientos", "/personas", "/lecturas", "/contenido", "/swipe",
  "/escuela", "/dev", "/notas", "/dibujos", "/buscar", "/agenda", "/vistas", "/proyectos", "/ajustes",
  "/onboarding", "/instalar", "/mas", "/importar", "/exportar",
];

test("todas las pantallas cargan sin error ni desbordamiento", async ({ page }, info) => {
  await devLogin(page, 111);
  const problems: string[] = [];
  page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
  for (const route of ROUTES) {
    await page.goto(route);
    await page.waitForLoadState("networkidle");
    await expect(page.locator("main, [role=main]").first(), route).toBeVisible();
    const text = await page.locator("body").innerText();
    if (/se está integrando/i.test(text)) problems.push(`${route}: pantalla pendiente`);
    if (/Página no encontrada/.test(text)) problems.push(`${route}: no encontrada`);
    await expectNoHorizontalOverflow(page, route);
    await page.screenshot({ path: `test-results/screens/${info.project.name}${route.replaceAll("/", "_") || "_root"}.png`, fullPage: true });
  }
  expect(problems).toEqual([]);
});
