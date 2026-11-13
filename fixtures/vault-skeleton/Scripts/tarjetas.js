// Panel de tarjetas de crédito. Uso: await dv.view("Scripts/tarjetas")
// Tarjetas: notas en Proyectos/personal/Finanzas/Tarjetas/ (frontmatter limite, disponible, actualizado, dia_corte, dia_pago, estado).
// Me queda = disponible − gastos pagados con la tarjeta después de `actualizado` + pagos a la tarjeta (Pago de deuda que la enlaza) después de esa fecha.

const DIR = "Proyectos/personal/Finanzas";
const dinero = n => (n < 0 ? "−$" : "$") + Math.abs(Math.round(n)).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s ?? "").replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const fmt = d => d ? (typeof d === "string" ? d : d.toFormat("yyyy-MM-dd")) : "";

// Todos los gastos registrados
const gastos = [];
for (const p of dv.pages(`"${DIR}/Gastos"`).where(p => /^Gastos \d{4}-\d{2}$/.test(p.file.name))) {
  for (const l of ((await dv.io.load(p.file.path)) || "").split("\n")) {
    const c = l.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
    if (c.length >= 5 && /^\d{4}-\d{2}-\d{2}$/.test(c[0])) gastos.push({ fecha: c[0], concepto: c[1], monto: monto(c[2]), categoria: c[3], pago: c[4] });
  }
}
const enlaza = (texto, nombre) => texto.includes(`[[${nombre}|`) || texto.includes(`[[${nombre}\\|`) || texto.includes(`[[${nombre}]]`);

const hoy = new Date();
const proximo = dia => {
  if (!dia) return null;
  const d = new Date(hoy.getFullYear(), hoy.getMonth(), Math.min(+dia, new Date(hoy.getFullYear(), hoy.getMonth() + 1, 0).getDate()));
  if (d < new Date(hoy.getFullYear(), hoy.getMonth(), hoy.getDate())) d.setMonth(d.getMonth() + 1, Math.min(+dia, new Date(d.getFullYear(), d.getMonth() + 2, 0).getDate()));
  return d;
};

const tarjetas = dv.pages(`"${DIR}/Tarjetas"`).where(p => p.estado !== "cancelada").sort(p => p.file.name).array();
if (!tarjetas.length) { dv.paragraph("Sin tarjetas todavía: crea una nota en `Finanzas/Tarjetas/` con `Templates/Tarjeta.md`."); return; }

let tLimite = 0, tQueda = 0;
const filas = tarjetas.map(t => {
  const desde = fmt(t.actualizado);
  const despues = gastos.filter(g => g.fecha > desde);
  const gastado = despues.filter(g => g.categoria !== "Pago de deuda" && enlaza(g.pago, t.file.name)).reduce((a, g) => a + g.monto, 0);
  const pagado = despues.filter(g => g.categoria === "Pago de deuda" && enlaza(g.concepto + g.pago, t.file.name)).reduce((a, g) => a + g.monto, 0);
  const limite = monto(t.limite), queda = monto(t.disponible) - gastado + pagado;
  tLimite += limite; tQueda += queda;
  const usado = limite ? Math.round(((limite - queda) / limite) * 100) : null;
  const pago = proximo(t.dia_pago);
  const dias = pago ? Math.round((pago - new Date(hoy.getFullYear(), hoy.getMonth(), hoy.getDate())) / 864e5) : null;
  return [
    t.file.link,
    limite ? dinero(limite) : "—",
    t.disponible != null ? dinero(monto(t.disponible)) : "—",
    desde || "—",
    gastado ? dinero(gastado) : "—",
    pagado ? dinero(pagado) : "—",
    (queda < 0 ? "🔴 " : "") + dinero(queda),
    usado != null ? (usado >= 80 ? "🔴 " : usado >= 50 ? "🟡 " : "🟢 ") + usado + "%" : "—",
    t.dia_corte ?? "—",
    pago ? `${t.dia_pago} · ${dias === 0 ? "**hoy**" : `en ${dias} días`}` : "—",
  ];
});

dv.paragraph(`**Me queda en total:** ${dinero(tQueda)}${tLimite ? ` de ${dinero(tLimite)} de límite · ${Math.round(((tLimite - tQueda) / tLimite) * 100)}% usado` : ""}`);
dv.table(["Tarjeta", "Límite", "Disponible", "Lo vi el", "Gasté después", "Pagué después", "Me queda", "% usado", "Corte", "Pago"], filas);
