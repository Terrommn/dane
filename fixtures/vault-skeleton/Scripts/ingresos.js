// Resumen de una nota de ingresos mensual. Uso: await dv.view("Scripts/ingresos")
// Lee la tabla | Fecha | Concepto | Monto | Categoría | Cuenta | de la nota actual y la compara con Gastos/Gastos YYYY-MM.md.

const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const filas = text => (text || "").split("\n").map(l => l.trim()).filter(l => l.startsWith("|"))
  .map(l => l.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim()))
  .filter(c => c.length >= 3 && /^\d{4}-\d{2}-\d{2}$/.test(c[0]));

const cur = dv.current();
const rows = filas(await dv.io.load(cur.file.path)).map(c => ({ monto: monto(c[2]), categoria: c[3] || "Otro" }));
const total = rows.reduce((a, r) => a + r.monto, 0);

const ym = cur.file.name.replace("Ingresos ", "");
const gastosPath = `Proyectos/personal/Finanzas/Gastos/Gastos ${ym}.md`;
const gastos = filas(await dv.io.load(gastosPath)).reduce((a, c) => a + monto(c[2]), 0);
const neto = total - gastos;
const lista = ((await dv.io.load("Proyectos/personal/Finanzas/Ingresos fijos.md")) || "").split("\n").map(l => l.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim()))
  .filter(c => c.length >= 6 && c[5] === "activo");
const esperado = lista.reduce((a, c) => a + monto(c[1]), 0);
dv.paragraph(`**Ingresos del mes:** ${dinero(total)}${esperado ? " de " + dinero(esperado) + " esperados" : ""} · **Gastos:** ${dinero(gastos)} · **Me quedó:** ${neto >= 0 ? "🟢 " + dinero(neto) : "🔴 −" + dinero(-neto)} · **Registros:** ${rows.length}`);

const porCat = {};
for (const r of rows) porCat[r.categoria] = (porCat[r.categoria] || 0) + r.monto;
const cats = Object.entries(porCat).sort((a, b) => b[1] - a[1]);
if (cats.length) dv.table(["Categoría", "Total", "%"], cats.map(([c, v]) => [c, dinero(v), total ? Math.round((v / total) * 100) + "%" : "—"]));
