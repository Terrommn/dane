// Gráfica de barras: cuánto gasté por categoría en la nota de gastos actual. Uso: await dv.view("Scripts/gastos-categorias")
// Lee la tabla | Fecha | Concepto | Monto | Categoría | Pago | (todos los registros, incluye pagos de deuda).
// Los registros sin categoría salen como su propia barra "Sin categoría".

const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };

const text = await dv.io.load(dv.current().file.path);
const porCat = {};
for (const line of text.split("\n")) {
  const t = line.trim();
  if (!t.startsWith("|")) continue;
  const c = t.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
  if (c.length < 3 || !/^\d{4}-\d{2}-\d{2}$/.test(c[0])) continue;
  const cat = c[3] || "Sin categoría";
  porCat[cat] = (porCat[cat] || 0) + monto(c[2]);
}

// Todas las categorías salen siempre, aunque estén en $0 (las de $0 al final, en el orden de la lista).
const CATEGORIAS = ["Comida", "Gasolina", "Casa", "Salud/gym", "Deporte", "Suscripciones", "Personal/ocio", "Salidas con amigos", "Pareja", "Trabajo", "Inversión", "Negocio", "Nómina", "Pago de deuda", "Otro", "Sin categoría"];
for (const c of CATEGORIAS) porCat[c] = porCat[c] || 0;
const orden = c => { const i = CATEGORIAS.indexOf(c); return i < 0 ? CATEGORIAS.length : i; };
const datos = Object.entries(porCat).sort((a, b) => b[1] - a[1] || orden(a[0]) - orden(b[0]));

// Mismos colores que Scripts/gastos.js
const COLOR = {
  "Nómina": "var(--color-blue)", "Gasolina": "var(--color-orange)", "Pareja": "var(--color-pink)",
  "Salidas con amigos": "var(--color-purple)", "Suscripciones": "var(--color-cyan)", "Comida": "var(--color-green)",
  "Casa": "var(--color-yellow)", "Salud/gym": "#8a9a5b", "Personal/ocio": "#b0855f", "Trabajo": "#6c7a89", "Negocio": "#2f9e8f", "Inversión": "#c9a227",
  "Deporte": "#5a8f3e", "Pago de deuda": "#9e7bb5", "Otro": "var(--text-faint)", "Sin categoría": "var(--text-muted)",
};
const max = datos[0][1] || 1;
const total = datos.reduce((a, [, v]) => a + v, 0);
const wrap = dv.el("div", "", { attr: { style: "max-width:640px;font-variant-numeric:tabular-nums" } });
for (const [cat, v] of datos) {
  const row = wrap.createDiv({ attr: { style: "display:flex;align-items:center;gap:8px;margin:4px 0;font-size:.9em" } });
  row.createDiv({ text: cat, attr: { style: "flex:0 0 140px;text-align:right;color:var(--text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis" } });
  const track = row.createDiv({ attr: { style: "flex:1;min-width:0" } });
  const bar = track.createDiv({ attr: { title: `${cat}: ${dinero(v)} (${total ? Math.round((v / total) * 100) : 0}%)` } });
  bar.style.cssText = `height:16px;border-radius:0 4px 4px 0;width:${(v / max) * 100}%;background:${COLOR[cat] || "var(--text-muted)"}`;
  row.createDiv({ text: dinero(v), attr: { style: "flex:0 0 80px;font-weight:600" } });
}
wrap.createDiv({ text: `Total registrado: ${dinero(total)}`, attr: { style: "margin-top:6px;font-size:.85em;color:var(--text-muted)" } });
