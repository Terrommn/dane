import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, apiFetch, type Schemas } from "../../lib/api";
import { toApiError, unwrap } from "../../lib/errors";

export type Habit = Schemas["Habit"];
export type HabitSummary = Schemas["HabitSummary"];
export type HabitDetail = Schemas["HabitDetail"];
export type HabitVersion = Schemas["HabitVersionRow"];
export type HabitLink = Schemas["HabitLink"];
export type FrequencyBody = Schemas["FrequencyBody"];

/** Formas reales de los objetos que el OpenAPI declara como `Object` (crates/domain/src/metrics.rs). */
export interface Compliance {
  required: number;
  done: number;
  missed: number;
  unrecorded: number;
  excluded_not_applicable: number;
  paused_days: number;
  extra: number;
  rate: number | null;
}
export interface Streaks {
  current: number;
  longest: number;
  unit: "days" | "weeks";
}
export interface TodayProgress {
  date: string;
  shown: boolean;
  required_today: boolean;
  status: "done" | "not_done" | "not_applicable" | null;
  week_done: number | null;
  week_target: number | null;
}

export const asCompliance = (x: unknown) => x as Compliance;
export const asStreaks = (x: unknown) => x as Streaks;
export const asToday = (x: unknown) => x as TodayProgress;

export const habitKeys = {
  all: ["habits"] as const,
  list: (date: string) => ["habits", "list", date] as const,
  detail: (id: string) => ["habits", "detail", id] as const,
  history: (from: string, to: string) => ["habits", "history", from, to] as const,
  links: ["habits", "links"] as const,
};

export function useHabits(date: string, archived = false) {
  return useQuery({
    queryKey: [...habitKeys.list(date), archived],
    queryFn: async () => unwrap(await api.GET("/habits", { params: { query: { date, archived } } })),
  });
}

export function useHabit(id: string) {
  return useQuery({
    queryKey: habitKeys.detail(id),
    queryFn: async () => unwrap(await api.GET("/habits/{id}", { params: { path: { id } } })),
  });
}

export function useHabitHistory(from: string, to: string) {
  return useQuery({
    queryKey: habitKeys.history(from, to),
    queryFn: async () => unwrap(await api.GET("/habits/history", { params: { query: { from, to } } })),
  });
}

export function useHabitLinks() {
  return useQuery({
    queryKey: habitKeys.links,
    queryFn: async () => unwrap(await api.GET("/habit-links")),
  });
}

export function useHabitMutations() {
  const qc = useQueryClient();
  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: habitKeys.all });
    void qc.invalidateQueries({ queryKey: ["goals"] });
    void qc.invalidateQueries({ queryKey: ["dashboard"] });
    void qc.invalidateQueries({ queryKey: ["day"] });
  };
  const create = useMutation({
    mutationFn: async (body: Schemas["CreateHabit"]) => unwrap(await api.POST("/habits", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["UpdateHabit"] }) =>
      unwrap(await api.PUT("/habits/{id}", { params: { path: { id } }, body })),
    onSettled: invalidate,
  });
  const addVersion = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["NewVersionBody"] }) =>
      unwrap(await api.POST("/habits/{id}/versions", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const addPause = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["HabitPauseBody"] }) =>
      unwrap(await api.POST("/habits/{id}/pauses", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const deletePause = useMutation({
    mutationFn: async ({ id, pauseId }: { id: string; pauseId: string }) =>
      unwrap(
        await api.DELETE("/habits/{id}/pauses/{pause_id}", {
          params: { path: { id, pause_id: pauseId } },
        }),
      ),
    onSuccess: invalidate,
  });
  const setEntry = useMutation({
    mutationFn: async ({ id, date, body }: { id: string; date: string; body: Schemas["EntryBody"] }) =>
      unwrap(await api.PUT("/habits/{id}/entries/{date}", { params: { path: { id, date } }, body })),
    onSettled: invalidate,
  });
  const clearEntry = useMutation({
    mutationFn: async ({ id, date }: { id: string; date: string }) =>
      unwrap(await api.DELETE("/habits/{id}/entries/{date}", { params: { path: { id, date } } })),
    onSettled: invalidate,
  });
  const setupFixed = useMutation({
    mutationFn: async (body: Schemas["FixedHabitsBody"]) => unwrap(await api.POST("/habits/fixed", { body })),
    onSuccess: invalidate,
  });
  // `/habit-links/*` va por apiFetch: el OpenAPI generado mezcla los esquemas homónimos `LinkBody` de objetivos y
  // hábitos (y sus parámetros de ruta). Mismas cabeceras de auth, CSRF e Idempotency-Key que el cliente tipado.
  const setLink = useMutation({
    mutationFn: async ({ trigger, habitId }: { trigger: string; habitId: string }) =>
      linkRequest("PUT", trigger, { habit_id: habitId }),
    onSuccess: invalidate,
  });
  const removeLink = useMutation({
    mutationFn: async (trigger: string) => linkRequest("DELETE", trigger),
    onSuccess: invalidate,
  });
  return {
    create,
    update,
    addVersion,
    addPause,
    deletePause,
    setEntry,
    clearEntry,
    setupFixed,
    setLink,
    removeLink,
  };
}

async function linkRequest(
  method: "PUT" | "DELETE",
  trigger: string,
  body?: unknown,
): Promise<HabitLink[] | null> {
  const res = await apiFetch(`/habit-links/${encodeURIComponent(trigger)}`, {
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const json = text ? (JSON.parse(text) as unknown) : null;
  if (!res.ok) throw toApiError(res.status, json);
  return json as HabitLink[] | null;
}

export const WEEKDAY_BITS: { bit: number; short: string; long: string }[] = [
  { bit: 1, short: "L", long: "Lunes" },
  { bit: 2, short: "M", long: "Martes" },
  { bit: 4, short: "X", long: "Miércoles" },
  { bit: 8, short: "J", long: "Jueves" },
  { bit: 16, short: "V", long: "Viernes" },
  { bit: 32, short: "S", long: "Sábado" },
  { bit: 64, short: "D", long: "Domingo" },
];

/** "Diario", "L, M, V", "4 veces por semana". */
export function describeFrequency(
  v: Pick<HabitVersion, "frequency" | "weekdays" | "times_per_week"> | null | undefined,
) {
  if (!v) return "Sin frecuencia vigente";
  if (v.frequency === "daily") return "Diario";
  if (v.frequency === "times_per_week") {
    const n = v.times_per_week ?? 0;
    return `${n} ${n === 1 ? "vez" : "veces"} por semana`;
  }
  const days = WEEKDAY_BITS.filter((d) => ((v.weekdays ?? 0) & d.bit) !== 0).map((d) => d.long.slice(0, 3));
  return days.length === 7 ? "Todos los días" : days.join(", ");
}

export const LINK_TRIGGERS: { trigger: string; label: string; hint: string }[] = [
  {
    trigger: "weight_logged",
    label: "Registrar peso",
    hint: "Al anotar tu peso en Hoy se marca este hábito",
  },
  {
    trigger: "expense_logged",
    label: "Registrar un gasto",
    hint: "Al registrar un gasto se marca este hábito",
  },
  {
    trigger: "positive_reflection",
    label: "Escribir algo en «Lo que hice bien»",
    hint: "Al anotar algo bueno del día se marca este hábito",
  },
  {
    trigger: "devotional_content",
    label: "Devocional con contenido",
    hint: "Un devocional escrito (no vacío) marca este hábito",
  },
];

/** Estados de una celda de historial (`HistoryCell.state`). Sin registro y "no" se distinguen siempre. */
export const CELL_STATE: Record<string, { label: string; className: string; symbol: string }> = {
  done: { label: "Cumplido", className: "bg-ok text-white", symbol: "✓" },
  not_done: { label: "No (registrado)", className: "bg-danger text-white", symbol: "✕" },
  not_applicable: { label: "No aplica", className: "bg-surface-2 text-muted", symbol: "–" },
  unrecorded: {
    label: "Sin registro (cuenta como no cumplido)",
    className: "border-2 border-dashed border-warn text-warn",
    symbol: "?",
  },
  not_required: { label: "No exigible ese día", className: "bg-transparent text-muted", symbol: "·" },
  paused: { label: "En pausa", className: "bg-surface-2 text-muted", symbol: "‖" },
  future: { label: "Futuro", className: "bg-transparent text-muted", symbol: "" },
  open: { label: "Hoy (abierto)", className: "border border-accent text-link", symbol: "•" },
};
