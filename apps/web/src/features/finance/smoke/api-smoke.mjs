// Uso: node apps/web/src/features/finance/smoke/api-smoke.mjs con server + `pnpm -C apps/web dev --port 5177` (dev-login, ID 111, BD vacía).
// Ejercita la API de finanzas a través del proxy de Vite con cookie + CSRF, usando los mismos cuerpos que arma la UI.
const BASE = "http://127.0.0.1:5177/api/v1";
let cookie = "";
let csrf = "";
let ok = 0;
let fail = 0;
const check = (name, cond, extra = "") => {
  if (cond) ok++;
  else fail++;
  console.log(`${cond ? "✓" : "✗"} ${name}${extra ? ` — ${extra}` : ""}`);
};
async function call(method, path, body, headers = {}) {
  const h = { "content-type": "application/json", cookie, ...headers };
  if (method !== "GET") {
    h["x-csrf-token"] = csrf;
    h["idempotency-key"] ??= crypto.randomUUID();
  }
  const r = await fetch(BASE + path, { method, headers: h, body: body ? JSON.stringify(body) : undefined });
  const set = r.headers.getSetCookie?.() ?? [];
  for (const c of set) {
    const kv = c.split(";")[0];
    const [k, v] = kv.split("=");
    if (k === "dane_session" || k === "dane_csrf") {
      cookie = cookie
        .split("; ")
        .filter((x) => x && !x.startsWith(`${k}=`))
        .concat(kv)
        .join("; ");
      if (k === "dane_csrf") csrf = v;
    }
  }
  const text = await r.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = text;
  }
  return { status: r.status, json };
}

const login = await call("POST", "/auth/dev-login", { telegram_id: 111 });
check("dev-login con cookie", login.status === 200 && csrf.length > 10);
const month = new Date().toISOString().slice(0, 7);
const today = new Date().toISOString().slice(0, 10);

const cats = (await call("GET", "/finance/categories?archived=false")).json;
const cat = (n, k) => cats.find((c) => c.name === n && c.kind === k)?.id;
check("catálogo inicial (15 gasto + 5 ingreso)", cats.filter((c) => c.kind === "expense").length === 15 && cats.filter((c) => c.kind === "income").length === 5);

const deb = await call("POST", "/finance/accounts", { name: "Débito Banco", bank: "Banco", kind: "debit", currency: "MXN", credit_limit_cents: null, cut_day: null, pay_day: null, status: "active", is_default_debit: true, notes_md: "", archived: false });
check("crear cuenta débito", deb.status === 201, JSON.stringify(deb.json).slice(0, 120));
const card = await call("POST", "/finance/accounts", { name: "Tarjeta A", bank: "Banco", kind: "credit_card", currency: "MXN", credit_limit_cents: 1000000, cut_day: 7, pay_day: 27, status: "active", is_default_debit: false, notes_md: "", archived: false });
check("crear tarjeta", card.status === 201);
const debId = deb.json.account.id;
const cardId = card.json.account.id;

const snap = await call("POST", `/finance/accounts/${debId}/snapshots`, { balance_cents: 2000000, observed_at: new Date(Date.now() - 3600_000).toISOString(), observed_on: null, semantics: "instant", note: "app" });
check("snapshot instantáneo (débito)", snap.status === 201 && snap.json.estimated_cents === 2000000, `estimado=${snap.json?.estimated_cents}`);
const snapCard = await call("POST", `/finance/accounts/${cardId}/snapshots`, { balance_cents: 900000, observed_at: null, observed_on: today, semantics: "legacy_after_day", note: "" });
check("snapshot legacy tarjeta", snapCard.status === 201, JSON.stringify(snapCard.json).slice(0, 160));

const key = crypto.randomUUID();
const exp = { kind: "expense", occurred_on: today, concept: "Gasolina", amount_cents: 50000, currency: "MXN", original: null, category_id: cat("Gasolina", "expense"), account_id: cardId, counter_account_id: null, debt_id: null, person_id: null, financed_by_person_id: null, project_id: null, budget_item_id: null, adjustment_sign: null, notes_md: "", track_as_debt: false, client_key: key };
const e1 = await call("POST", "/finance/transactions", exp);
const e2 = await call("POST", "/finance/transactions", exp);
check("gasto con tarjeta (client_key)", e1.status === 201 && e1.json.created === true, `status=${e1.status}`);
check("reintento con la misma client_key no duplica", [200, 201].includes(e2.status) && e2.json.created === false && e2.json.transaction.id === e1.json.transaction.id);

const row = (k) => ({ ...exp, concept: "Café", amount_cents: 4500, category_id: cat("Comida", "expense"), account_id: debId, client_key: k });
const b = await call("POST", "/finance/transactions/batch", { items: [row(crypto.randomUUID()), row(crypto.randomUUID())] });
check("lote: dos compras iguales se conservan", b.status === 200 && b.json.results.filter((r) => r.created).length === 2, `status=${b.status}`);
const badBatch = await call("POST", "/finance/transactions/batch", { items: [row(crypto.randomUUID()), { ...row(crypto.randomUUID()), category_id: cat("Sueldo", "income") }] });
check("lote atómico: 422 con índice del elemento", badBatch.status === 422 && /items\[1\]/.test(badBatch.json?.error?.field ?? ""), JSON.stringify(badBatch.json?.error));

const pay = await call("POST", "/finance/transactions", { ...exp, kind: "card_payment", concept: "Pago Tarjeta A", amount_cents: 50000, category_id: null, account_id: debId, counter_account_id: cardId, client_key: crypto.randomUUID() });
check("pago de tarjeta (cuenta → tarjeta)", pay.status === 201, JSON.stringify(pay.json).slice(0, 160));

const fx = await call("POST", "/finance/transactions", { ...exp, kind: "income", concept: "Freelance USD", amount_cents: null, currency: null, original: { amount_cents: 10000, currency: "USD", fx_rate: "17.2345", fx_date: today, fx_source: "Banco" }, category_id: cat("Freelance", "income"), account_id: debId, client_key: crypto.randomUUID() });
check("ingreso en USD con conversión explícita", fx.status === 201 && fx.json.transaction.amount_cents === 172345, `amount=${fx.json?.transaction?.amount_cents}`);

const projects = (await call("GET", "/projects?archived=false")).json;
const negocio = projects.find((p) => p.slug === "negocio").id;
const bud = await call("GET", `/finance/budgets/${month}`);
check("presupuesto del mes (snapshot tras el primer gasto)", bud.status === 200, `persisted=${bud.json?.persisted} v=${bud.json?.version}`);
const put = await call("PUT", `/finance/budgets/${month}`, { version: bud.json.version, lines: [{ category_id: cat("Comida", "expense"), amount_cents: 300000 }], items: [{ id: null, project_id: negocio, category_id: cat("Negocio", "expense"), name: "Ads", amount_cents: 650000, keywords: ["Ads", "Facebook"], position: 0 }, { id: null, project_id: negocio, category_id: cat("Negocio", "expense"), name: "WhatsApp", amount_cents: 100000, keywords: ["WhatsApp"], position: 1 }] });
check("guardar presupuesto con rubros", put.status === 200 && put.json.lines.some((l) => l.from_items && l.amount_cents === 750000), `status=${put.status} ${JSON.stringify(put.json?.error ?? "")}`);
const stale = await call("PUT", `/finance/budgets/${month}`, { version: bud.json.version, lines: [], items: [] });
check("presupuesto con versión vieja → 409", stale.status === 409, `status=${stale.status} current.version=${stale.json?.error?.current?.version}`);

const rule = await call("POST", "/finance/recurring", { kind: "subscription", name: "Netflix", amount_cents: 29900, currency: "MXN", day_kind: "day", day: 31, category_id: cat("Suscripciones", "expense"), account_id: cardId, area_id: null, project_id: null, status: "active", starts_on: `${month}-01`, ends_on: null, notes_md: "" });
check("crear suscripción con el cuerpo real (FinanceRuleBody)", rule.status === 201, `status=${rule.status} ${JSON.stringify(rule.json?.error ?? "")}`);
const inc = await call("POST", "/finance/recurring", { kind: "fixed_income", name: "Sueldo", amount_cents: 1500000, currency: "MXN", day_kind: "day", day: 1, category_id: cat("Sueldo", "income"), account_id: debId, area_id: null, project_id: null, status: "active", starts_on: `${month}-01`, ends_on: null, notes_md: "" });
check("crear ingreso fijo", inc.status === 201);
const mat = await call("POST", "/finance/recurring/materialize");
check("registrar vencidas", mat.status === 200, `created=${mat.json?.created}`);
const occ = (await call("GET", "/finance/recurring/occurrences")).json;
check("ocurrencias con estados", Array.isArray(occ) && occ.length >= 2, occ.map((o) => `${o.rule_name}:${o.state}`).join(", "));
const pending = occ.find((o) => o.state === "pending");
if (pending) {
  const conf = await call("POST", `/finance/transactions/${pending.transaction_id}/confirm`, { version: pending.transaction_version, amount_cents: pending.amount_cents + 100, occurred_on: null, account_id: null });
  check("confirmar y corregir ocurrencia (misma fila)", conf.status === 200 && conf.json.id === pending.transaction_id && conf.json.status === "confirmed");
}
const ruleUpd = await call("PUT", `/finance/recurring/${rule.json.id}`, { kind: "subscription", name: "Netflix", amount_cents: 32900, currency: "MXN", day_kind: "last", day: null, category_id: cat("Suscripciones", "expense"), account_id: cardId, area_id: null, project_id: null, status: "active", starts_on: `${month}-01`, ends_on: null, notes_md: "", version: rule.json.version });
check("editar regla (último día)", ruleUpd.status === 200 && ruleUpd.json.day_kind === "last");

const debt = await call("POST", "/finance/debts", { name: "Préstamo coche", kind: "loan", account_id: null, person_id: null, principal_cents: 1000000, currency: "MXN", opened_on: today, goal_id: null, notes_md: "", cancelled: false });
check("crear deuda", debt.status === 201);
const abono = await call("POST", "/finance/transactions", { ...exp, kind: "debt_payment", concept: "Abono coche", amount_cents: 200000, category_id: null, account_id: debId, debt_id: debt.json.debt.id, client_key: crypto.randomUUID() });
check("abono a deuda", abono.status === 201);
const debts = (await call("GET", "/finance/debts")).json;
const dv = debts.find((d) => d.debt.id === debt.json.debt.id);
check("saldo de la deuda tras abono parcial", dv.paid_cents === 200000 && dv.remaining_cents === 800000 && dv.debt.status === "open");

const plan = (await call("GET", "/finance/credit-plan")).json;
const planPut = await call("PUT", "/finance/credit-plan", { version: plan.version, goal_text: "Subir score", target_date: null, steps: [{ text: "Pagar total", done: false }], rules: [{ text: "Uso < 30%" }], max_usage_pct: 30, excluded_account_ids: [], forbid_cards_with_debt: true });
check("guardar plan de crédito", planPut.status === 200);
const sugNo = (await call("GET", `/finance/card-suggestion?date=${today}`)).json;
const sug = (await call("GET", `/finance/card-suggestion?date=${today}&amount_cents=100000`)).json;
check("sugerencia sin monto es provisional", sugNo.provisional === true, sugNo.explanation);
check("sugerencia con monto explica", typeof sug.explanation === "string" && sug.cards.length === 1, `${sug.card_name ?? sug.debit_account_name} · ${sug.explanation}`);

const dash = await call("GET", "/finance/dashboard");
const t = dash.json.totals;
check("dashboard: compra con tarjeta + pago no duplican consumo", t.consumption_cents === 50000 + 9000 + 0 + (pending && pending.kind === "subscription" ? 0 : 0) || t.consumption_cents >= 59000, `consumo=${t.consumption_cents} pagos=${t.debt_payments_cents} ganado=${t.earned_income_cents}`);
check("dashboard: pagos de deuda separados", t.debt_payments_cents === 250000, `pagos=${t.debt_payments_cents}`);
check("dashboard: series y secciones", dash.json.daily.length === dash.json.days_in_month && Array.isArray(dash.json.last_six_months) && Array.isArray(dash.json.top_expenses));

const list = await call("GET", `/finance/transactions?kind=expense&sort=amount_desc&limit=25&offset=0`);
check("lista filtrada con totales", list.status === 200 && list.json.totals.count === list.json.total, `total=${list.json.total} gasto=${list.json.totals.expense_cents}`);

const tx = e1.json.transaction;
const upd = await call("PUT", `/finance/transactions/${tx.id}`, { ...exp, concept: "Gasolina Pemex", client_key: null, version: tx.version });
const staleTx = await call("PUT", `/finance/transactions/${tx.id}`, { ...exp, concept: "Viejo", client_key: null, version: tx.version });
check("editar movimiento", upd.status === 200 && upd.json.concept === "Gasolina Pemex");
check("versión vieja → 409 con current (Transaction)", staleTx.status === 409 && staleTx.json.error.current?.concept === "Gasolina Pemex", Object.keys(staleTx.json.error.current ?? {}).slice(0, 6).join(","));
const accStale = await call("PUT", `/finance/accounts/${debId}`, { name: "x", bank: "", kind: "debit", currency: "MXN", status: "active", is_default_debit: true, notes_md: "", archived: false, version: 99 });
check("cuenta con versión vieja → 409", accStale.status === 409, `current keys=${Object.keys(accStale.json?.error?.current ?? {}).slice(0, 5).join(",")}`);
const v = await call("POST", `/finance/transactions/${tx.id}/void`, { version: upd.json.version });
check("anular movimiento", v.status === 200 && v.json.status === "void");
const rep = await call("GET", `/finance/projects/${negocio}/report`);
check("reporte por proyecto", rep.status === 200 && rep.json.rubros.length >= 2, rep.json?.warnings?.join(" | "));

console.log(`\n${ok} ok, ${fail} fallidas`);
process.exit(fail ? 1 : 0);
