// Lógica pura de la pantalla de integraciones (estado, mapa de estados de ClickUp, agrupación de tareas).

export type Tone = "muted" | "ok" | "warn" | "danger" | "accent";

export const CONNECTION_LABEL: Record<string, { label: string; tone: Tone }> = {
  connected: { label: "Conectado", tone: "ok" },
  error: { label: "Error", tone: "danger" },
  disconnected: { label: "Desconectado", tone: "muted" },
  revoked: { label: "Revocado", tone: "muted" },
};

export function connectionInfo(status: string | null | undefined): { label: string; tone: Tone } {
  return CONNECTION_LABEL[status ?? "disconnected"] ?? { label: status ?? "Desconectado", tone: "muted" };
}

export type StatusCategory = "open" | "need_info" | "closed";
export const CATEGORY_LABEL: Record<StatusCategory, string> = {
  open: "Abierta",
  need_info: "Need info (bloqueada)",
  closed: "Cerrada / hecha",
};

/** Mapa por defecto del vault: `new` abierta, `need info` bloqueada, cerradas → hecha. */
export const DEFAULT_STATUS_MAP: Record<string, StatusCategory> = {
  new: "open",
  "need info": "need_info",
  closed: "closed",
  complete: "closed",
};

export interface StatusRow {
  status: string;
  category: StatusCategory;
}

export function statusMapToRows(map: unknown): StatusRow[] {
  const src =
    map && typeof map === "object" && !Array.isArray(map) && Object.keys(map).length > 0
      ? (map as Record<string, unknown>)
      : DEFAULT_STATUS_MAP;
  return Object.entries(src)
    .filter(
      (e): e is [string, StatusCategory] => e[1] === "open" || e[1] === "need_info" || e[1] === "closed",
    )
    .map(([status, category]) => ({ status, category }));
}

/**
 * Filas → mapa para la API. Los estados de ClickUp se comparan en minúsculas; filas vacías se omiten y la última
 * repetición gana. Devuelve también errores legibles.
 */
export function rowsToStatusMap(rows: StatusRow[]): {
  map: Record<string, StatusCategory>;
  errors: string[];
} {
  const map: Record<string, StatusCategory> = {};
  const errors: string[] = [];
  const seen = new Set<string>();
  for (const r of rows) {
    const k = r.status.trim().toLowerCase();
    if (!k) continue;
    if (seen.has(k)) errors.push(`El estado «${k}» está repetido.`);
    seen.add(k);
    map[k] = r.category;
  }
  if (!Object.values(map).includes("open")) errors.push("Mapea al menos un estado como «Abierta».");
  return { map, errors };
}

export interface TaskGroup<T> {
  key: "easy" | "assigned" | "need_info";
  title: string;
  items: T[];
}

/** Orden de la daily: fáciles (con razón) → resto asignadas → need info. */
export function groupClickupTasks<T extends { group: string }>(tasks: T[]): TaskGroup<T>[] {
  const groups: TaskGroup<T>[] = [
    { key: "easy", title: "Fáciles para hoy", items: [] },
    { key: "assigned", title: "Asignadas", items: [] },
    { key: "need_info", title: "Need info (bloqueadas)", items: [] },
  ];
  for (const t of tasks) {
    const g = groups.find((x) => x.key === t.group) ?? groups[1];
    g?.items.push(t);
  }
  return groups.filter((g) => g.items.length > 0);
}

export function googleConfig(cfg: unknown): {
  calendars: string[];
  calendarEnabled: boolean;
  gmailEnabled: boolean;
} {
  const c = cfg && typeof cfg === "object" ? (cfg as Record<string, unknown>) : {};
  return {
    calendars: Array.isArray(c.calendars)
      ? c.calendars.filter((x): x is string => typeof x === "string")
      : [],
    calendarEnabled: c.calendar_enabled !== false,
    gmailEnabled: c.gmail_enabled !== false,
  };
}

export function clickupConfig(cfg: unknown): { viewId: string; areaId: string; statusMap: unknown } {
  const c = cfg && typeof cfg === "object" ? (cfg as Record<string, unknown>) : {};
  return {
    viewId: typeof c.view_id === "string" ? c.view_id : "",
    areaId: typeof c.area_id === "string" ? c.area_id : "",
    statusMap: c.status_map,
  };
}

/** Explicación del parámetro `?google=` con el que vuelve el callback OAuth. */
export function googleReturnMessage(v: string | null): { tone: "ok" | "error"; text: string } | null {
  if (v === "connected") return { tone: "ok", text: "Google conectado. Elige los calendarios y sincroniza." };
  if (v === "cancelled") return { tone: "error", text: "Cancelaste la autorización de Google." };
  if (v === "error") return { tone: "error", text: "No se pudo conectar Google. Intenta de nuevo." };
  return null;
}
