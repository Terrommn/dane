import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import type { Compliance } from "../habits/habitsApi";

export type Goal = Schemas["Goal"];
export type GoalView = Schemas["GoalView"];
export type GoalDetail = Schemas["GoalDetail"];
export type GoalBody = Schemas["GoalBody"];
export type Planning = Schemas["Planning"];

/** Forma real de `GoalView.progress` (crates/domain/src/goals.rs: GoalProgress). */
export interface GoalProgress {
  has_data: boolean;
  current: number | null;
  target: number | null;
  baseline: number | null;
  ratio: number | null;
  remaining: number | null;
  achieved: boolean;
  overachieved: boolean;
  target_zero: boolean;
  expected_by_today: number | null;
  pace_delta: number | null;
  projection_estimate: number | null;
  last_measured_on: string | null;
  elapsed_days: number;
  total_days: number;
}

export const asProgress = (x: unknown) => x as GoalProgress;
export const asAdherence = (x: unknown) => x as Compliance;

export interface GoalClosing {
  achievement?: string;
  learning?: string;
  difficulties?: string;
  adjustments?: string;
}

export const GOAL_STATUS: Record<string, string> = {
  active: "Activo",
  achieved: "Logrado",
  not_achieved: "No logrado",
};

export const PROGRESS_TYPE: Record<string, { label: string; hint: string }> = {
  none: { label: "Sin medición", hint: "Solo se cierra como logrado o no logrado" },
  cumulative: { label: "Acumulativo", hint: "Se suma: 4 libros, 20 sesiones, 600 minutos, $5,000" },
  current_value: {
    label: "Valor actual",
    hint: "Una medición con dirección: peso, saldo. No se proyecta linealmente",
  },
  milestone: { label: "Hito", hint: "Un resultado que se completa una vez" },
};

export const goalKeys = {
  all: ["goals"] as const,
  list: (q: Record<string, unknown>) => ["goals", "list", q] as const,
  detail: (id: string) => ["goals", "detail", id] as const,
  planning: (week: string | null) => ["goals", "planning", week] as const,
};

export function useGoals(query: {
  kind?: string;
  period_key?: string;
  status?: string;
  from?: string;
  to?: string;
  archived?: boolean;
}) {
  return useQuery({
    queryKey: goalKeys.list(query),
    queryFn: async () => unwrap(await api.GET("/goals", { params: { query } })),
  });
}

export function useGoal(id: string) {
  return useQuery({
    queryKey: goalKeys.detail(id),
    queryFn: async () => unwrap(await api.GET("/goals/{id}", { params: { path: { id } } })),
  });
}

export function usePlanning(week: string | null) {
  return useQuery({
    queryKey: goalKeys.planning(week),
    queryFn: async () =>
      unwrap(await api.GET("/goals/planning", { params: { query: week ? { week } : {} } })),
  });
}

export function goalToBody(g: Goal, patch: Partial<GoalBody> = {}): GoalBody {
  return {
    title: g.title,
    area_id: g.area_id ?? null,
    motivation: g.motivation,
    success_criteria: g.success_criteria,
    status: g.status,
    parent_goal_id: g.parent_goal_id ?? null,
    progress_type: g.progress_type,
    unit: g.unit ?? null,
    baseline: g.baseline ?? null,
    target: g.target ?? null,
    direction: g.direction ?? null,
    progress_source: g.progress_source,
    source_ref: g.source_ref ?? null,
    position: g.position,
    archived: g.archived_at != null,
    ...patch,
    version: g.version,
  };
}

export function useGoalMutations() {
  const qc = useQueryClient();
  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: goalKeys.all });
    void qc.invalidateQueries({ queryKey: ["dashboard"] });
    void qc.invalidateQueries({ queryKey: ["habits"] });
    void qc.invalidateQueries({ queryKey: ["weekly"] });
  };
  const create = useMutation({
    mutationFn: async (body: GoalBody) => unwrap(await api.POST("/goals", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: GoalBody }) =>
      unwrap(await api.PUT("/goals/{id}", { params: { path: { id } }, body })),
    onSettled: invalidate,
  });
  const close = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["CloseBody"] }) =>
      unwrap(await api.POST("/goals/{id}/close", { params: { path: { id } }, body })),
    onSettled: invalidate,
  });
  const addProgress = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["ProgressBody"] }) =>
      unwrap(await api.POST("/goals/{id}/progress", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const deleteProgress = useMutation({
    mutationFn: async ({ id, eventId }: { id: string; eventId: string }) =>
      unwrap(
        await api.DELETE("/goals/{id}/progress/{event_id}", { params: { path: { id, event_id: eventId } } }),
      ),
    onSuccess: invalidate,
  });
  const addLink = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["LinkBody"] }) =>
      unwrap(await api.POST("/goals/{id}/links", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const removeLink = useMutation({
    mutationFn: async ({ id, srcType, srcId }: { id: string; srcType: string; srcId: string }) =>
      unwrap(
        await api.DELETE("/goals/{id}/links/{src_type}/{src_id}", {
          params: { path: { id, src_type: srcType, src_id: srcId } },
        }),
      ),
    onSuccess: invalidate,
  });
  return { create, update, close, addProgress, deleteProgress, addLink, removeLink };
}

/** Texto del avance de resultado: "3 de 4 libros", "72.4 kg (meta 70)", "Hito pendiente". */
export function describeProgress(g: Goal, p: GoalProgress): string {
  const unit = g.unit ? ` ${g.unit}` : "";
  const n = (x: number | null) =>
    x === null ? "—" : new Intl.NumberFormat("es-MX", { maximumFractionDigits: 2 }).format(x);
  switch (g.progress_type) {
    case "cumulative":
      if (p.target_zero) return `${n(p.current ?? 0)}${unit} (meta 0)`;
      return `${n(p.current ?? 0)} de ${n(p.target)}${unit}`;
    case "current_value":
      return p.has_data
        ? `${n(p.current)}${unit} · meta ${n(p.target)}${unit}`
        : `Sin mediciones · meta ${n(p.target)}${unit}`;
    case "milestone":
      return p.achieved ? "Hito completado" : "Hito pendiente";
    default:
      return "Sin medición cuantitativa";
  }
}
