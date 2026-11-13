// Gastos y presupuesto de un proyecto, leídos de las notas Gastos YYYY-MM (no copia nada: solo los muestra).
// Uso: await dv.view("Scripts/gastos-proyecto", { proyecto: "negocio" })
// Cuenta una fila si su categoría es el proyecto o si el concepto enlaza [[proyecto]].
// Desglose: tabla | Rubro | Presupuesto | Palabras clave | bajo "## Presupuesto" de la nota del proyecto;
// cada gasto cae en el primer rubro con una palabra clave en su concepto (si ninguna, "Sin rubro").

const DIR = "Proyectos/personal/Finanzas";
const p = String(input?.proyecto || dv.current().file.name).toLowerCase();
const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const limpio = s => s.replace(/\[\[([^\]|]+\\?\|)?([^\]]+)\]\]/g, "$2");
const celdas = l => l.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
const seccion = (text, h) => ((text || "").match(new RegExp(`^## ${h}[^\\n]*\\n([\\s\\S]*?)(?=^## |(?![\\s\\S]))`, "m")) || [, ""])[1];
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// Rubros del proyecto
const rubros = seccion(await dv.io.load(dv.current().file.path), "Presupuesto").split("\n").map(celdas)
  .filter(c => c.length >= 3 && c[0] !== "Rubro" && !/^-+$/.test(c[0]))
  .map(c => ({ nombre: c[0], pres: monto(c[1]), re: new RegExp(`\\b(${c[2].split(",").map(k => esc(k.trim())).filter(Boolean).join("|")})`, "i") }));
const rubroDe = concepto => (rubros.find(r => r.re.source !== "\\b()" && r.re.test(concepto)) || { nombre: "Sin rubro" }).nombre;

// Gastos del proyecto y presupuesto de su categoría en cada mes
const filas = [], presMes = {};
for (const pg of dv.pages(`"${DIR}/Gastos"`).where(x => /^Gastos \d{4}-\d{2}$/.test(x.file.name))) {
  const text = await dv.io.load(pg.file.path);
  const ym = pg.file.name.slice(7);
  for (const c of seccion(text, "Presupuesto").split("\n").map(celdas))
    if (c.length >= 2 && c[0].toLowerCase() === p) presMes[ym] = monto(c[1]);
  for (const l of (text || "").split("\n")) {
    const c = celdas(l);
    if (c.length < 4 || !/^\d{4}-\d{2}-\d{2}$/.test(c[0])) continue;
    const enlaza = new RegExp(`\\[\\[${p}(\\\\?\\||\\]\\])`, "i").test(c[1]);
    if (c[3].toLowerCase() === p || enlaza) {
      const concepto = limpio(c[1]);
      filas.push({ fecha: c[0], concepto, monto: monto(c[2]), categoria: c[3], rubro: rubroDe(concepto) });
    }
  }
}
filas.sort((a, b) => b.fecha.localeCompare(a.fecha));

const hoy = new Date();
const ym = `${hoy.getFullYear()}-${String(hoy.getMonth() + 1).padStart(2, "0")}`;

// Este mes: gastado vs presupuesto por rubro
if (rubros.length) {
  const suma = rubros.reduce((a, r) => a + r.pres, 0);
  const delMes = filas.filter(f => f.fecha.startsWith(ym));
  const gas = n => delMes.filter(f => f.rubro === n).reduce((a, f) => a + f.monto, 0);
  const lineas = rubros.map(r => [r.nombre, dinero(gas(r.nombre)), dinero(r.pres), gas(r.nombre) <= r.pres ? "🟢 " + dinero(r.pres - gas(r.nombre)) : "🔴 +" + dinero(gas(r.nombre) - r.pres)]);
  if (gas("Sin rubro")) lineas.push(["Sin rubro", dinero(gas("Sin rubro")), "—", "revisa el concepto"]);
  const total = delMes.reduce((a, f) => a + f.monto, 0);
  lineas.push(["**Total**", `**${dinero(total)}**`, `**${dinero(suma)}**`, total <= suma ? "🟢 " + dinero(suma - total) : "🔴 +" + dinero(total - suma)]);
  dv.header(4, `Este mes (${ym})`);
  dv.table(["Rubro", "Gastado", "Presupuesto", "Queda / Me pasé"], lineas);
  if (presMes[ym] !== undefined && presMes[ym] !== suma)
    dv.paragraph(`⚠️ El desglose suma ${dinero(suma)} pero en Gastos ${ym} la categoría dice ${dinero(presMes[ym])}. Iguálalos.`);
}

const subs = (await dv.io.load(`${DIR}/Suscripciones.md`) || "").split("\n").map(celdas)
  .filter(c => c.length >= 7 && c[6] === "activa" && c[5].toLowerCase().includes(`[[${p}]]`));
dv.paragraph(`**Fijo al mes (suscripciones):** ${dinero(subs.reduce((a, c) => a + monto(c[1]), 0))}` +
  (subs.length ? ` (${subs.map(c => `${c[0]} ${c[1]}`).join(", ")})` : ""));

const porMes = {};
for (const f of filas) porMes[f.fecha.slice(0, 7)] = (porMes[f.fecha.slice(0, 7)] || 0) + f.monto;
dv.header(4, "Por mes");
dv.table(["Mes", "Gastado", "Presupuesto"], Object.entries(porMes).sort((a, b) => b[0].localeCompare(a[0]))
  .map(([m, t]) => [m, dinero(t), presMes[m] !== undefined ? dinero(presMes[m]) : "—"]));
dv.header(4, "Detalle");
dv.table(["Fecha", "Concepto", "Monto", "Rubro"], filas.map(f => [f.fecha, f.concepto, dinero(f.monto), f.rubro]));
