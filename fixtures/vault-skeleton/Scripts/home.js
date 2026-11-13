// Métricas del Home: semana vs semana pasada y mes vs mes pasado.
// Uso: await dv.view("Scripts/home")  (las metas se leen del frontmatter de la nota que lo llama)

const M = window.moment;
const F = "YYYY-MM-DD";
const cfg = dv.current();
const META_PESO = cfg.meta_peso ?? 70;
const META_EJ = cfg.meta_ejercicio_semana ?? 4;
const MAX_FALLAS = cfg.max_fallas_devocional ?? 5;

const dinero = n => "$" + Math.round(n).toLocaleString("es-MX");
const monto = s => { const n = parseFloat(String(s).replace(/[$,\s]/g, "")); return isNaN(n) ? 0 : n; };
const pct = (a, b) => (b ? Math.round((a / b) * 100) : null);

const hoy = M().startOf("day");
const periodos = {
  semana: { ini: hoy.clone().startOf("isoWeek"), fin: hoy.clone() },
  semanaPasada: { ini: hoy.clone().startOf("isoWeek").subtract(7, "days"), fin: hoy.clone().startOf("isoWeek").subtract(1, "days") },
  mes: { ini: hoy.clone().startOf("month"), fin: hoy.clone() },
  mesPasado: { ini: hoy.clone().subtract(1, "month").startOf("month"), fin: hoy.clone().subtract(1, "month").endOf("month").startOf("day") },
};

// --- Datos ---
const dailies = dv.pages('"Daily"').where(p => /^\d{4}-\d{2}-\d{2}$/.test(p.file.name)).array();

// Gastos: tablas en Proyectos/personal/Finanzas/Gastos/Gastos YYYY-MM.md
const gastos = [];
for (const p of dv.pages('"Proyectos/personal/Finanzas"').where(p => p.file.name.startsWith("Gastos ")).array()) {
  const text = await dv.io.load(p.file.path);
  for (const line of (text || "").split("\n")) {
    const t = line.trim();
    if (!t.startsWith("|")) continue;
    const c = t.split(/(?<!\\)\|/).slice(1, -1).map(x => x.trim());
    if (c.length < 3 || !/^\d{4}-\d{2}-\d{2}$/.test(c[0])) continue;
    gastos.push({ fecha: M(c[0], F), monto: monto(c[2]), categoria: c[3] || "Otro" });
  }
}

const enRango = (d, r) => d.isSameOrAfter(r.ini) && d.isSameOrBefore(r.fin);

function stats(r) {
  const dias = r.fin.diff(r.ini, "days") + 1;
  const ds = dailies.filter(p => enRango(M(p.file.name, F), r)).sort((a, b) => a.file.name.localeCompare(b.file.name));
  const cuenta = k => ds.filter(p => p[k] === true).length;
  const habitos = sec => {
    let hechos = 0, total = 0;
    for (const p of ds) for (const t of p.file.tasks.array()) {
      if (t.section?.subpath !== sec || !t.text.trim()) continue;
      total++; if (t.completed) hechos++;
    }
    return { hechos, total };
  };
  const pesos = ds.filter(p => typeof p.peso === "number").map(p => ({ f: p.file.name, v: p.peso }));
  const g = gastos.filter(x => enRango(x.fecha, r));
  return {
    dias, conDaily: ds.length,
    devocional: cuenta("devocional"), ejercicio: cuenta("ejercicio"), lectura: cuenta("lectura"),
    hMes: habitos("Hábitos del mes"), hSem: habitos("Hábitos de la semana"),
    pesoIni: pesos[0]?.v, pesoFin: pesos[pesos.length - 1]?.v,
    gasto: g.filter(x => x.categoria !== "Pago de deuda").reduce((a, x) => a + x.monto, 0),
    deudaPagada: g.filter(x => x.categoria === "Pago de deuda").reduce((a, x) => a + x.monto, 0),
  };
}

// --- Formato ---
const flecha = (a, b, menosEsMejor = false) => {
  if (a == null || b == null) return "—";
  const d = Math.round((a - b) * 10) / 10;
  if (d === 0) return "=";
  const bueno = menosEsMejor ? d < 0 : d > 0;
  return `${bueno ? "🟢" : "🔴"} ${d > 0 ? "+" : ""}${d}`;
};
const hab = h => (h.total ? `${pct(h.hechos, h.total)}% (${h.hechos}/${h.total})` : "—");
const kg = v => (v == null ? "—" : `${v} kg`);
const bajada = s => (s.pesoIni != null && s.pesoFin != null ? Math.round((s.pesoFin - s.pesoIni) * 10) / 10 : null);

function tabla(a, b, titulos, esMes) {
  const ejMeta = esMes ? `meta ${META_EJ}/sem` : `meta ${META_EJ}`;
  const rows = [
    ["🙏 Devocional", `${a.devocional}/${a.dias}`, `${b.devocional}/${b.dias}`, flecha(a.devocional, b.devocional)],
    [`🏋️ Ejercicio (${ejMeta})`, `${a.ejercicio}`, `${b.ejercicio}`, flecha(a.ejercicio, b.ejercicio)],
    ["📖 Lectura", `${a.lectura}/${a.dias}`, `${b.lectura}/${b.dias}`, flecha(a.lectura, b.lectura)],
    ["✅ Hábitos del mes", hab(a.hMes), hab(b.hMes), flecha(pct(a.hMes.hechos, a.hMes.total), pct(b.hMes.hechos, b.hMes.total))],
    ["✅ Hábitos de la semana", hab(a.hSem), hab(b.hSem), flecha(pct(a.hSem.hechos, a.hSem.total), pct(b.hSem.hechos, b.hSem.total))],
    ["⚖️ Peso (último)", kg(a.pesoFin), kg(b.pesoFin), flecha(a.pesoFin, b.pesoFin, true)],
    ["⚖️ Cambio de peso en el periodo", bajada(a) == null ? "—" : `${bajada(a)} kg`, bajada(b) == null ? "—" : `${bajada(b)} kg`, ""],
    ["💸 Gasto (sin deudas)", dinero(a.gasto), dinero(b.gasto), flecha(a.gasto, b.gasto, true)],
    ["💳 Pagado a deudas", dinero(a.deudaPagada), dinero(b.deudaPagada), flecha(a.deudaPagada, b.deudaPagada)],
  ];
  dv.table(["Métrica", ...titulos, "Cambio"], rows);
}

// --- Render ---
const S = stats(periodos.semana), SP = stats(periodos.semanaPasada);
const Mm = stats(periodos.mes), MP = stats(periodos.mesPasado);

// Resumen rápido
const ultimoPeso = [...dailies].sort((a, b) => b.file.name.localeCompare(a.file.name)).find(p => typeof p.peso === "number")?.peso;
const fallas = Mm.dias - Mm.devocional;
const mesNota = dv.page(`Proyectos/personal/Objetivos/Mes ${hoy.format("YYYY-MM")}`);
let deudaTotal = 0, deudaPagadaObj = 0;
if (mesNota) for (const t of mesNota.file.tasks.array()) {
  const m = t.text.match(/\$\s?([\d,]+)/);
  if (!m || !/deuda/i.test(t.section?.subpath ?? "")) continue;
  const v = monto(m[1]); deudaTotal += v; if (t.completed) deudaPagadaObj += v;
}
const mesStr = hoy.format("YYYY-MM"), semStr = hoy.format("GGGG-[W]WW");
dv.paragraph([
  dv.fileLink(`Daily/${hoy.format(F)}`, false, "📅 Daily de hoy"),
  dv.fileLink(`Proyectos/personal/Objetivos/Mes ${mesStr}`, false, "🎯 Objetivos del mes"),
  dv.fileLink(`Proyectos/personal/Objetivos/Semana ${semStr}`, false, "🎯 Objetivos de la semana"),
  dv.fileLink(`Proyectos/personal/Finanzas/Gastos/Gastos ${mesStr}`, false, "💸 Gastos del mes"),
].join(" · "));
dv.header(3, "Resumen");
dv.list([
  `⚖️ **Peso:** ${kg(ultimoPeso)} · meta ${META_PESO} kg · ${ultimoPeso != null ? `faltan ${Math.max(0, Math.round((ultimoPeso - META_PESO) * 10) / 10)} kg` : "anota tu `peso` en la daily"}`,
  `🙏 **Devocional del mes:** ${fallas} fallas de ${MAX_FALLAS} permitidas ${fallas > MAX_FALLAS ? "🔴" : "🟢"}`,
  `🏋️ **Ejercicio esta semana:** ${S.ejercicio}/${META_EJ} ${S.ejercicio >= META_EJ ? "🟢" : ""}`,
  deudaTotal ? `💳 **Deudas:** ${dinero(deudaPagadaObj)} de ${dinero(deudaTotal)} liquidado (${pct(deudaPagadaObj, deudaTotal)}%)` : `💳 **Deudas:** sin datos en el objetivo del mes`,
]);

dv.header(3, `Semana ${hoy.format("GGGG-[W]WW")} vs semana pasada`);
tabla(S, SP, ["Esta semana", "Semana pasada"], false);

dv.header(3, `${hoy.clone().locale("es").format("MMMM YYYY")} vs ${hoy.clone().subtract(1, "month").locale("es").format("MMMM")}`);
tabla(Mm, MP, ["Este mes", "Mes pasado"], true);

dv.paragraph(`<small>Semana y mes actuales cuentan hasta hoy (${hoy.format(F)}). Días sin daily cuentan como no hechos.</small>`);
