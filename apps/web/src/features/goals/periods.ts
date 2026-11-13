// Periodos civiles: semanas ISO (lunes–domingo, año ISO) y meses `YYYY-MM`. Sin zona: fechas locales `YYYY-MM-DD`.
import { addDays } from "../../lib/format";

function parse(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1));
}
const fmt = (d: Date) => d.toISOString().slice(0, 10);

/** Lunes de la semana ISO de la fecha. */
export function isoWeekStart(iso: string): string {
  const d = parse(iso);
  const dow = (d.getUTCDay() + 6) % 7; // lunes = 0
  return addDays(iso, -dow);
}

/** Clave `GGGG-Www` (el año ISO puede diferir del civil: 2026-12-31 → 2026-W53, 2024-12-30 → 2025-W01). */
export function isoWeekKey(iso: string): string {
  const monday = parse(isoWeekStart(iso));
  const thursday = new Date(monday);
  thursday.setUTCDate(monday.getUTCDate() + 3);
  const year = thursday.getUTCFullYear();
  const jan4 = new Date(Date.UTC(year, 0, 4));
  const week1Monday = parse(isoWeekStart(fmt(jan4)));
  const week = Math.round((monday.getTime() - week1Monday.getTime()) / (7 * 86400000)) + 1;
  return `${year}-W${String(week).padStart(2, "0")}`;
}

export function isValidWeekKey(key: string | null | undefined): key is string {
  if (!key || !/^\d{4}-W\d{2}$/.test(key)) return false;
  const start = weekKeyStart(key);
  return start !== null && isoWeekKey(start) === key;
}

/** Lunes de una clave `GGGG-Www` (null si no existe, p. ej. W54). */
export function weekKeyStart(key: string): string | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(key);
  if (!m) return null;
  const year = Number(m[1]);
  const week = Number(m[2]);
  if (week < 1 || week > 53) return null;
  const week1 = isoWeekStart(fmt(new Date(Date.UTC(year, 0, 4))));
  const start = addDays(week1, (week - 1) * 7);
  return isoWeekKey(start) === key ? start : null;
}

export function shiftWeek(key: string, n: number): string {
  const start = weekKeyStart(key);
  if (!start) return key;
  return isoWeekKey(addDays(start, n * 7));
}

export function monthKey(iso: string): string {
  return iso.slice(0, 7);
}

export function isValidMonthKey(key: string | null | undefined): key is string {
  if (!key || !/^\d{4}-\d{2}$/.test(key)) return false;
  const mm = Number(key.slice(5));
  return mm >= 1 && mm <= 12;
}

export function shiftMonth(key: string, n: number): string {
  const y = Number(key.slice(0, 4));
  const m = Number(key.slice(5, 7)) - 1 + n;
  const d = new Date(Date.UTC(y, m, 1));
  return fmt(d).slice(0, 7);
}

export function monthStart(key: string): string {
  return `${key}-01`;
}

export function monthEnd(key: string): string {
  return addDays(`${shiftMonth(key, 1)}-01`, -1);
}

/** "noviembre 2026". */
export function formatMonth(key: string): string {
  return new Intl.DateTimeFormat("es-MX", { month: "long", year: "numeric", timeZone: "UTC" }).format(
    parse(`${key}-01`),
  );
}

/** "Semana 46 · 9–15 nov". */
export function formatWeek(key: string): string {
  const start = weekKeyStart(key);
  if (!start) return key;
  const end = addDays(start, 6);
  const f = (iso: string) =>
    new Intl.DateTimeFormat("es-MX", { day: "numeric", month: "short", timeZone: "UTC" }).format(parse(iso));
  return `Semana ${Number(key.slice(6))} · ${f(start)} – ${f(end)}`;
}
