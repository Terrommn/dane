// Crear/editar una tarea con su historial (arrastres, completada, reabierta) y enlaces.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { ConflictPrompt, LinksPanel, useAreasAndProjects } from "../notes/common";
import {
  EVENT_LABEL,
  PRIORITY,
  type Task,
  type TaskBody,
  taskBody,
  usePeopleOptions,
  useTaskDetail,
  useTaskMutations,
} from "./tasksApi";

type Form = Omit<TaskBody, "version" | "archived">;

function emptyForm(defaults: Partial<Form>): Form {
  return {
    title: "",
    notes_md: "",
    area_id: null,
    project_id: null,
    priority: null,
    due_on: null,
    planned_on: null,
    class_note_id: null,
    person_ids: [],
    ...defaults,
  };
}

export function ExternalNotice({ task }: { task: Task }) {
  if (!task.external_source) return null;
  return (
    <div className="rounded-lg border border-warn/50 bg-surface-2 p-3 text-sm">
      <p>
        Tarea de <strong>ClickUp</strong>
        {task.external_status && <> · estado «{task.external_status}»</>}
        {task.external_url && (
          <>
            {" · "}
            <a href={task.external_url} target="_blank" rel="noopener noreferrer">
              Abrir en ClickUp
            </a>
          </>
        )}
      </p>
      <p className="mt-1 text-muted">Marcarla aquí solo cambia su estado en esta app; no modifica ClickUp.</p>
    </div>
  );
}

export function TaskDialog({
  open,
  onClose,
  taskId,
  defaults = {},
}: {
  open: boolean;
  onClose: () => void;
  /** null = nueva tarea. */
  taskId: string | null;
  defaults?: Partial<Form>;
}) {
  const detail = useTaskDetail(taskId);
  const task = detail.data?.task ?? null;
  return (
    <Dialog open={open} onClose={onClose} size="lg" title={taskId ? "Tarea" : "Nueva tarea"}>
      {taskId && !task ? (
        <p className="text-muted">{detail.isError ? errorMessage(detail.error) : "Cargando…"}</p>
      ) : (
        <TaskForm
          key={task ? `${task.id}:${task.version}` : "new"}
          task={task}
          defaults={defaults}
          onDone={onClose}
          events={detail.data?.events ?? []}
        />
      )}
    </Dialog>
  );
}

function TaskForm({
  task,
  defaults,
  onDone,
  events,
}: {
  task: Task | null;
  defaults: Partial<Form>;
  onDone: () => void;
  events: { id: number; at: string; kind: string; from_date?: string | null; to_date?: string | null }[];
}) {
  const { areas, projects } = useAreasAndProjects();
  const people = usePeopleOptions();
  const { create, update, transition, archive } = useTaskMutations();
  const toast = useToast();
  const [form, setForm] = useState<Form>(() => (task ? taskBody(task) : emptyForm(defaults)));
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<Task | null>(null);
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => ({ ...f, [k]: v }));

  const save = async (data: Form, version?: number) => {
    if (!data.title.trim()) return setError("Escribe qué hay que hacer");
    setError(null);
    try {
      if (task)
        await update.mutateAsync({
          id: task.id,
          body: { ...data, archived: false, version: version ?? task.version },
        });
      else await create.mutateAsync({ ...data, title: data.title.trim() });
      toast.show(task ? "Tarea guardada" : "Tarea creada", { tone: "ok" });
      onDone();
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as Task);
      else if (e instanceof ApiError && e.field) setError(e.message);
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const projectOptions = projects.filter(
    (p) => !form.area_id || p.area_id === form.area_id || p.id === form.project_id,
  );
  return (
    <div className="space-y-4">
      {task && <ExternalNotice task={task} />}
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          void save(form);
        }}
      >
        <TextField
          label="Tarea"
          value={form.title}
          onChange={(e) => set("title", e.target.value)}
          error={error ?? undefined}
          autoFocus={!task}
        />
        <div className="grid gap-3 sm:grid-cols-2">
          <SelectField
            label="Área"
            value={form.area_id ?? ""}
            onChange={(e) => set("area_id", e.target.value || null)}
          >
            <option value="">Sin área</option>
            {areas.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </SelectField>
          <SelectField
            label="Proyecto"
            value={form.project_id ?? ""}
            onChange={(e) => set("project_id", e.target.value || null)}
          >
            <option value="">Sin proyecto</option>
            {projectOptions.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </SelectField>
          <TextField
            label="Para el día"
            type="date"
            value={form.planned_on ?? ""}
            onChange={(e) => set("planned_on", e.target.value || null)}
            hint="Aparece en Hoy ese día; si no se hace, se arrastra (misma tarea)."
          />
          <TextField
            label="Fecha límite"
            type="date"
            value={form.due_on ?? ""}
            onChange={(e) => set("due_on", e.target.value || null)}
          />
          <SelectField
            label="Prioridad"
            value={form.priority?.toString() ?? ""}
            onChange={(e) => set("priority", e.target.value ? Number(e.target.value) : null)}
          >
            <option value="">Sin prioridad</option>
            {Object.entries(PRIORITY).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </SelectField>
          <fieldset>
            <legend className="mb-1 text-sm font-medium">Personas</legend>
            <div className="max-h-32 space-y-1 overflow-y-auto rounded-lg border border-line p-2">
              {(people.data ?? []).length === 0 && (
                <p className="text-sm text-muted">Sin personas registradas.</p>
              )}
              {(people.data ?? []).map((p) => (
                <label key={p.id} className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={form.person_ids?.includes(p.id) ?? false}
                    onChange={(e) =>
                      set(
                        "person_ids",
                        e.target.checked
                          ? [...(form.person_ids ?? []), p.id]
                          : (form.person_ids ?? []).filter((x) => x !== p.id),
                      )
                    }
                  />
                  {p.name}
                </label>
              ))}
            </div>
          </fieldset>
        </div>
        <TextArea
          label="Notas"
          rows={3}
          value={form.notes_md ?? ""}
          onChange={(e) => set("notes_md", e.target.value)}
        />
        <div className="flex flex-wrap justify-between gap-2">
          <div className="flex flex-wrap gap-2">
            {task?.status === "open" && (
              <>
                <Button
                  variant="secondary"
                  onClick={() => transition.mutate({ task, to: "complete" }, { onSuccess: onDone })}
                >
                  Marcar hecha
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => transition.mutate({ task, to: "cancel" }, { onSuccess: onDone })}
                >
                  Cancelar tarea
                </Button>
              </>
            )}
            {task && task.status !== "open" && (
              <Button
                variant="secondary"
                onClick={() => transition.mutate({ task, to: "reopen" }, { onSuccess: onDone })}
              >
                Reabrir
              </Button>
            )}
            {task && !task.archived_at && (
              <Button variant="ghost" onClick={() => archive.mutate(task.id, { onSuccess: onDone })}>
                Archivar
              </Button>
            )}
          </div>
          <Button type="submit" loading={create.isPending || update.isPending}>
            {task ? "Guardar" : "Crear"}
          </Button>
        </div>
      </form>

      {task && (
        <>
          <section aria-label="Historial">
            <h3 className="mb-2 font-semibold">Historial</h3>
            <ol className="space-y-1 text-sm">
              {events.map((ev) => (
                <li key={ev.id} className="flex flex-wrap items-center gap-2">
                  <Badge
                    tone={ev.kind === "carried_over" ? "warn" : ev.kind === "completed" ? "ok" : "muted"}
                  >
                    {EVENT_LABEL[ev.kind] ?? ev.kind}
                  </Badge>
                  <span className="text-muted">{formatDateTime(ev.at)}</span>
                  {ev.kind === "carried_over" && ev.from_date && ev.to_date && (
                    <span>
                      {formatShortDate(ev.from_date)} → {formatShortDate(ev.to_date)}
                    </span>
                  )}
                </li>
              ))}
            </ol>
          </section>
          <LinksPanel type="task" id={task.id} />
        </>
      )}
      <ConflictPrompt
        conflict={conflict}
        mine={form as unknown as Record<string, unknown>}
        onClose={() => setConflict(null)}
        onTakeServer={(srv) => setForm(taskBody(srv))}
        onSave={(data, v) => void save(data as unknown as Form, v)}
      />
    </div>
  );
}
