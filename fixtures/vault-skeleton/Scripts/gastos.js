// Resumen de una nota de gastos mensual. Uso: await dv.view("Scripts/gastos")
// Lee la tabla | Fecha | Concepto | Monto | Categoría | Pago | de la nota actual.

const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };

const text = await dv.io.load(dv.current().file.path);
const rows = [];
for (const line of text.split("\n")) {
  const t = line.trim();
  if (!t.startsWith("|")) continue;
  const c = t.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
  if (c.length < 3 || !/^\d{4}-\d{2}-\d{2}$/.test(c[0])) continue;
  rows.push({ fecha: c[0], monto: monto(c[2]), categoria: c[3] || "Otro" });
}

const total = rows.reduce((a, r) => a + r.monto, 0);
const sinDeuda = rows.filter(r => r.categoria !== "Pago de deuda").reduce((a, r) => a + r.monto, 0);
dv.paragraph(`**Total del mes:** ${dinero(total)} · **Gasto (sin pagos de deuda):** ${dinero(sinDeuda)} · **Registros:** ${rows.length}`);

const porCat = {};
for (const r of rows) porCat[r.categoria] = (porCat[r.categoria] || 0) + r.monto;

// Presupuesto del mes: tabla | Categoría | Presupuesto | bajo "## Presupuesto" de esta misma nota
const presupuesto = {};
const m = text.match(/^## Presupuesto[^\n]*\n([\s\S]*?)(?=^## |(?![\s\S]))/m);
if (m) {
  for (const line of m[1].split("\n")) {
    const c = line.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
    if (c.length < 2 || c[0] === "Categoría" || /^-+$/.test(c[0])) continue;
    const n = monto(c[1]);
    if (c[1] && n > 0) presupuesto[c[0]] = n;
  }
}

const tope = Object.entries(presupuesto).filter(([c]) => c !== "Pago de deuda").reduce((a, [, v]) => a + v, 0);
if (tope) {
  const usado = Object.keys(presupuesto).filter(c => c !== "Pago de deuda").reduce((a, c) => a + (porCat[c] || 0), 0);
  const dif = tope - usado;
  dv.paragraph(`**Presupuesto:** ${dinero(usado)} de ${dinero(tope)} · ${dif >= 0 ? "te quedan " + dinero(dif) : "🔴 te pasaste " + dinero(-dif)}`);
}

// Con presupuesto: tabla de gastado vs presupuesto + pie de cómo se reparte el presupuesto
const conPres = Object.keys(presupuesto)
  .map(c => [c, porCat[c] || 0, presupuesto[c]])
  .sort((a, b) => b[2] - a[2]);
if (conPres.length) {
  // Color fijo por categoría (sigue a la categoría, no a su posición). Rojo reservado para "me pasé".
  const COLOR = {
    "Nómina": "var(--color-blue)", "Gasolina": "var(--color-orange)", "Pareja": "var(--color-pink)",
    "Salidas con amigos": "var(--color-purple)", "Suscripciones": "var(--color-cyan)", "Comida": "var(--color-green)",
    "Casa": "var(--color-yellow)", "Salud/gym": "#8a9a5b", "Personal/ocio": "#b0855f", "Trabajo": "#6c7a89", "Negocio": "#2f9e8f", "Inversión": "#c9a227",
    "Pago de deuda": "#9e7bb5", "Otro": "var(--text-faint)",
  };
  const wrap = dv.el("div", "", { attr: { style: "display:flex;flex-wrap:wrap;gap:24px;align-items:flex-start" } });
  wrap.createEl("style", { text: ".gp-t{border-collapse:collapse;font-variant-numeric:tabular-nums}.gp-t th,.gp-t td{padding:4px 10px;border-bottom:1px solid var(--background-modifier-border);text-align:right}.gp-t th:first-child,.gp-t td:first-child{text-align:left}.gp-dot{display:inline-block;width:10px;height:10px;border-radius:50%;margin-right:6px}" });

  // Tabla
  const tbl = wrap.createDiv({ attr: { style: "flex:1 1 360px;min-width:0;overflow-x:auto" } }).createEl("table", { cls: "gp-t" });
  const hr = tbl.createEl("tr");
  for (const t of ["Categoría", "Gastado", "Presupuesto", "Queda / Me pasé", "%"]) hr.createEl("th", { text: t });
  for (const [c, v, p] of conPres) {
    const tr = tbl.createEl("tr");
    const td = tr.createEl("td");
    td.createSpan({ cls: "gp-dot" }).style.background = COLOR[c] || "var(--text-muted)";
    td.appendText(c);
    tr.createEl("td", { text: dinero(v) });
    tr.createEl("td", { text: dinero(p) });
    tr.createEl("td", { text: v <= p ? "🟢 " + dinero(p - v) : "🔴 +" + dinero(v - p) });
    tr.createEl("td", { text: Math.round((v / p) * 100) + "%" });
  }

  // Pie (dona) del presupuesto por categoría
  const totalPres = conPres.reduce((a, [, , p]) => a + p, 0);
  const box = wrap.createDiv({ attr: { style: "flex:0 0 220px;text-align:center" } });
  box.createDiv({ text: "Cómo reparto el presupuesto", attr: { style: "font-size:.85em;color:var(--text-muted);margin-bottom:6px" } });
  const NS = "http://www.w3.org/2000/svg", R = 90, r = 52, C = 100;
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 200 200"); svg.setAttribute("width", "200"); svg.setAttribute("height", "200");
  box.appendChild(svg);
  let ang = -Math.PI / 2;
  const pt = (rad, a) => `${C + rad * Math.cos(a)},${C + rad * Math.sin(a)}`;
  for (const [c, , p] of conPres) {
    const frac = p / totalPres, a2 = ang + frac * 2 * Math.PI, big = frac > 0.5 ? 1 : 0;
    const path = document.createElementNS(NS, "path");
    const d = frac >= 0.9999
      ? `M${pt(R, -Math.PI / 2)} A${R},${R} 0 1 1 ${pt(R, Math.PI * 1.4999)} L${pt(r, Math.PI * 1.4999)} A${r},${r} 0 1 0 ${pt(r, -Math.PI / 2)} Z`
      : `M${pt(R, ang)} A${R},${R} 0 ${big} 1 ${pt(R, a2)} L${pt(r, a2)} A${r},${r} 0 ${big} 0 ${pt(r, ang)} Z`;
    path.setAttribute("d", d);
    path.setAttribute("fill", COLOR[c] || "var(--text-muted)");
    path.setAttribute("stroke", "var(--background-primary)"); path.setAttribute("stroke-width", "2");
    const t = document.createElementNS(NS, "title"); t.textContent = `${c}: ${dinero(p)} (${Math.round(frac * 100)}%)`; path.appendChild(t);
    svg.appendChild(path);
    ang = a2;
  }
  const mid = document.createElementNS(NS, "text");
  mid.setAttribute("x", C); mid.setAttribute("y", C + 5); mid.setAttribute("text-anchor", "middle");
  mid.setAttribute("style", "fill:var(--text-normal);font-size:15px;font-weight:600"); mid.textContent = dinero(totalPres);
  svg.appendChild(mid);
  const leg = box.createDiv({ attr: { style: "font-size:.8em;text-align:left;display:inline-block;margin-top:6px" } });
  for (const [c, , p] of conPres) {
    const row = leg.createDiv();
    row.createSpan({ cls: "gp-dot" }).style.background = COLOR[c] || "var(--text-muted)";
    row.appendText(`${c} · ${Math.round((p / totalPres) * 100)}%`);
  }
}

// Sin presupuesto: solo lo gastado, aparte
const sinPres = Object.entries(porCat).filter(([c]) => !presupuesto[c]).sort((a, b) => b[1] - a[1]);
if (sinPres.length) {
  dv.paragraph(`**Fuera del presupuesto:** ${sinPres.map(([c, v]) => `${c} ${dinero(v)}`).join(" · ")} · total ${dinero(sinPres.reduce((a, [, v]) => a + v, 0))}`);
}
