// Dinero exacto: todo se maneja en centavos enteros y las conversiones texto ↔ centavos se hacen con aritmética de
// cadenas (nunca `x / 100` ni `parseFloat`), igual que `domain::money` en el servidor.

const SYMBOL: Record<string, string> = { MXN: "$", USD: "US$", EUR: "€" };

function group(int: string): string {
  return int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Centavos → "$1,250.50" (es-MX). Negativos con signo menos tipográfico: "−$5.00". */
export function formatCents(cents: number | null | undefined, currency = "MXN"): string {
  if (cents == null || !Number.isFinite(cents)) return "—";
  const neg = cents < 0;
  const digits = String(Math.abs(Math.trunc(cents))).padStart(3, "0");
  const int = digits.slice(0, -2);
  const frac = digits.slice(-2);
  const sym = SYMBOL[currency] ?? `${currency} `;
  return `${neg ? "−" : ""}${sym}${group(int)}.${frac}`;
}

/** Versión compacta para ejes de gráficas: "$12.5k", "$1.2M" (solo presentación). */
export function formatCentsShort(cents: number): string {
  const units = Math.trunc(cents / 100);
  const abs = Math.abs(units);
  const sign = units < 0 ? "−" : "";
  if (abs >= 1_000_000) return `${sign}$${(abs / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${sign}$${(abs / 1_000).toFixed(1)}k`;
  return `${sign}$${abs}`;
}

export type ParseResult = { ok: true; cents: number } | { ok: false; error: string };

/**
 * Texto del usuario → centavos exactos. Acepta "1250", "1,250.5", "$ 1 250.50". Rechaza negativos, más de dos
 * decimales, letras y montos fuera del rango seguro. `allowZero` para presupuestos.
 */
export function parseMoney(input: string, { allowZero = false } = {}): ParseResult {
  const cleaned = input.replace(/[$\s ,]/g, "").replace(/^MXN|MXN$/i, "");
  if (cleaned === "") return { ok: false, error: "Escribe un monto" };
  if (cleaned.startsWith("-")) return { ok: false, error: "El monto no puede ser negativo" };
  const m = /^(\d+)(?:\.(\d*))?$/.exec(cleaned);
  if (!m) return { ok: false, error: "Monto inválido (usa punto para decimales)" };
  const intPart = (m[1] ?? "0").replace(/^0+(?=\d)/, "");
  const frac = m[2] ?? "";
  if (frac.length > 2) return { ok: false, error: "Máximo dos decimales" };
  const centsStr = `${intPart}${frac.padEnd(2, "0")}`;
  if (centsStr.length > 15) return { ok: false, error: "Monto demasiado grande" };
  const cents = Number(centsStr);
  if (!Number.isSafeInteger(cents)) return { ok: false, error: "Monto demasiado grande" };
  if (cents === 0 && !allowZero) return { ok: false, error: "El monto debe ser mayor que cero" };
  return { ok: true, cents };
}

/** Centavos → texto editable "1250.50" (sin símbolo ni separadores de miles). */
export function centsToInput(cents: number | null | undefined): string {
  if (cents == null) return "";
  const digits = String(Math.abs(Math.trunc(cents))).padStart(3, "0");
  return `${cents < 0 ? "-" : ""}${digits.slice(0, -2)}.${digits.slice(-2)}`;
}

/** Valida una tasa de cambio como texto decimal positivo ("17.2345"); se envía tal cual (sin floats). */
export function validRate(input: string): string | null {
  const t = input.trim();
  if (!/^\d+(\.\d{1,8})?$/.test(t)) return null;
  if (/^0+(\.0+)?$/.test(t)) return null;
  return t;
}

/** Porcentaje entero redondeado de dos enteros (presentación). */
export function pct(part: number, whole: number): number | null {
  if (!whole) return null;
  return Math.round((part * 100) / whole);
}
