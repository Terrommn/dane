// Formato es-MX y utilidades de fecha civil (YYYY-MM-DD, sin zona: las fechas de actividad son locales).

export function fold(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

export function todayIso(timeZone?: string, now: Date = new Date()): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(now);
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")}`;
}

export function addDays(iso: string, n: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const dt = new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1));
  dt.setUTCDate(dt.getUTCDate() + n);
  return dt.toISOString().slice(0, 10);
}

/** "jueves 12 de noviembre de 2026" (fecha civil, sin desplazamiento por zona). */
export function formatLongDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Intl.DateTimeFormat("es-MX", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1)));
}

export function formatShortDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Intl.DateTimeFormat("es-MX", { day: "numeric", month: "short", timeZone: "UTC" }).format(
    new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1)),
  );
}

export function formatDateTime(isoInstant: string, timeZone?: string): string {
  return new Intl.DateTimeFormat("es-MX", { dateStyle: "medium", timeStyle: "short", timeZone }).format(
    new Date(isoInstant),
  );
}

/** Centavos → "$1,250.00". */
export function formatMoney(cents: number, currency = "MXN"): string {
  return new Intl.NumberFormat("es-MX", { style: "currency", currency }).format(cents / 100);
}

/** "HH:MM:SS" del servidor → "HH:MM" para <input type="time">. */
export function toTimeInput(t: string | null | undefined): string {
  return (t ?? "").slice(0, 5);
}
/** "HH:MM" → "HH:MM:00" (formato NaiveTime del servidor). */
export function fromTimeInput(t: string): string {
  return /^\d{2}:\d{2}$/.test(t) ? `${t}:00` : t;
}
