// Formulario de movimiento → `TxBody` del servidor, con las mismas reglas por tipo que valida el backend
// (`apps/server/src/api/finance/transactions.rs`), para dar errores inmediatos y comprensibles. El servidor sigue
// siendo la autoridad: sus 422 se muestran igual.
import type { Schemas } from "../../lib/api";
import type { TxKind } from "./labels";
import { parseMoney, validRate } from "./money";

export type TxBody = Schemas["TxBody"];

export interface TxForm {
  kind: TxKind;
  occurred_on: string;
  concept: string;
  amount: string;
  /** Moneda del monto cuando no hay conversión (por defecto la base). */
  currency: string;
  useFx: boolean;
  fx_currency: string;
  fx_amount: string;
  fx_rate: string;
  fx_date: string;
  fx_source: string;
  category_id: string;
  account_id: string;
  counter_account_id: string;
  debt_id: string;
  person_id: string;
  /** Gasto pagado por otra persona (préstamo familiar). */
  financed: boolean;
  financed_by_person_id: string;
  project_id: string;
  budget_item_id: string;
  adjustment_sign: "1" | "-1";
  notes_md: string;
  track_as_debt: boolean;
}

export interface AccountLite {
  id: string;
  kind: string;
  currency: string;
}
export interface CategoryLite {
  id: string;
  kind: string;
}

export function emptyTxForm(date: string, baseCurrency = "MXN", kind: TxKind = "expense"): TxForm {
  return {
    kind,
    occurred_on: date,
    concept: "",
    amount: "",
    currency: baseCurrency,
    useFx: false,
    fx_currency: "USD",
    fx_amount: "",
    fx_rate: "",
    fx_date: date,
    fx_source: "",
    category_id: "",
    account_id: "",
    counter_account_id: "",
    debt_id: "",
    person_id: "",
    financed: false,
    financed_by_person_id: "",
    project_id: "",
    budget_item_id: "",
    adjustment_sign: "-1",
    notes_md: "",
    track_as_debt: false,
  };
}

export type BuildResult =
  | { ok: true; body: TxBody }
  | { ok: false; errors: Partial<Record<keyof TxForm, string>> };

const opt = (s: string) => (s.trim() ? s : null);

export function buildTxBody(
  f: TxForm,
  ctx: { accounts: AccountLite[]; categories: CategoryLite[]; baseCurrency: string },
): BuildResult {
  const errors: Partial<Record<keyof TxForm, string>> = {};
  const acc = ctx.accounts.find((a) => a.id === f.account_id);
  const counter = ctx.accounts.find((a) => a.id === f.counter_account_id);
  const cat = ctx.categories.find((c) => c.id === f.category_id);
  const isCard = (a?: AccountLite) => a?.kind === "credit_card";
  const isCash = (a?: AccountLite) => !!a && a.kind !== "credit_card";

  if (!f.concept.trim()) errors.concept = "Describe el movimiento";
  else if (f.concept.trim().length > 200) errors.concept = "Máximo 200 caracteres";
  if (!/^\d{4}-\d{2}-\d{2}$/.test(f.occurred_on)) errors.occurred_on = "Fecha inválida";

  let amount_cents: number | null = null;
  let original: Schemas["FxBody"] | null = null;
  if (f.useFx) {
    const cur = f.fx_currency.trim().toUpperCase();
    if (!/^[A-Z]{3}$/.test(cur)) errors.fx_currency = "Moneda ISO de 3 letras (USD, EUR…)";
    else if (cur === ctx.baseCurrency) errors.fx_currency = "La moneda original es tu moneda base";
    const orig = parseMoney(f.fx_amount);
    if (!orig.ok) errors.fx_amount = orig.error;
    const rate = validRate(f.fx_rate);
    if (!rate)
      errors.fx_rate = `Tasa explícita positiva (${ctx.baseCurrency} por 1 ${cur || "unidad"}); no se inventan cotizaciones`;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(f.fx_date)) errors.fx_date = "Fecha de la tasa";
    if (!f.fx_source.trim()) errors.fx_source = "Indica la fuente de la tasa (banco, app, Banxico…)";
    if (orig.ok && rate) {
      original = {
        amount_cents: orig.cents,
        currency: cur,
        fx_rate: rate,
        fx_date: f.fx_date,
        fx_source: f.fx_source.trim(),
      };
    }
  } else {
    const p = parseMoney(f.amount);
    if (!p.ok) errors.amount = p.error;
    else amount_cents = p.cents;
    const cur = f.currency.trim().toUpperCase();
    if (!/^[A-Z]{3}$/.test(cur)) errors.currency = "Moneda ISO de 3 letras";
  }
  const movementCurrency = f.useFx ? ctx.baseCurrency : f.currency.trim().toUpperCase();
  const accountForCurrency = acc ?? counter;
  if (accountForCurrency && accountForCurrency.currency !== movementCurrency && !errors.currency) {
    errors.account_id = `La cuenta está en ${accountForCurrency.currency}; usa conversión explícita`;
  }

  switch (f.kind) {
    case "expense":
      if (cat?.kind !== "expense") errors.category_id = "Elige una categoría de gasto";
      if (f.financed) {
        if (!f.financed_by_person_id) errors.financed_by_person_id = "¿Quién lo pagó?";
        if (f.account_id) errors.account_id = "Un gasto pagado por otra persona no sale de tus cuentas";
      }
      break;
    case "income":
      if (cat?.kind !== "income") errors.category_id = "Elige una categoría de ingreso";
      if (isCard(acc)) errors.account_id = "Un ingreso entra a una cuenta, no a una tarjeta";
      break;
    case "loan_received":
      if (cat && cat.kind !== "income") errors.category_id = "Categoría de ingreso (financiamiento)";
      break;
    case "transfer":
      if (!isCash(acc)) errors.account_id = "Cuenta de origen (débito/efectivo)";
      if (!isCash(counter)) errors.counter_account_id = "Cuenta destino (débito/efectivo)";
      if (acc && counter && acc.id === counter.id)
        errors.counter_account_id = "Origen y destino deben ser distintos";
      break;
    case "card_payment":
      if (!isCash(acc)) errors.account_id = "Cuenta de la que sale el pago";
      if (!isCard(counter)) errors.counter_account_id = "Tarjeta que recibe el pago";
      break;
    case "debt_payment":
      if (!f.debt_id) errors.debt_id = "Indica la deuda que abonas";
      break;
    case "adjustment":
      if (!acc) errors.account_id = "Elige la cuenta a ajustar";
      break;
  }

  if (Object.keys(errors).length > 0) return { ok: false, errors };

  const usesCounter = f.kind === "transfer" || f.kind === "card_payment";
  const body: TxBody = {
    kind: f.kind,
    occurred_on: f.occurred_on,
    concept: f.concept.trim(),
    amount_cents: f.useFx ? null : amount_cents,
    currency: f.useFx ? null : movementCurrency,
    original,
    category_id:
      f.kind === "transfer" || f.kind === "card_payment" || f.kind === "adjustment"
        ? null
        : opt(f.category_id),
    account_id: f.kind === "expense" && f.financed ? null : opt(f.account_id),
    counter_account_id: usesCounter ? opt(f.counter_account_id) : null,
    debt_id: f.kind === "debt_payment" ? opt(f.debt_id) : null,
    person_id: opt(f.person_id),
    financed_by_person_id: f.kind === "expense" && f.financed ? opt(f.financed_by_person_id) : null,
    project_id: opt(f.project_id),
    budget_item_id: f.kind === "expense" ? opt(f.budget_item_id) : null,
    adjustment_sign: f.kind === "adjustment" ? Number(f.adjustment_sign) : null,
    notes_md: f.notes_md,
    track_as_debt: f.kind === "loan_received" ? f.track_as_debt : false,
  };
  return { ok: true, body };
}

/** Transacción del servidor → formulario editable (para editar). */
export function txToForm(t: Schemas["Transaction"]): TxForm {
  const base = emptyTxForm(t.occurred_on, t.currency, t.kind as TxKind);
  const hasFx = !!t.original_currency;
  return {
    ...base,
    concept: t.concept,
    amount: hasFx ? "" : centsText(t.amount_cents),
    currency: t.currency,
    useFx: hasFx,
    fx_currency: t.original_currency ?? "USD",
    fx_amount: hasFx ? centsText(t.original_amount_cents ?? 0) : "",
    fx_rate: t.fx_rate ?? "",
    fx_date: t.fx_date ?? t.occurred_on,
    fx_source: t.fx_source ?? "",
    category_id: t.category_id ?? "",
    account_id: t.account_id ?? "",
    counter_account_id: t.counter_account_id ?? "",
    debt_id: t.debt_id ?? "",
    person_id: t.person_id ?? "",
    financed: !!t.financed_by_person_id,
    financed_by_person_id: t.financed_by_person_id ?? "",
    project_id: t.project_id ?? "",
    budget_item_id: t.budget_item_id ?? "",
    adjustment_sign: t.adjustment_sign === 1 ? "1" : "-1",
    notes_md: t.notes_md,
  };
}

function centsText(c: number): string {
  const d = String(Math.abs(Math.trunc(c))).padStart(3, "0");
  return `${d.slice(0, -2)}.${d.slice(-2)}`;
}

/** Signo con el que un movimiento afecta el "flujo" visible en la lista (presentación). */
export function flowSign(kind: string, adjustmentSign?: number | null): -1 | 0 | 1 {
  if (kind === "income" || kind === "loan_received") return 1;
  if (kind === "transfer") return 0;
  if (kind === "adjustment") return adjustmentSign === 1 ? 1 : -1;
  return -1;
}
