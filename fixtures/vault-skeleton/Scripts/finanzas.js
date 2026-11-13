// Dashboard de finanzas. Uso: await dv.view("Scripts/finanzas")
// Lee Proyectos/personal/Finanzas/Gastos/Gastos YYYY-MM.md (## Presupuesto y ## Registro de cada mes) y Suscripciones.md.

const DIR = "Proyectos/personal/Finanzas";
const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const MESES = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
const nombreMes = ym => MESES[+ym.slice(5) - 1] + " " + ym.slice(2, 4);

async function tabla(path) {
  const text = await dv.io.load(path);
  if (!text) return [];
  return text.split("\n").map(l => l.trim()).filter(l => l.startsWith("|"))
    .map(l => l.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim()));
}

// Presupuesto de una nota: la tabla | Categoría | Presupuesto | bajo "## Presupuesto"
function presupuestoDe(text) {
  const pres = {};
  const m = (text || "").match(/^## Presupuesto[^\n]*\n([\s\S]*?)(?=^## |(?![\s\S]))/m);
  if (!m) return pres;
  for (const l of m[1].split("\n")) {
    const c = l.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
    if (c.length < 2 || c[0] === "Categoría" || /^-+$/.test(c[0])) continue;
    if (monto(c[1]) > 0) pres[c[0]] = monto(c[1]);
  }
  return pres;
}
const topeDe = pres => Object.entries(pres).filter(([c]) => c !== "Pago de deuda").reduce((a, [, v]) => a + v, 0);

// Gastos y presupuesto de todos los meses
const meses = {}, presupuestos = {};
for (const p of dv.pages(`"${DIR}"`).where(p => /^Gastos \d{4}-\d{2}$/.test(p.file.name))) {
  const ym = p.file.name.slice(7);
  const text = await dv.io.load(p.file.path);
  presupuestos[ym] = presupuestoDe(text);
  meses[ym] = (text || "").split("\n").map(l => l.trim()).filter(l => l.startsWith("|"))
    .map(l => l.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim()))
    .filter(c => c.length >= 3 && /^\d{4}-\d{2}-\d{2}$/.test(c[0]))
    .map(c => ({ fecha: c[0], concepto: c[1].replace(/\[\[([^\]|]+\|)?([^\]]+)\]\]/g, "$2"), monto: monto(c[2]), categoria: c[3] || "Otro", pago: c[4] || "" }));
}

// Ingresos de todos los meses: Ingresos/Ingresos YYYY-MM.md
const ingresos = {};
for (const p of dv.pages(`"${DIR}"`).where(p => /^Ingresos \d{4}-\d{2}$/.test(p.file.name))) {
  ingresos[p.file.name.slice(9)] = (await tabla(p.file.path))
    .filter(c => c.length >= 3 && /^\d{4}-\d{2}-\d{2}$/.test(c[0])).reduce((a, c) => a + monto(c[2]), 0);
}
const esperado = (await tabla(`${DIR}/Ingresos fijos.md`)).filter(c => c.length >= 6 && c[5] === "activo").reduce((a, c) => a + monto(c[1]), 0);
const gastoTotal = k => (meses[k] || []).reduce((a, r) => a + r.monto, 0); // incluye pagos de deuda: es dinero que salió

const subs = (await tabla(`${DIR}/Suscripciones.md`))
  .filter(c => c.length >= 7 && c[0] !== "Servicio" && !/^-+$/.test(c[0]) && c[6] === "activa");
const totalSubs = subs.reduce((a, c) => a + monto(c[1]), 0);

const hoy = new Date();
const ymHoy = `${hoy.getFullYear()}-${String(hoy.getMonth() + 1).padStart(2, "0")}`;
const ym = dv.current().mes ? String(dv.current().mes) : ymHoy;
const rows = (meses[ym] || []).filter(r => r.categoria !== "Pago de deuda");
const diasMes = new Date(+ym.slice(0, 4), +ym.slice(5), 0).getDate();
const diaHoy = ym === ymHoy ? hoy.getDate() : diasMes;
const gastado = rows.reduce((a, r) => a + r.monto, 0);
const presupuesto = presupuestos[ym] || {};
const tope = topeDe(presupuesto);
// Proyección: lo variable al ritmo diario + las suscripciones completas (son fijas, no se reparten por día)
const variable = rows.filter(r => !/^Suscripción /.test(r.concepto)).reduce((a, r) => a + r.monto, 0);
const proyeccion = (diaHoy ? (variable / diaHoy) * diasMes : 0) + totalSubs;
// Escala "bonita" para los ejes: 0, 2,500, 5,000…
const nice = v => { const p = Math.pow(10, Math.floor(Math.log10(v))); return [1, 2, 2.5, 5, 10].map(m => m * p).find(m => m * 4 >= v) * 4; };

// Estilos: tokens de Obsidian para que funcione en tema claro y oscuro
const css = `
.fin { --fin-acc: var(--interactive-accent); --fin-bad: var(--color-red); --fin-ok: var(--color-green); --fin-grid: var(--background-modifier-border); --fin-muted: var(--text-muted); }
.fin h3 { margin: 1.4em 0 .4em; }
.fin .kpis { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
.fin .kpi { border: 1px solid var(--fin-grid); border-radius: 8px; padding: 10px 12px; }
.fin .kpi .l { font-size: .8em; color: var(--fin-muted); }
.fin .kpi .v { font-size: 1.5em; font-weight: 600; font-variant-numeric: tabular-nums; }
.fin .kpi .s { font-size: .8em; color: var(--fin-muted); }
.fin .bad { color: var(--fin-bad); } .fin .ok { color: var(--fin-ok); }
.fin .cat { display: grid; grid-template-columns: 130px 1fr 150px; gap: 8px; align-items: center; margin: 6px 0; font-size: .9em; }
.fin .track { position: relative; height: 12px; background: var(--background-secondary); border-radius: 4px; }
.fin .fill { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 0 4px 4px 0; background: var(--fin-acc); }
.fin .fill.over { background: var(--fin-bad); }
.fin .mark { position: absolute; top: -3px; bottom: -3px; width: 2px; background: var(--text-normal); }
.fin .num { text-align: right; font-variant-numeric: tabular-nums; }
.fin svg text { fill: var(--fin-muted); font-size: 11px; }
.fin .leg { font-size: .8em; color: var(--fin-muted); }
@media (max-width: 500px) { .fin .cat { grid-template-columns: 90px 1fr; } .fin .cat .num { grid-column: 1 / -1; text-align: left; } }
`;
const root = dv.el("div", "", { cls: "fin" });
root.createEl("style", { text: css });
const h = (tag, text, parent = root, cls) => parent.createEl(tag, { text, cls });

// 1. KPIs
h("h3", `Resumen ${nombreMes(ym)}`);
const kpis = root.createDiv({ cls: "kpis" });
const kpi = (l, v, s, cls) => { const d = kpis.createDiv({ cls: "kpi" }); h("div", l, d, "l"); h("div", v, d, "v " + (cls || "")); if (s) h("div", s, d, "s"); };
kpi("Gastado (sin deudas)", dinero(gastado), `${rows.length} registros`);
if (tope) {
  const queda = tope - gastado;
  kpi(queda >= 0 ? "Me queda del presupuesto" : "Me pasé del presupuesto", dinero(Math.abs(queda)), `de ${dinero(tope)}`, queda >= 0 ? "ok" : "bad");
} else kpi("Presupuesto", "—", `llénalo en Gastos ${ym}`);
kpi("Proyección fin de mes", dinero(proyeccion), tope ? (proyeccion > tope ? `🔴 ${dinero(proyeccion - tope)} arriba` : `🟢 dentro`) : `${dinero(variable / Math.max(diaHoy, 1))}/día + suscripciones`, tope && proyeccion > tope ? "bad" : "");
if (ingresos[ym] !== undefined) {
  const neto = ingresos[ym] - gastoTotal(ym);
  kpi("Ingresos", dinero(ingresos[ym]), esperado ? `de ${dinero(esperado)} esperados` : `vs ${dinero(gastoTotal(ym))} de gastos`);
  kpi(neto >= 0 ? "Me quedó" : "Gasté de más", dinero(Math.abs(neto)), "ingresos − gastos (con deudas)", neto >= 0 ? "ok" : "bad");
}
kpi("Suscripciones / mes", dinero(totalSubs), `${subs.length} activas`);

// 1b. Plan del mes: ingreso esperado vs presupuesto (incluye pagos de deuda presupuestados)
const presTotal = Object.values(presupuesto).reduce((a, v) => a + v, 0);
if (esperado || presTotal) {
  const sobrante = esperado - presTotal;
  h("h3", "Plan del mes: ingreso esperado vs presupuesto");
  const max = Math.max(esperado, presTotal, 1);
  const barraPlan = (label, v, color) => {
    const row = root.createDiv({ cls: "cat" });
    h("div", label, row);
    const tr = row.createDiv({ cls: "track" });
    tr.setAttr("title", `${label}: ${dinero(v)}`);
    const f = tr.createDiv({ cls: "fill" });
    f.style.width = (v / max) * 100 + "%"; f.style.background = color;
    h("div", dinero(v), row, "num");
  };
  barraPlan("Ingreso esperado", esperado, "var(--color-blue)");
  barraPlan("Presupuesto", presTotal, "var(--color-orange)");
  const k2 = root.createDiv({ cls: "kpis" });
  const d = k2.createDiv({ cls: "kpi" });
  h("div", sobrante >= 0 ? "Sobrante esperado (lo que planeo que me quede)" : "Me falta para cubrir el presupuesto", d, "l");
  h("div", dinero(Math.abs(sobrante)), d, "v " + (sobrante >= 0 ? "ok" : "bad"));
  h("div", `${dinero(esperado)} esperados − ${dinero(presTotal)} de presupuesto` + (esperado ? ` · ${Math.round((sobrante / esperado) * 100)}% del ingreso` : ""), d, "s");
  if (!esperado) h("div", "Llena Ingresos fijos para ver el ingreso esperado.", root, "leg");
}

// 2. Gasto acumulado del mes vs ritmo del presupuesto
const svgNS = "http://www.w3.org/2000/svg";
const svg = (parent, w, hgt) => { const s = document.createElementNS(svgNS, "svg"); s.setAttribute("viewBox", `0 0 ${w} ${hgt}`); s.setAttribute("width", "100%"); parent.appendChild(s); return s; };
const el = (s, tag, attrs, title) => { const e = document.createElementNS(svgNS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); if (title) { const t = document.createElementNS(svgNS, "title"); t.textContent = title; e.appendChild(t); } s.appendChild(e); return e; };
const txt = (s, x, y, t, anchor = "start") => { const e = el(s, "text", { x, y, "text-anchor": anchor }); e.textContent = t; };

h("h3", "Gasto acumulado del mes");
{
  const W = 640, H = 220, L = 52, R = 12, T = 12, B = 26;
  const porDia = Array(diasMes + 1).fill(0);
  for (const r of rows) porDia[+r.fecha.slice(8)] += r.monto;
  const acum = []; let a = 0;
  for (let d = 1; d <= diaHoy; d++) { a += porDia[d]; acum.push([d, a]); }
  const maxY = nice(Math.max(tope, a, 1));
  const x = d => L + ((d - 1) / (diasMes - 1)) * (W - L - R);
  const y = v => H - B - (v / maxY) * (H - T - B);
  const s = svg(root, W, H);
  for (let i = 0; i <= 4; i++) {
    const v = (maxY / 4) * i;
    el(s, "line", { x1: L, x2: W - R, y1: y(v), y2: y(v), stroke: "var(--fin-grid)", "stroke-width": 1 });
    txt(s, L - 6, y(v) + 4, dinero(v), "end");
  }
  for (const d of [1, 8, 15, 22, diasMes]) txt(s, x(d), H - 8, String(d), "middle");
  if (tope) el(s, "line", { x1: x(1), y1: y(0), x2: x(diasMes), y2: y(tope), stroke: "var(--fin-muted)", "stroke-width": 2, "stroke-dasharray": "5 5" }, `Ritmo del presupuesto: ${dinero(tope)} al día ${diasMes}`);
  if (acum.length) {
    el(s, "polyline", { points: acum.map(([d, v]) => `${x(d)},${y(v)}`).join(" "), fill: "none", stroke: "var(--fin-acc)", "stroke-width": 2, "stroke-linejoin": "round" });
    for (const [d, v] of acum) if (porDia[d]) el(s, "circle", { cx: x(d), cy: y(v), r: 4, fill: "var(--fin-acc)", stroke: "var(--background-primary)", "stroke-width": 2 }, `Día ${d}: +${dinero(porDia[d])} · acumulado ${dinero(v)}`);
  }
  h("div", tope ? "Línea sólida: lo que llevo gastado. Punteada: hasta dónde debería ir para no pasarme. Pasa el mouse por los puntos para ver el detalle." : "Pasa el mouse por los puntos para ver el detalle. Llena el presupuesto del mes para ver la línea del ritmo.", root, "leg");
}

// 3. Categorías vs presupuesto
h("h3", "Por categoría vs presupuesto");
{
  const porCat = {};
  for (const r of rows) porCat[r.categoria] = (porCat[r.categoria] || 0) + r.monto;
  const cats = [...new Set([...Object.keys(presupuesto).filter(c => c !== "Pago de deuda"), ...Object.keys(porCat)])]
    .map(c => [c, porCat[c] || 0, presupuesto[c] || 0]).sort((a, b) => b[1] - a[1]);
  const max = Math.max(...cats.map(([, v, p]) => Math.max(v, p)), 1);
  for (const [c, v, p] of cats) {
    const row = root.createDiv({ cls: "cat" });
    h("div", c, row);
    const tr = row.createDiv({ cls: "track" });
    tr.setAttr("title", `${c}: ${dinero(v)}${p ? " de " + dinero(p) : ""}`);
    tr.createDiv({ cls: "fill" + (p && v > p ? " over" : "") }).style.width = (v / max) * 100 + "%";
    if (p) tr.createDiv({ cls: "mark" }).style.left = `calc(${(p / max) * 100}% - 1px)`;
    h("div", p ? (v <= p ? `${dinero(v)} · 🟢 quedan ${dinero(p - v)}` : `${dinero(v)} · 🔴 +${dinero(v - p)}`) : dinero(v), row, "num");
  }
  h("div", "La raya vertical es el presupuesto de la categoría; la barra se pone roja si me paso.", root, "leg");
}

// 4. Últimos 6 meses
h("h3", "Últimos 6 meses: ingresos vs gastos (sin deudas)");
{
  const lista = [];
  for (let i = 5; i >= 0; i--) {
    const d = new Date(+ym.slice(0, 4), +ym.slice(5) - 1 - i, 1);
    const k = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
    lista.push([k, (meses[k] || []).filter(r => r.categoria !== "Pago de deuda").reduce((a, r) => a + r.monto, 0), topeDe(presupuestos[k] || {}), ingresos[k] || 0]);
  }
  const W = 640, H = 200, L = 52, R = 12, T = 18, B = 26;
  const maxY = nice(Math.max(...lista.map(([, v, t, ing]) => Math.max(v, t, ing)), 1) * 1.1);
  const y = v => H - B - (v / maxY) * (H - T - B);
  const bw = (W - L - R) / lista.length;
  const s = svg(root, W, H);
  for (let i = 0; i <= 4; i++) {
    const v = (maxY / 4) * i;
    el(s, "line", { x1: L, x2: W - R, y1: y(v), y2: y(v), stroke: "var(--fin-grid)", "stroke-width": 1 });
    txt(s, L - 6, y(v) + 4, dinero(v), "end");
  }
  const barra = (x0, w, v, fill, tip) => el(s, "path", { d: `M${x0},${y(0)} V${y(v) + 4} q0,-4 4,-4 H${x0 + w - 4} q4,0 4,4 V${y(0)} Z`, fill }, tip);
  lista.forEach(([k, v, t, ing], i) => {
    const x0 = L + i * bw + bw * 0.5 + 1, w = bw * 0.3;
    if (ing) barra(L + i * bw + bw * 0.2 - 1, w, ing, "var(--color-blue)", `${nombreMes(k)}: ingresos ${dinero(ing)}`);
    if (v) el(s, "path", { d: `M${x0},${y(0)} V${y(v) + 4} q0,-4 4,-4 H${x0 + w - 4} q4,0 4,4 V${y(0)} Z`, fill: t && v > t ? "var(--fin-bad)" : "var(--color-orange)" }, `${nombreMes(k)}: ${dinero(v)}${t ? " de " + dinero(t) : ""}`);
    if (t) el(s, "line", { x1: x0 - 8, x2: x0 + w + 8, y1: y(t), y2: y(t), stroke: "var(--text-normal)", "stroke-width": 2, "stroke-dasharray": "4 3" }, `Presupuesto ${nombreMes(k)}: ${dinero(t)}`);
    if (v) txt(s, x0 + w / 2, y(v) - 5, dinero(v), "middle");
    txt(s, x0 - 1, H - 8, nombreMes(k), "middle");
  });
  h("div", "🟦 Ingresos · 🟧 Gastos · 🟥 Gastos arriba del presupuesto · Raya punteada: presupuesto de ese mes. Pasa el mouse por las barras.", root, "leg");
}

// 4b. Historial: presupuesto vs gastado por mes (queda registrado en cada nota del mes)
h("h3", "Historial por mes");
{
  const hist = [...new Set([...Object.keys(meses), ...Object.keys(ingresos)])].sort().reverse().map(k => {
    const neto = (ingresos[k] || 0) - gastoTotal(k);
    const g = (meses[k] || []).filter(r => r.categoria !== "Pago de deuda").reduce((a, r) => a + r.monto, 0);
    const t = topeDe(presupuestos[k] || {});
    return [dv.fileLink(`${DIR}/Gastos/Gastos ${k}`, false, nombreMes(k)), t ? dinero(t) : "—", dinero(g), t ? (g <= t ? "🟢 sobraron " + dinero(t - g) : "🔴 me pasé " + dinero(g - t)) : "—", ingresos[k] !== undefined ? dinero(ingresos[k]) : "—", ingresos[k] !== undefined ? (neto >= 0 ? "🟢 " : "🔴 −") + dinero(Math.abs(neto)) : "—"];
  });
  dv.table(["Mes", "Presupuesto", "Gastado", "Presupuesto vs gastado", "Ingresos", "Me quedó"], hist);
}

// 5. Gastos más grandes del mes
h("h3", "Gastos más grandes del mes");
const top = [...rows].sort((a, b) => b.monto - a.monto).slice(0, 5);
if (top.length) dv.table(["Fecha", "Concepto", "Monto", "Categoría"], top.map(r => [r.fecha, r.concepto, dinero(r.monto), r.categoria]));
else dv.paragraph("Sin gastos este mes.");
