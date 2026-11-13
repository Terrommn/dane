import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type Task = Schemas["Task"];
export type TaskBody = Schemas["TaskBody"];
/**
 * Regla de pendiente recurrente. Declarada aquí porque en el OpenAPI actual `RecurrenceRule`/`RuleBody` chocan con
 * los de finanzas (mismo nombre de esquema) y el tipo generado no corresponde a /task-recurrences.
 */
export interface RecurrenceRule {
  id: string;
  title: string;
  area_id?: string | null;
  project_id?: string | null;
  day_kind: string;
  day?: number | null;
  active: boolean;
  starts_on: string;
  version: number;
}
export interface RuleInput {
  title: string;
  area_id?: string | null;
  project_id?: string | null;
  day_kind: string;
  day?: number | null;
  active?: boolean;
  starts_on?: string | null;
  version?: number | null;
}

export const PRIORITY: Record<number, string> = { 1: "Alta", 2: "Media", 3: "Normal", 4: "Baja" };
export const TASK_STATUS: Record<string, string> = { open: "Abierta", done: "Hecha", cancelled: "Cancelada" };
export const EVENT_LABEL: Record<string, string> = {
  created: "Creada",
  carried_over: "Arrastrada",
  completed: "Completada",
  reopened: "Reabierta",
  edited: "Editada",
  cancelled: "Cancelada",
  external_sync: "Sincronizada",
};

/** Texto legible de la regla: "Día 31 (o el último si el mes es más corto)", "Último día", "Penúltimo día". */
export function ruleLabel(r: { day_kind: string; day?: number | null }): string {
  if (r.day_kind === "last") return "Último día del mes";
  if (r.day_kind === "penultimate") return "Penúltimo día del mes";
  const d = r.day ?? 1;
  return d >= 29 ? `Día ${d} (o el último si el mes es más corto)` : `Día ${d} de cada mes`;
}

export function taskBody(t: Task, patch: Partial<TaskBody> = {}): TaskBody {
  return {
    title: t.title,
    notes_md: t.notes_md,
    area_id: t.area_id ?? null,
    project_id: t.project_id ?? null,
    priority: t.priority ?? null,
    due_on: t.due_on ?? null,
    planned_on: t.planned_on ?? null,
    class_note_id: t.class_note_id ?? null,
    person_ids: t.person_ids,
    archived: t.archived_at != null,
    ...patch,
    version: t.version,
  };
}

export function useDayTasks(date: string) {
  return useQuery({
    queryKey: ["tasks", "day", date],
    queryFn: async () => unwrap(await api.GET("/tasks/day/{date}", { params: { path: { date } } })),
  });
}

export interface TaskFilter {
  status?: string;
  area_id?: string;
  project_id?: string;
  q?: string;
  external?: boolean;
  archived?: boolean;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
}

export function useTaskList(f: TaskFilter) {
  return useQuery({
    queryKey: ["tasks", "list", f],
    queryFn: async () => {
      const page = unwrap(await api.GET("/tasks", { params: { query: { ...f, limit: f.limit ?? 100 } } }));
      return { ...page, items: page.items as unknown as Task[] };
    },
  });
}

export function useTaskDetail(id: string | null) {
  return useQuery({
    queryKey: ["task", id],
    enabled: !!id,
    queryFn: async () => unwrap(await api.GET("/tasks/{id}", { params: { path: { id: id ?? "" } } })),
  });
}

export function useRecurrences() {
  return useQuery({
    queryKey: ["task-recurrences"],
    queryFn: async () => unwrap(await api.GET("/task-recurrences")) as unknown as RecurrenceRule[],
  });
}

export function usePeopleOptions() {
  return useQuery({
    queryKey: ["people", "options"],
    queryFn: async () => unwrap(await api.GET("/people", { params: { query: {} } })),
  });
}

export function useTaskMutations() {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["tasks"] });
    void qc.invalidateQueries({ queryKey: ["task"] });
  };
  const create = useMutation({
    mutationFn: async (body: TaskBody) => unwrap(await api.POST("/tasks", { body })),
    onSuccess: refresh,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: TaskBody }) =>
      unwrap(await api.PUT("/tasks/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
  const transition = useMutation({
    mutationFn: async ({ task, to }: { task: Task; to: "complete" | "reopen" | "cancel" }) => {
      const opts = { params: { path: { id: task.id } }, body: { version: task.version } };
      if (to === "complete") return unwrap(await api.POST("/tasks/{id}/complete", opts));
      if (to === "reopen") return unwrap(await api.POST("/tasks/{id}/reopen", opts));
      return unwrap(await api.POST("/tasks/{id}/cancel", opts));
    },
    onSuccess: refresh,
  });
  const archive = useMutation({
    mutationFn: async (id: string) => unwrap(await api.DELETE("/tasks/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const carryOver = useMutation({
    mutationFn: async (to_date: string) => unwrap(await api.POST("/tasks/carry-over", { body: { to_date } })),
    onSuccess: refresh,
  });
  return { create, update, transition, archive, carryOver };
}

export function useRecurrenceMutations() {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["task-recurrences"] });
    void qc.invalidateQueries({ queryKey: ["tasks"] });
  };
  const save = useMutation({
    mutationFn: async ({ id, body }: { id?: string; body: RuleInput }) =>
      (id
        ? unwrap(await api.PUT("/task-recurrences/{id}", { params: { path: { id } }, body: body as never }))
        : unwrap(await api.POST("/task-recurrences", { body: body as never }))) as unknown as RecurrenceRule,
    onSuccess: refresh,
  });
  const materialize = useMutation({
    mutationFn: async () => unwrap(await api.POST("/task-recurrences/materialize")),
    onSuccess: refresh,
  });
  return { save, materialize };
}
