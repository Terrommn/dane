// Panel de cuentas de débito. Uso: await dv.view("Scripts/cuentas")
// Cuentas: notas en Proyectos/personal/Finanzas/Cuentas/ (frontmatter banco, saldo, actualizado, estado).
// `saldo` es lo que había al EMPEZAR el día `actualizado`. Me queda = saldo + ingresos con Cuenta que menciona el banco desde ese día − gastos con Pago que lo menciona (sin tarjetas de crédito) desde ese día.
// Así cualquier gasto o ingreso que se registre ese mismo día o después se refleja solo.

const DIR = "Proyectos/personal/Finanzas";
const dinero = n => (n < 0 ? "−$" : "$") + Math.abs(n).toLocaleString("es-MX", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const monto = s => { const n = parseFloat(String(s ?? "").replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const fmt = d => d ? (typeof d === "string" ? d : d.toFormat("yyyy-MM-dd")) : "";

// Filas | Fecha | Concepto | Monto | Categoría | Pago/Cuenta | de todas las notas de un tipo
async function filas(carpeta, re) {
  const out = [];
  for (const p of dv.pages(`"${DIR}/${carpeta}"`).where(p => re.test(p.file.name))) {
    for (const l of ((await dv.io.load(p.file.path)) || "").split("\n")) {
      const c = l.trim().split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
      if (c.length >= 5 && /^\d{4}-\d{2}-\d{2}$/.test(c[0])) out.push({ fecha: c[0], monto: monto(c[2]), cuenta: c[4] });
    }
  }
  return out;
}
const gastos = await filas("Gastos", /^Gastos \d{4}-\d{2}$/);
const ingresos = await filas("Ingresos", /^Ingresos \d{4}-\d{2}$/);
const usa = (texto, banco) => !/\[\[Tarjeta /.test(texto) && texto.toLowerCase().includes(banco.toLowerCase());

const cuentas = dv.pages(`"${DIR}/Cuentas"`).where(p => p.estado !== "cerrada").sort(p => p.file.name).array();
if (!cuentas.length) { dv.paragraph("Sin cuentas todavía: crea una nota en `Finanzas/Cuentas/` con `Templates/Cuenta.md`."); return; }

let total = 0;
const tabla = cuentas.map(c => {
  const desde = fmt(c.actualizado), banco = String(c.banco || c.file.name.replace(/^Cuenta /, ""));
  const entro = ingresos.filter(r => r.fecha >= desde && usa(r.cuenta, banco)).reduce((a, r) => a + r.monto, 0);
  const salio = gastos.filter(r => r.fecha >= desde && usa(r.cuenta, banco)).reduce((a, r) => a + r.monto, 0);
  const queda = monto(c.saldo) + entro - salio;
  total += queda;
  return [c.file.link, c.saldo != null ? dinero(monto(c.saldo)) : "—", desde || "—", entro ? dinero(entro) : "—", salio ? dinero(salio) : "—", (queda < 0 ? "🔴 " : "") + dinero(queda)];
});

dv.paragraph(`**Tengo en débito:** ${dinero(total)}`);
dv.table(["Cuenta", "Saldo al iniciar el día", "Día", "Entró desde ese día", "Salió desde ese día", "Me queda"], tabla);
