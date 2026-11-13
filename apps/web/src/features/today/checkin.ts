// Modelo del check-in diario en el cliente.
//
// * `formFromFull` convierte la respuesta del servidor (CheckinFull / DayView aplanado) en el formulario.
// * `buildPatch` produce un guardado PARCIAL: solo los campos que cambiaron respecto de la base sobre la que se editó
//   (campo ausente = sin cambio; `null` explícito = borrar → "sin dato", que nunca es 0 ni "no").
// * Las secciones son opcionales: un check-in con solo energía es válido.
import type { Schemas } from "../../lib/api";
import { parsePesosToCents, parseQuantity, parseWeight } from "./numbers";

export type HabitStatus = "done" | "not_done" | "not_applicable";

export interface HabitMark {
  /** null = sin registrar. */
  status: HabitStatus | null;
  reason: string;
  quantity: string;
}

export interface CheckinForm {
  /** "" = sin dato. */
  weight_kg: string;
  /** null = sin dato. */
  energy: number | null;
  planned_spend_text: string;
  /** Monto en pesos como texto; "" = no dijo monto. */
  planned_spend_amount: string;
  /** "No planeo gastar nada" (distinto de no haber respondido). */
  planned_spend_none: boolean;
  notes_md: string;
  log: string[];
  went_well: string[];
  went_badly: string[];
  habits: Record<string, HabitMark>;
  morning_done: boolean;
  evening_done: boolean;
}

/** Lo que guarda el borrador local: lo editado y la base del servidor sobre la que se editó. */
export interface CheckinDraft {
  form: CheckinForm;
  base: CheckinForm;
}

type FullLike = Pick<
  Schemas["CheckinFull"],
  "checkin" | "log" | "went_well" | "went_badly" | "habit_entries" | "version"
>;

export function emptyForm(): CheckinForm {
  return {
    weight_kg: "",
    energy: null,
    planned_spend_text: "",
    planned_spend_amount: "",
    planned_spend_none: false,
    notes_md: "",
    log: [],
    went_well: [],
    went_badly: [],
    habits: {},
    morning_done: false,
    evening_done: false,
  };
}

function weightText(w: string | null | undefined): string {
  if (!w) return "";
  const n = Number(w);
  return Number.isFinite(n) ? String(n) : w;
}

function amountText(cents: number | null | undefined): string {
  if (cents === null || cents === undefined) return "";
  const pesos = Math.floor(cents / 100);
  const c = cents % 100;
  return c === 0 ? String(pesos) : `${pesos}.${String(c).padStart(2, "0")}`;
}

export function formFromFull(full: FullLike): CheckinForm {
  const c = full.checkin ?? null;
  const habits: Record<string, HabitMark> = {};
  for (const e of full.habit_entries) {
    habits[e.habit_id] = {
      status: e.status as HabitStatus,
      reason: e.reason ?? "",
      quantity: e.quantity ? weightText(e.quantity) : "",
    };
  }
  const sortText = (xs: { text: string; position: number }[]) =>
    [...xs].sort((a, b) => a.position - b.position).map((x) => x.text);
  return {
    weight_kg: weightText(c?.weight_kg),
    energy: c?.energy ?? null,
    planned_spend_text: c?.planned_spend_text ?? "",
    planned_spend_amount: amountText(c?.planned_spend_cents),
    planned_spend_none: c?.planned_spend_none ?? false,
    notes_md: c?.notes_md ?? "",
    log: sortText(full.log),
    went_well: sortText(full.went_well),
    went_badly: sortText(full.went_badly),
    habits,
    morning_done: c?.morning_done_at != null,
    evening_done: c?.evening_done_at != null,
  };
}

const sameList = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);
const clean = (xs: string[]) => xs.map((x) => x.trim()).filter(Boolean);
const NO_MARK: HabitMark = { status: null, reason: "", quantity: "" };

export function sameMark(a: HabitMark | undefined, b: HabitMark | undefined): boolean {
  const x = a ?? NO_MARK;
  const y = b ?? NO_MARK;
  return (
    x.status === y.status && x.reason.trim() === y.reason.trim() && x.quantity.trim() === y.quantity.trim()
  );
}

export type BuildResult =
  | { ok: true; patch: Schemas["CheckinPatch"]; empty: boolean }
  | { ok: false; error: string };

/** Parche parcial con solo lo que cambió. `extra` permite forzar marcas (mañana/noche). */
export function buildPatch(form: CheckinForm, base: CheckinForm, version: number): BuildResult {
  const patch: Schemas["CheckinPatch"] = { version };
  let changes = 0;
  const set = <K extends keyof Schemas["CheckinPatch"]>(k: K, v: Schemas["CheckinPatch"][K]) => {
    patch[k] = v;
    changes++;
  };

  if (form.weight_kg.trim() !== base.weight_kg.trim()) {
    const w = parseWeight(form.weight_kg);
    if (!w.ok) return { ok: false, error: w.error };
    set("weight_kg", w.value);
  }
  if (form.energy !== base.energy) {
    if (form.energy !== null && !(Number.isInteger(form.energy) && form.energy >= 1 && form.energy <= 5))
      return { ok: false, error: "La energía va de 1 a 5" };
    set("energy", form.energy);
  }
  if (form.planned_spend_text.trim() !== base.planned_spend_text.trim())
    set("planned_spend_text", form.planned_spend_text.trim() || null);
  if (form.planned_spend_amount.trim() !== base.planned_spend_amount.trim()) {
    const a = parsePesosToCents(form.planned_spend_amount);
    if (!a.ok) return { ok: false, error: `Gasto planeado: ${a.error}` };
    set("planned_spend_cents", a.value);
  }
  if (form.planned_spend_none !== base.planned_spend_none) set("planned_spend_none", form.planned_spend_none);
  if (form.notes_md !== base.notes_md) set("notes_md", form.notes_md);
  for (const k of ["log", "went_well", "went_badly"] as const) {
    if (!sameList(clean(form[k]), clean(base[k]))) set(k, clean(form[k]));
  }
  if (form.morning_done !== base.morning_done) set("morning_done", form.morning_done);
  if (form.evening_done !== base.evening_done) set("evening_done", form.evening_done);

  const ids = new Set([...Object.keys(form.habits), ...Object.keys(base.habits)]);
  const habits: Schemas["HabitEntryPatch"][] = [];
  for (const id of ids) {
    const m = form.habits[id] ?? NO_MARK;
    if (sameMark(m, base.habits[id])) continue;
    if (m.status === "not_applicable" && !m.reason.trim())
      return { ok: false, error: "Explica por qué un hábito no aplica (no cuenta para el cumplimiento)" };
    const q = parseQuantity(m.quantity);
    if (!q.ok) return { ok: false, error: q.error };
    habits.push({
      habit_id: id,
      status: m.status,
      reason: m.status === "not_applicable" ? m.reason.trim() : m.reason.trim() || null,
      quantity: q.value,
    });
  }
  if (habits.length) set("habits", habits);
  return { ok: true, patch, empty: changes === 0 };
}

/**
 * Aplica un parche (p. ej. el de una operación pendiente en la cola offline) sobre el formulario del servidor, para
 * mostrar al usuario sus cambios aún no sincronizados.
 */
export function applyPatch(form: CheckinForm, patch: Partial<Schemas["CheckinPatch"]>): CheckinForm {
  const f: CheckinForm = { ...form, habits: { ...form.habits } };
  if ("weight_kg" in patch) f.weight_kg = patch.weight_kg ? weightText(patch.weight_kg) : "";
  if ("energy" in patch) f.energy = patch.energy ?? null;
  if ("planned_spend_text" in patch) f.planned_spend_text = patch.planned_spend_text ?? "";
  if ("planned_spend_cents" in patch) f.planned_spend_amount = amountText(patch.planned_spend_cents);
  if (typeof patch.planned_spend_none === "boolean") f.planned_spend_none = patch.planned_spend_none;
  if (typeof patch.notes_md === "string") f.notes_md = patch.notes_md;
  if (patch.log) f.log = [...patch.log];
  if (patch.went_well) f.went_well = [...patch.went_well];
  if (patch.went_badly) f.went_badly = [...patch.went_badly];
  if (typeof patch.morning_done === "boolean") f.morning_done = patch.morning_done;
  if (typeof patch.evening_done === "boolean") f.evening_done = patch.evening_done;
  for (const h of patch.habits ?? []) {
    f.habits[h.habit_id] = {
      status: (h.status as HabitStatus | null | undefined) ?? null,
      reason: h.reason ?? "",
      quantity: h.quantity ? weightText(h.quantity) : "",
    };
  }
  return f;
}

/** Normaliza lo guardado como borrador: `{form, base}` (edición local) o un parche (envío pendiente/en conflicto). */
export function draftToLocal(data: unknown, server: CheckinForm): CheckinDraft {
  if (data && typeof data === "object" && "form" in data && "base" in data) return data as CheckinDraft;
  return { form: applyPatch(server, (data ?? {}) as Partial<Schemas["CheckinPatch"]>), base: server };
}

/** Hay cambios locales respecto de la base. */
export function isDirty(d: CheckinDraft | undefined | null): boolean {
  if (!d) return false;
  const r = buildPatch(d.form, d.base, 0);
  return !r.ok || !r.empty;
}

/** Campos del parche → vista comparable para el diálogo de conflicto (mía vs servidor). */
export function patchView(patch: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(patch)) if (k !== "version") out[k] = v;
  return out;
}

/** Estado del servidor (CheckinFull) en la misma forma que un parche, para comparar campo por campo. */
export function serverAsPatch(current: FullLike): Record<string, unknown> {
  const f = formFromFull(current);
  const cents = parsePesosToCents(f.planned_spend_amount);
  return {
    weight_kg: f.weight_kg || null,
    energy: f.energy,
    planned_spend_text: f.planned_spend_text || null,
    planned_spend_cents: cents.ok ? cents.value : null,
    planned_spend_none: f.planned_spend_none,
    notes_md: f.notes_md,
    log: f.log,
    went_well: f.went_well,
    went_badly: f.went_badly,
    morning_done: f.morning_done,
    evening_done: f.evening_done,
    habits: Object.entries(f.habits).map(([habit_id, m]) => ({
      habit_id,
      status: m.status,
      reason: m.reason || null,
      quantity: m.quantity || null,
    })),
  };
}

export const ENERGY_LABEL: Record<number, string> = {
  1: "Muy baja",
  2: "Baja",
  3: "Normal",
  4: "Alta",
  5: "Muy alta",
};

export const HABIT_STATUS_LABEL: Record<HabitStatus | "unrecorded", string> = {
  done: "Cumplido",
  not_done: "No",
  not_applicable: "No aplica",
  unrecorded: "Sin registrar",
};
