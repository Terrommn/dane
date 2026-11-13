// Conversión de entradas del usuario a valores exactos. Nada de floats para dinero: se trabaja con texto → centavos.

export type Parsed<T> = { ok: true; value: T } | { ok: false; error: string };

/**
 * "1,250.50" | "$1250" | "0.5" → centavos (entero). Vacío → null (sin dato, distinto de 0).
 * Rechaza negativos y más de 2 decimales.
 */
export function parsePesosToCents(input: string): Parsed<number | null> {
  const s = input
    .trim()
    .replace(/[$\s ]/g, "")
    .replace(/,/g, "");
  if (s === "") return { ok: true, value: null };
  if (!/^\d+(\.\d{0,2})?$/.test(s))
    return { ok: false, error: "Monto inválido (máximo 2 decimales, sin negativos)" };
  const [int = "0", frac = ""] = s.split(".");
  const cents = Number(int) * 100 + Number(frac.padEnd(2, "0"));
  if (!Number.isSafeInteger(cents)) return { ok: false, error: "Monto demasiado grande" };
  return { ok: true, value: cents };
}

/** Centavos → texto editable "1250.50" (sin símbolo). */
export function centsToInput(cents: number | null | undefined): string {
  if (cents === null || cents === undefined) return "";
  const neg = cents < 0;
  const abs = Math.abs(cents);
  const s = `${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
  return neg ? `-${s}` : s;
}

/**
 * Peso en kg como decimal exacto en texto ("72.4"). Vacío → null (sin registrar, nunca 0).
 * Acepta coma decimal. Rango (0, 500) con máximo 2 decimales (igual que el servidor).
 */
export function parseWeight(input: string): Parsed<string | null> {
  const s = input.trim().replace(",", ".");
  if (s === "") return { ok: true, value: null };
  if (!/^\d{1,3}(\.\d{1,2})?$/.test(s))
    return { ok: false, error: "Peso inválido (ej. 72.4, máximo 2 decimales)" };
  const n = Number(s);
  if (!(n > 0 && n < 500)) return { ok: false, error: "El peso debe estar entre 0 y 500 kg" };
  return { ok: true, value: normalizeDecimal(s) };
}

/** Cantidad positiva opcional (minutos, páginas, libros) en texto decimal. */
export function parseQuantity(input: string, field = "Cantidad"): Parsed<string | null> {
  const s = input.trim().replace(",", ".");
  if (s === "") return { ok: true, value: null };
  if (!/^\d+(\.\d{1,3})?$/.test(s)) return { ok: false, error: `${field} inválida` };
  return { ok: true, value: normalizeDecimal(s) };
}

/** "072.40" → "72.4"; "5.0" → "5". */
export function normalizeDecimal(s: string): string {
  const [i = "0", f = ""] = s.split(".");
  const int = i.replace(/^0+(?=\d)/, "");
  const frac = f.replace(/0+$/, "");
  return frac ? `${int}.${frac}` : int;
}

/** Decimal del servidor ("72.40") → número para mostrar/gráficas (no para cálculos de dinero). */
export function decimalToNumber(s: string | number | null | undefined): number | null {
  if (s === null || s === undefined || s === "") return null;
  const n = typeof s === "number" ? s : Number(s);
  return Number.isFinite(n) ? n : null;
}

/** Número con hasta `max` decimales en es-MX. */
export function formatNumber(n: number | null | undefined, max = 1): string {
  if (n === null || n === undefined || !Number.isFinite(n)) return "—";
  return new Intl.NumberFormat("es-MX", { maximumFractionDigits: max }).format(n);
}

export function formatPct(rate: number | null | undefined): string {
  if (rate === null || rate === undefined || !Number.isFinite(rate)) return "—";
  return `${Math.round(rate * 100)}%`;
}
