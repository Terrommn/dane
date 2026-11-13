import { useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { type Column, DataTable } from "../../components/ui/DataTable";
import { DatePicker } from "../../components/ui/DatePicker";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { areaName, useAreasAndProjects } from "../notes/common";
import { TaskDialog } from "./TaskDialog";
import {
  PRIORITY,
  type RecurrenceRule,
  ruleLabel,
  TASK_STATUS,
  type Task,
  useDayTasks,
  useRecurrenceMutations,
  useRecurrences,
  useTaskList,
  useTaskMutations,
} from "./tasksApi";

type Tab = "dia" | "lista" | "recurrentes";

export function useUserToday() {
  const { me } = useAuth();
  return todayIso(me?.settings.timezone ?? undefined);
}

function TaskRow({ task, onOpen, today }: { task: Task; onOpen: () => void; today: string }) {
  const { transition } = useTaskMutations();
  const toast = useToast();
  const done = task.status === "done";
  const carried = task.planned_on && task.planned_on < today && task.status === "open";
  return (
    <li className="flex items-start gap-3 py-2">
      <input
        type="checkbox"
        className="mt-1 h-5 w-5 shrink-0 accent-[var(--c-accent)]"
        aria-label={done ? `Reabrir ${task.title}` : `Completar ${task.title}`}
        checked={done}
        disabled={task.status === "cancelled" || transition.isPending}
        onChange={() =>
          transition.mutate(
            { task, to: done ? "reopen" : "complete" },
            { onError: (e) => toast.show(errorMessage(e), { tone: "error" }) },
          )
        }
      />
      <button type="button" className="min-w-0 flex-1 text-left" onClick={onOpen}>
        <span className={done ? "text-muted line-through" : ""}>{task.title}</span>
        <span className="mt-0.5 flex flex-wrap gap-1">
          {task.priority && (
            <Badge tone={task.priority === 1 ? "danger" : "muted"}>{PRIORITY[task.priority]}</Badge>
          )}
          {task.external_source && <Badge tone="accent">ClickUp</Badge>}
          {task.ease_hint && <Badge tone="ok">🟢 fácil: {task.ease_hint}</Badge>}
          {task.external_status_category === "need_info" && <Badge tone="warn">⏸ need info</Badge>}
          {carried && <Badge tone="warn">Arrastrada desde {formatShortDate(task.planned_on ?? "")}</Badge>}
          {task.due_on && <Badge>vence {formatShortDate(task.due_on)}</Badge>}
          {task.recurrence_rule_id && <Badge>recurrente</Badge>}
        </span>
      </button>
      {task.external_url && (
        <a className="shrink-0 text-sm" href={task.external_url} target="_blank" rel="noopener noreferrer">
          Abrir
        </a>
      )}
    </li>
  );
}

function DayView({ onOpen, onNew }: { onOpen: (id: string) => void; onNew: (d: string) => void }) {
  const today = useUserToday();
  const [params, setParams] = useSearchParams();
  const date = params.get("date") ?? today;
  const day = useDayTasks(date);
  const { carryOver } = useTaskMutations();
  const toast = useToast();
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <DatePicker
          value={date}
          onChange={(d) => {
            const p = new URLSearchParams(params);
            p.set("date", d);
            setParams(p, { replace: true });
          }}
        />
        <div className="flex gap-2">
          <Button
            variant="secondary"
            loading={carryOver.isPending}
            onClick={() =>
              carryOver.mutate(date, {
                onSuccess: (r) =>
                  toast.show(
                    r.moved.length
                      ? `${r.moved.length} pendientes arrastrados (mismas tareas)`
                      : "No había pendientes que arrastrar",
                    {
                      tone: "ok",
                    },
                  ),
                onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
              })
            }
          >
            Arrastrar pendientes a este día
          </Button>
          <Button onClick={() => onNew(date)}>Nueva</Button>
        </div>
      </div>
      <QueryState query={day}>
        {(d) =>
          d.groups.every((g) => g.tasks.length === 0) ? (
            <EmptyState
              title="Sin pendientes para este día"
              action={<Button onClick={() => onNew(date)}>Agregar pendiente</Button>}
            />
          ) : (
            <>
              <p className="text-sm text-muted">
                {d.open_count} abiertas · {d.done_count} hechas
              </p>
              {d.groups
                .filter((g) => g.tasks.length > 0)
                .map((g) => (
                  <Card key={g.area_id ?? "none"} title={g.area_name}>
                    <ul className="divide-y divide-line">
                      {g.tasks.map((t) => (
                        <TaskRow key={t.id} task={t} today={today} onOpen={() => onOpen(t.id)} />
                      ))}
                    </ul>
                  </Card>
                ))}
            </>
          )
        }
      </QueryState>
    </div>
  );
}

function ListView({ onOpen }: { onOpen: (id: string) => void }) {
  const { areas, projects } = useAreasAndProjects();
  const [status, setStatus] = useState("open");
  const [area, setArea] = useState("");
  const [external, setExternal] = useState("");
  const [archived, setArchived] = useState(false);
  const list = useTaskList({
    status: status || undefined,
    area_id: area || undefined,
    external: external === "" ? undefined : external === "1",
    archived,
    limit: 200,
  });
  const columns: Column<Task>[] = [
    { key: "title", header: "Tarea", render: (t) => t.title, sortValue: (t) => t.title },
    {
      key: "area",
      header: "Área",
      render: (t) => areaName(areas, t.area_id),
      sortValue: (t) => areaName(areas, t.area_id),
    },
    {
      key: "project",
      header: "Proyecto",
      render: (t) => projects.find((p) => p.id === t.project_id)?.name ?? "—",
      hideOnCard: true,
    },
    {
      key: "planned",
      header: "Día",
      render: (t) => (t.planned_on ? formatShortDate(t.planned_on) : "—"),
      sortValue: (t) => t.planned_on,
    },
    {
      key: "due",
      header: "Vence",
      render: (t) => (t.due_on ? formatShortDate(t.due_on) : "—"),
      sortValue: (t) => t.due_on,
    },
    {
      key: "prio",
      header: "Prioridad",
      render: (t) => (t.priority ? PRIORITY[t.priority] : "—"),
      sortValue: (t) => t.priority ?? 9,
    },
    { key: "status", header: "Estado", render: (t) => TASK_STATUS[t.status] ?? t.status },
  ];
  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-4 sm:items-end">
        <SelectField label="Estado" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Todos</option>
          {Object.entries(TASK_STATUS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <SelectField label="Área" value={area} onChange={(e) => setArea(e.target.value)}>
          <option value="">Todas</option>
          {areas.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        <SelectField label="Origen" value={external} onChange={(e) => setExternal(e.target.value)}>
          <option value="">Todas</option>
          <option value="0">De la app</option>
          <option value="1">ClickUp</option>
        </SelectField>
        <CheckboxField
          label="Archivadas"
          checked={archived}
          onChange={(e) => setArchived(e.target.checked)}
        />
      </div>
      <QueryState query={list}>
        {(d) => (
          <DataTable
            caption="Tareas"
            rows={d.items}
            columns={columns}
            rowKey={(t) => t.id}
            searchText={(t) => `${t.title} ${t.notes_md}`}
            onRowClick={(t) => onOpen(t.id)}
            emptyTitle="Sin tareas con estos filtros"
            initialSort={{ key: "planned", dir: "asc" }}
          />
        )}
      </QueryState>
    </div>
  );
}

function RuleDialog({ rule, onClose }: { rule: RecurrenceRule | null; onClose: () => void }) {
  const { areas } = useAreasAndProjects();
  const { save } = useRecurrenceMutations();
  const toast = useToast();
  const [form, setForm] = useState({
    title: rule?.title ?? "",
    area_id: rule?.area_id ?? "",
    day_kind: rule?.day_kind ?? "day",
    day: rule?.day ?? 1,
    active: rule?.active ?? true,
  });
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    if (!form.title.trim()) return setError("Escribe el pendiente");
    try {
      await save.mutateAsync({
        id: rule?.id,
        body: {
          title: form.title.trim(),
          area_id: form.area_id || null,
          day_kind: form.day_kind,
          day: form.day_kind === "day" ? form.day : null,
          active: form.active,
          version: rule?.version ?? null,
        },
      });
      toast.show("Recurrente guardado", { tone: "ok" });
      onClose();
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={rule ? "Editar recurrente" : "Nuevo pendiente recurrente"}
      footer={
        <Button onClick={submit} loading={save.isPending}>
          Guardar
        </Button>
      }
    >
      <div className="space-y-3">
        <TextField
          label="Pendiente"
          value={form.title}
          onChange={(e) => setForm({ ...form, title: e.target.value })}
          error={error ?? undefined}
        />
        <SelectField
          label="Área"
          value={form.area_id}
          onChange={(e) => setForm({ ...form, area_id: e.target.value })}
        >
          <option value="">Sin área</option>
          {areas.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        <SelectField
          label="Cuándo"
          value={form.day_kind}
          onChange={(e) => setForm({ ...form, day_kind: e.target.value })}
        >
          <option value="day">Un día del mes</option>
          <option value="last">Último día del mes</option>
          <option value="penultimate">Penúltimo día del mes</option>
        </SelectField>
        {form.day_kind === "day" && (
          <TextField
            label="Día del mes"
            type="number"
            min={1}
            max={31}
            value={form.day}
            onChange={(e) =>
              setForm({ ...form, day: Math.min(31, Math.max(1, Number(e.target.value) || 1)) })
            }
            hint="Si el mes no tiene ese día (p. ej. 31 en abril) se usa el último."
          />
        )}
        <CheckboxField
          label="Activo"
          checked={form.active}
          onChange={(e) => setForm({ ...form, active: e.target.checked })}
        />
        <p className="text-sm text-muted">Editar o desactivar no cambia las tareas ya generadas.</p>
      </div>
    </Dialog>
  );
}

function RecurrencesView() {
  const rules = useRecurrences();
  const { areas } = useAreasAndProjects();
  const { materialize } = useRecurrenceMutations();
  const toast = useToast();
  const [editing, setEditing] = useState<RecurrenceRule | null | "new">(null);
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          variant="secondary"
          loading={materialize.isPending}
          onClick={() =>
            materialize.mutate(undefined, {
              onSuccess: (r) => toast.show(`${r.created} pendientes generados`, { tone: "ok" }),
              onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
            })
          }
        >
          Generar los de hoy
        </Button>
        <Button onClick={() => setEditing("new")}>Nuevo recurrente</Button>
      </div>
      <QueryState
        query={rules}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState title="Sin pendientes recurrentes">
            Ej.: facturas el día 14 y el penúltimo día de cada mes.
          </EmptyState>
        }
      >
        {(d) => (
          <ul className="divide-y divide-line rounded-xl border border-line bg-surface">
            {d.map((r) => (
              <li key={r.id} className="flex flex-wrap items-center justify-between gap-2 p-3">
                <div className="min-w-0">
                  <p className="font-medium">{r.title}</p>
                  <p className="text-sm text-muted">
                    {ruleLabel(r)} · {areaName(areas, r.area_id)}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  {!r.active && <Badge tone="warn">Inactivo</Badge>}
                  <Button size="sm" variant="secondary" onClick={() => setEditing(r)}>
                    Editar
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
      {editing && <RuleDialog rule={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

export function TasksPage() {
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") as Tab) || "dia";
  const openTask = params.get("task");
  const [creating, setCreating] = useState<string | null>(null);
  const setParam = (k: string, v: string | null) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    setParams(p, { replace: true });
  };
  const tabs: [Tab, string][] = [
    ["dia", "Por día"],
    ["lista", "Todas"],
    ["recurrentes", "Recurrentes"],
  ];
  return (
    <div className="space-y-4">
      <PageHeader title="Tareas" subtitle="Pendientes por área; los no hechos se arrastran sin duplicarse" />
      <div role="tablist" aria-label="Vista de tareas" className="flex flex-wrap gap-2">
        {tabs.map(([k, label]) => (
          <Button
            key={k}
            role="tab"
            aria-selected={tab === k}
            variant={tab === k ? "primary" : "secondary"}
            size="sm"
            onClick={() => setParam("tab", k)}
          >
            {label}
          </Button>
        ))}
      </div>
      {tab === "dia" && <DayView onOpen={(id) => setParam("task", id)} onNew={(d) => setCreating(d)} />}
      {tab === "lista" && <ListView onOpen={(id) => setParam("task", id)} />}
      {tab === "recurrentes" && <RecurrencesView />}
      {openTask && <TaskDialog open taskId={openTask} onClose={() => setParam("task", null)} />}
      {creating && (
        <TaskDialog
          open
          taskId={null}
          defaults={{ planned_on: creating }}
          onClose={() => setCreating(null)}
        />
      )}
    </div>
  );
}
