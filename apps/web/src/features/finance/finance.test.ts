import { describe, expect, it } from "vitest";
import { buildBatch, newRow } from "./BatchCaptureDialog";
import { buildBudgetBody } from "./BudgetPage";
import { monthRange, movementsLink } from "./FinanceDashboardPage";
import { dayRuleLabel, monthLabel, shiftMonth } from "./labels";
import { centsToInput, formatCents, parseMoney, pct, validRate } from "./money";
import { filtersFromParams } from "./TransactionsPage";
import { buildTxBody, emptyTxForm, flowSign, type TxForm, txToForm } from "./txForm";

describe("dinero exacto", () => {
  it("convierte texto a centavos sin floats", () => {
    expect(parseMoney("1,250.5")).toEqual({ ok: true, cents: 125050 });
    expect(parseMoney("$ 1 250.50")).toEqual({ ok: true, cents: 125050 });
    expect(parseMoney("0.1")).toEqual({ ok: true, cents: 10 });
    expect(parseMoney("19.99")).toEqual({ ok: true, cents: 1999 });
    // 0.1 + 0.2 sumados en centavos son exactos (con floats sería 0.30000000000000004).
    const a = parseMoney("0.1");
    const b = parseMoney("0.2");
    expect(a.ok && b.ok && a.cents + b.cents).toBe(30);
    expect(parseMoney("00012")).toEqual({ ok: true, cents: 1200 });
  });

  it("rechaza montos inválidos con mensajes comprensibles", () => {
    expect(parseMoney("1.999")).toMatchObject({ ok: false, error: "Máximo dos decimales" });
    expect(parseMoney("-5")).toMatchObject({ ok: false });
    expect(parseMoney("abc")).toMatchObject({ ok: false });
    expect(parseMoney("")).toMatchObject({ ok: false });
    expect(parseMoney("0")).toMatchObject({ ok: false, error: "El monto debe ser mayor que cero" });
    expect(parseMoney("0", { allowZero: true })).toEqual({ ok: true, cents: 0 });
    expect(parseMoney("99999999999999999")).toMatchObject({ ok: false, error: "Monto demasiado grande" });
  });

  it("formatea en es-MX desde centavos con aritmética de cadenas", () => {
    expect(formatCents(125050)).toBe("$1,250.50");
    expect(formatCents(5)).toBe("$0.05");
    expect(formatCents(-5)).toBe("−$0.05");
    expect(formatCents(100_000_000)).toBe("$1,000,000.00");
    expect(formatCents(9_007_199_254_740_991)).toBe("$90,071,992,547,409.91");
    expect(formatCents(1234, "USD")).toBe("US$12.34");
    expect(formatCents(null)).toBe("—");
    expect(centsToInput(125050)).toBe("1250.50");
    expect(centsToInput(7)).toBe("0.07");
  });

  it("valida tasas explícitas y porcentajes", () => {
    expect(validRate("17.2345")).toBe("17.2345");
    expect(validRate("0")).toBeNull();
    expect(validRate("-1")).toBeNull();
    expect(validRate("abc")).toBeNull();
    expect(pct(1, 3)).toBe(33);
    expect(pct(1, 0)).toBeNull();
  });
});

const ctx = {
  baseCurrency: "MXN",
  accounts: [
    { id: "deb", kind: "debit", currency: "MXN" },
    { id: "deb2", kind: "cash", currency: "MXN" },
    { id: "card", kind: "credit_card", currency: "MXN" },
    { id: "usd", kind: "debit", currency: "USD" },
  ],
  categories: [
    { id: "food", kind: "expense" },
    { id: "salary", kind: "income" },
  ],
};
const form = (p: Partial<TxForm>): TxForm => ({
  ...emptyTxForm("2026-11-12"),
  concept: "x",
  amount: "100",
  ...p,
});

describe("movimientos por tipo (mismas reglas que el servidor)", () => {
  it("gasto con tarjeta: consumo con cuenta de la tarjeta y centavos exactos", () => {
    const r = buildTxBody(form({ category_id: "food", account_id: "card", amount: "250.50" }), ctx);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.body).toMatchObject({
        kind: "expense",
        amount_cents: 25050,
        account_id: "card",
        currency: "MXN",
      });
      expect(r.body.counter_account_id).toBeNull();
    }
  });

  it("gasto exige categoría de gasto", () => {
    const r = buildTxBody(form({ category_id: "salary" }), ctx);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.errors.category_id).toBeTruthy();
  });

  it("préstamo familiar: lo pagó otra persona, sin cuenta propia", () => {
    const bad = buildTxBody(form({ category_id: "food", financed: true, account_id: "deb" }), ctx);
    expect(bad.ok).toBe(false);
    const ok = buildTxBody(form({ category_id: "food", financed: true, financed_by_person_id: "p1" }), ctx);
    expect(ok.ok && ok.body.financed_by_person_id).toBe("p1");
    expect(ok.ok && ok.body.account_id).toBeNull();
  });

  it("pago de tarjeta: de cuenta a tarjeta; transferencia solo entre cuentas de efectivo", () => {
    expect(
      buildTxBody(form({ kind: "card_payment", account_id: "deb", counter_account_id: "card" }), ctx).ok,
    ).toBe(true);
    expect(
      buildTxBody(form({ kind: "card_payment", account_id: "card", counter_account_id: "deb" }), ctx).ok,
    ).toBe(false);
    const t = buildTxBody(
      form({ kind: "transfer", account_id: "deb", counter_account_id: "deb2", category_id: "food" }),
      ctx,
    );
    expect(t.ok && t.body.category_id).toBeNull();
    expect(
      buildTxBody(form({ kind: "transfer", account_id: "deb", counter_account_id: "card" }), ctx).ok,
    ).toBe(false);
    expect(
      buildTxBody(form({ kind: "transfer", account_id: "deb", counter_account_id: "deb" }), ctx).ok,
    ).toBe(false);
  });

  it("ingreso no entra a una tarjeta; abono exige deuda; ajuste exige cuenta y signo", () => {
    expect(buildTxBody(form({ kind: "income", category_id: "salary", account_id: "card" }), ctx).ok).toBe(
      false,
    );
    expect(buildTxBody(form({ kind: "debt_payment" }), ctx).ok).toBe(false);
    expect(buildTxBody(form({ kind: "debt_payment", debt_id: "d1", account_id: "deb" }), ctx).ok).toBe(true);
    expect(buildTxBody(form({ kind: "adjustment" }), ctx).ok).toBe(false);
    const adj = buildTxBody(form({ kind: "adjustment", account_id: "deb", adjustment_sign: "1" }), ctx);
    expect(adj.ok && adj.body.adjustment_sign).toBe(1);
  });

  it("moneda extranjera: conversión explícita obligatoria con tasa, fecha y fuente", () => {
    const sinFuente = buildTxBody(
      form({
        kind: "income",
        category_id: "salary",
        useFx: true,
        fx_currency: "USD",
        fx_amount: "100",
        fx_rate: "17.2345",
      }),
      ctx,
    );
    expect(sinFuente.ok).toBe(false);
    if (!sinFuente.ok) expect(sinFuente.errors.fx_source).toBeTruthy();
    const ok = buildTxBody(
      form({
        kind: "income",
        category_id: "salary",
        useFx: true,
        fx_currency: "usd",
        fx_amount: "100",
        fx_rate: "17.2345",
        fx_source: "Banco",
        account_id: "deb",
      }),
      ctx,
    );
    expect(ok.ok).toBe(true);
    if (ok.ok) {
      expect(ok.body.amount_cents).toBeNull();
      expect(ok.body.original).toEqual({
        amount_cents: 10000,
        currency: "USD",
        fx_rate: "17.2345",
        fx_date: "2026-11-12",
        fx_source: "Banco",
      });
    }
    // Monto en dólares sin conversión hacia una cuenta en pesos: se exige conversión.
    const mismatch = buildTxBody(
      form({ kind: "expense", category_id: "food", currency: "USD", account_id: "deb" }),
      ctx,
    );
    expect(mismatch.ok).toBe(false);
  });

  it("préstamo recibido puede registrar la deuda; otros tipos no", () => {
    const l = buildTxBody(form({ kind: "loan_received", track_as_debt: true, account_id: "deb" }), ctx);
    expect(l.ok && l.body.track_as_debt).toBe(true);
    const e = buildTxBody(form({ category_id: "food", track_as_debt: true }), ctx);
    expect(e.ok && e.body.track_as_debt).toBe(false);
  });

  it("editar: la transacción del servidor vuelve al formulario sin perder centavos", () => {
    const f = txToForm({
      id: "t",
      kind: "income",
      status: "confirmed",
      origin: "manual",
      occurred_on: "2026-11-01",
      concept: "Sueldo",
      amount_cents: 1723450,
      currency: "MXN",
      original_amount_cents: 100000,
      original_currency: "USD",
      fx_rate: "17.2345",
      fx_date: "2026-11-01",
      fx_source: "Banco",
      notes_md: "",
      version: 3,
      created_at: "",
      updated_at: "",
    });
    expect(f).toMatchObject({ useFx: true, fx_amount: "1000.00", fx_currency: "USD", kind: "income" });
  });

  it("signo de flujo por tipo", () => {
    expect(flowSign("expense")).toBe(-1);
    expect(flowSign("income")).toBe(1);
    expect(flowSign("transfer")).toBe(0);
    expect(flowSign("adjustment", 1)).toBe(1);
  });
});

describe("captura múltiple", () => {
  it("dos compras idénticas conservan claves distintas; filas vacías se ignoran", () => {
    const a = { ...newRow("2026-11-12"), concept: "Café", amount: "45", category_id: "food" };
    const b = { ...newRow("2026-11-12"), concept: "Café", amount: "45", category_id: "food" };
    const empty = newRow("2026-11-12");
    const { bodies, errors } = buildBatch([a, empty, b], ctx);
    expect(errors).toEqual({});
    expect(bodies).toHaveLength(2);
    expect(bodies[0]?.client_key).not.toBe(bodies[1]?.client_key);
    expect(bodies[0]?.amount_cents).toBe(4500);
  });

  it("reporta el error en la fila correcta", () => {
    const ok = { ...newRow("2026-11-12"), concept: "Gas", amount: "500", category_id: "food" };
    const bad = { ...newRow("2026-11-12"), concept: "Gas", amount: "5.555", category_id: "food" };
    const { errors } = buildBatch([ok, bad], ctx);
    expect(Object.keys(errors)).toEqual(["1"]);
  });
});

describe("presupuesto", () => {
  const item = (p: Partial<Parameters<typeof buildBudgetBody>[2][number]>) => ({
    key: "k",
    id: null,
    project_id: "neg",
    category_id: "negocio",
    name: "Ads",
    amount: "6500",
    keywords: "Ads, anuncio , Facebook",
    ...p,
  });

  it("arma líneas y rubros en centavos con keywords limpias", () => {
    const r = buildBudgetBody(2, { food: "3,000", gas: "", zero: "0" }, [item({})]);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.body.version).toBe(2);
      expect(r.body.lines).toEqual([
        { category_id: "food", amount_cents: 300000 },
        { category_id: "zero", amount_cents: 0 },
      ]);
      expect(r.body.items?.[0]).toMatchObject({
        amount_cents: 650000,
        keywords: ["Ads", "anuncio", "Facebook"],
      });
    }
  });

  it("rechaza rubros repetidos en el mismo proyecto y montos inválidos", () => {
    const r = buildBudgetBody(1, { food: "abc" }, [item({ key: "a" }), item({ key: "b", name: "ads" })]);
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.errors["line:food"]).toBeTruthy();
      expect(r.errors["item:b"]).toBe("Rubro repetido en el proyecto");
    }
  });
});

describe("navegación y fechas", () => {
  it("filtros de movimientos desde la URL (enlaces del dashboard)", () => {
    const f = filtersFromParams(
      new URLSearchParams("from=2026-11-01&to=2026-11-30&category_id=c1&offset=25&x=1"),
    );
    expect(f).toEqual({ from: "2026-11-01", to: "2026-11-30", category_id: "c1", limit: 25, offset: 25 });
    expect(filtersFromParams(new URLSearchParams("offset=-3")).offset).toBe(0);
  });

  it("enlace a movimientos fuente de un mes", () => {
    expect(monthRange("2026-02", 28)).toEqual({ from: "2026-02-01", to: "2026-02-28" });
    expect(movementsLink("2026-11", 30, { kind: "expense" })).toBe(
      "/finanzas/movimientos?from=2026-11-01&to=2026-11-30&kind=expense",
    );
  });

  it("meses y reglas de día", () => {
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2027-01", -1)).toBe("2026-12");
    expect(monthLabel("2026-11")).toBe("nov 2026");
    expect(dayRuleLabel("penultimate")).toBe("Penúltimo día del mes");
    expect(dayRuleLabel("day", 31)).toContain("último");
  });
});
