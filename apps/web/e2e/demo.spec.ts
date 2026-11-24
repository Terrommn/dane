import { expect, test } from "@playwright/test";

// Espacio demo opt-in: usuario aislado con datos [DEMO] en todos los módulos, consultados por la UI real.
const MODULES: [string, RegExp][] = [
  ["/objetivos", /\[DEMO\] Bajar a 70 kg/],
  ["/habitos", /Ejercicio/],
  ["/tareas", /\[DEMO\]/],
  ["/inbox", /\[DEMO\] Idea/],
  ["/personas", /\[DEMO\] Ana López/],
  ["/lecturas", /\[DEMO\] Hábitos atómicos/],
  ["/contenido", /\[DEMO\] 3 hábitos para tu mañana/],
  ["/escuela", /\[DEMO\] Cálculo/],
  ["/notas", /\[DEMO\] SKIP LOCKED/],
  ["/finanzas/movimientos", /\[DEMO\] Supermercado/],
  ["/finanzas", /Consumo/],
  ["/dashboard", /\[DEMO\] Leer 4 libros/],
];

test("explorar con datos demo muestra un espacio aislado y poblado", async ({ page }) => {
  await page.goto("/login");
  await page.getByRole("button", { name: /Explorar con datos demo/ }).click();
  await expect(page.getByText(/Modo demo/).first()).toBeVisible();
  for (const [route, text] of MODULES) {
    await page.goto(route);
    await expect(page.locator("main").getByText(text).filter({ visible: true }).first(), route).toBeVisible();
  }
  const me = await (await page.request.get("/api/v1/me")).json();
  expect(me.user.is_demo).toBe(true);
});
