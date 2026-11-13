// Etiquetas en español de los códigos del servidor (finanzas).

export const TX_KINDS = [
  "expense",
  "income",
  "loan_received",
  "transfer",
  "card_payment",
  "debt_payment",
  "adjustment",
] as const;
export type TxKind = (typeof TX_KINDS)[number];

export const TX_KIND_LABEL: Record<TxKind, string> = {
  expense: "Gasto",
  income: "Ingreso",
  loan_received: "Préstamo recibido",
  transfer: "Transferencia",
  card_payment: "Pago de tarjeta",
  debt_payment: "Abono a deuda",
  adjustment: "Ajuste de saldo",
};

export const TX_KIND_HINT: Record<TxKind, string> = {
  expense: "Consumo. Con tarjeta cuenta una sola vez; el pago posterior de la tarjeta no es otro gasto.",
  income: "Ingreso ganado u otro ingreso que entra a una cuenta.",
  loan_received: "Financiamiento: no es ingreso ganado; puedes registrar la deuda al mismo tiempo.",
  transfer: "Entre tus cuentas de débito/efectivo. No es gasto ni ingreso.",
  card_payment: "De una cuenta a una tarjeta: libera crédito, no es consumo.",
  debt_payment: "Abono a un préstamo o deuda familiar. No vuelve a contar como consumo.",
  adjustment: "Corrige el saldo de una cuenta (suma o resta) sin afectar consumo ni ingresos.",
};

export const TX_STATUS_LABEL: Record<string, string> = {
  confirmed: "Confirmado",
  pending_reconciliation: "Pendiente de conciliar",
  void: "Anulado",
};

export const ORIGIN_LABEL: Record<string, string> = {
  manual: "Manual",
  recurring: "Recurrente",
  import: "Importado",
  llm: "Asistente",
  bot: "Telegram",
};

export const ACCOUNT_KIND_LABEL: Record<string, string> = {
  debit: "Débito",
  cash: "Efectivo",
  savings: "Ahorro",
  credit_card: "Tarjeta de crédito",
};

export const ACCOUNT_STATUS_LABEL: Record<string, string> = {
  active: "Activa",
  to_cancel: "Por cancelar",
  cancelled: "Cancelada",
  closed: "Cerrada",
};

export const ROLE_LABEL: Record<string, string> = {
  consumption: "Consumo",
  debt_payment: "Pago de deuda",
  investment: "Inversión",
  payroll: "Nómina",
  earned: "Ingreso ganado",
  financing: "Financiamiento",
  other_income: "Otro ingreso",
};

export const EXPENSE_ROLES = ["consumption", "debt_payment", "investment", "payroll"] as const;
export const INCOME_ROLES = ["earned", "financing", "other_income"] as const;

export const SEMANTICS_LABEL: Record<string, string> = {
  instant: "Saldo en este momento",
  legacy_day_start_inclusive: "Saldo al iniciar el día (cuentas del vault)",
  legacy_after_day: "Disponible visto ese día (tarjetas del vault)",
};

export const DEBT_KIND_LABEL: Record<string, string> = {
  credit_card: "Tarjeta de crédito",
  loan: "Préstamo",
  family: "Préstamo familiar",
  other: "Otra",
};

export const DEBT_STATUS_LABEL: Record<string, string> = {
  open: "Abierta",
  paid: "Saldada",
  cancelled: "Cancelada",
};

export const RULE_KIND_LABEL: Record<string, string> = {
  subscription: "Suscripción",
  fixed_income: "Ingreso fijo",
};

export const RULE_STATUS_LABEL: Record<string, string> = {
  active: "Activa",
  cancelled: "Cancelada",
  ended: "Terminada",
};

export const OCCURRENCE_STATE_LABEL: Record<string, string> = {
  pending: "Registrada · pendiente de conciliar",
  confirmed: "Confirmada",
  void: "Anulada",
  projected: "Proyectada (aún no registrada)",
};

export function dayRuleLabel(kind: string, day?: number | null): string {
  if (kind === "last") return "Último día del mes";
  if (kind === "penultimate") return "Penúltimo día del mes";
  return `Día ${day ?? "?"}${day && day > 28 ? " (o el último si el mes es más corto)" : ""}`;
}

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

/** "2026-11" → "nov 2026". */
export function monthLabel(ym: string): string {
  const [y, m] = ym.split("-");
  return `${MONTHS[Number(m) - 1] ?? m} ${y}`;
}

/** Mes anterior/siguiente de "YYYY-MM". */
export function shiftMonth(ym: string, delta: number): string {
  const [y, m] = ym.split("-").map(Number);
  const idx = (y ?? 1970) * 12 + ((m ?? 1) - 1) + delta;
  const ny = Math.floor(idx / 12);
  const nm = (idx % 12) + 1;
  return `${ny}-${String(nm).padStart(2, "0")}`;
}
