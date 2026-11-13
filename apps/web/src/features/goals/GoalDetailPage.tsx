// Detalle de objetivo: datos, progreso (eventos manuales y aportes de fuentes enlazadas), enlaces explícitos
// (lectura, tarea, contenido, deuda), hábitos que lo apoyan, relación semana → mes, cierre y archivado.
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { HabitFormDialog } from "../habits/HabitForm";
import { CloseGoalDialog } from "./CloseGoalDialog";
import { GoalCard } from "./GoalCard";
import { GoalFormDialog } from "./GoalForm";
import { type GoalClosing, type GoalDetail, goalToBody, useGoal, useGoalMutations } from "./goalsApi";

const LINK_TYPES: Record<string, string> = {
  reading: "Lectura",
  task: "Tarea",
  content: "Contenido",
  debt: "Deuda",
};

export function GoalDetailPage() {
  const { id = "" } = useParams();
  const goal = useGoal(id);
  return (
    <div className="space-y-4">
      <p>
        <Link to="/objetivos">← Objetivos</Link>
      </p>
      <QueryState query={goal}>{(g) => <Detail d={g} />}</QueryState>
    </div>
  );
}

function Detail({ d }: { d: GoalDetail }) {
  const g = d.goal;
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const m = useGoalMutations();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [closing, setClosing] = useState(false);
  const [habit, setHabit] = useState(false);
  const closingData = (g.closing ?? null) as GoalClosing | null;

  const run = async (p: Promise<unknown>, ok: string) => {
    try {
      await p;
      toast.show(ok, { tone: "ok" });
    } catch (e) {
      toast.show(
        e instanceof ApiError && e.isConflict ? "Cambió en otro dispositivo; recarga" : errorMessage(e),
        {
          tone: "error",
        },
      );
    }
  };

  return (
    <>
      <PageHeader
        title={g.title}
        subtitle={`${g.kind === "month" ? "Objetivo del mes" : "Objetivo de la semana"} ${g.period_key} · ${g.starts_on} → ${g.ends_on}`}
        actions={
          <>
            <Button variant="secondary" onClick={() => setEditing(true)}>
              Editar
            </Button>
            <Button onClick={() => setClosing(true)}>
              {g.status === "active" ? "Cerrar objetivo" : "Editar cierre"}
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                void run(
                  m.update.mutateAsync({
                    id: g.id,
                    body: goalToBody(g, { archived: g.archived_at == null }),
                  }),
                  g.archived_at ? "Recuperado" : "Archivado",
                )
              }
            >
              {g.archived_at ? "Recuperar" : "Archivar"}
            </Button>
          </>
        }
      />
      <GoalCard view={d} />
      <div className="grid gap-4 md:grid-cols-2">
        <Card title="Por qué y cómo sé que lo logré">
          <p className="text-sm">
            <span className="text-muted">Por qué importa: </span>
            {g.motivation || "—"}
          </p>
          <p className="mt-2 text-sm">
            <span className="text-muted">Criterio: </span>
            {g.success_criteria || "—"}
          </p>
          {d.parent && (
            <p className="mt-2 text-sm">
              Apoya a: <Link to={`/objetivos/${d.parent.id}`}>{d.parent.title}</Link>
            </p>
          )}
          {d.children.length > 0 && (
            <div className="mt-2 text-sm">
              <p className="text-muted">Objetivos semanales que lo apoyan:</p>
              <ul className="list-disc pl-5">
                {d.children.map((c) => (
                  <li key={c.id}>
                    <Link to={`/objetivos/${c.id}`}>{c.title}</Link> ({c.period_key})
                  </li>
                ))}
              </ul>
            </div>
          )}
        </Card>
        {closingData && (
          <Card title="Cierre">
            <dl className="space-y-1 text-sm">
              {(
                [
                  ["achievement", "Logro"],
                  ["learning", "Aprendizaje"],
                  ["difficulties", "Dificultades"],
                  ["adjustments", "Próximos ajustes"],
                ] as const
              ).map(([k, label]) => (
                <div key={k}>
                  <dt className="text-muted">{label}</dt>
                  <dd>{closingData[k] || "—"}</dd>
                </div>
              ))}
            </dl>
          </Card>
        )}
      </div>

      {g.progress_type !== "none" && <ProgressCard d={d} today={today} run={run} />}
      <LinksCard d={d} run={run} />

      <Card
        title="Hábitos que lo apoyan"
        actions={
          <Button size="sm" onClick={() => setHabit(true)}>
            Agregar hábito
          </Button>
        }
      >
        {d.habits.length === 0 ? (
          <p className="text-sm text-muted">
            Propón de 1 a 3 acciones diarias que empujen directo a este objetivo.
          </p>
        ) : (
          <ul className="space-y-1 text-sm">
            {d.habits.map((h) => (
              <li key={h.id}>
                <Link to={`/habitos/${h.id}`}>{h.name}</Link>
                {h.unit && <span className="text-muted"> ({h.unit})</span>}
              </li>
            ))}
          </ul>
        )}
        {d.habits.length >= 3 && (
          <p className="mt-2 text-sm text-warn">Ya hay 3 o más hábitos: cuida que sea realista.</p>
        )}
      </Card>

      {editing && <GoalFormDialog open onClose={() => setEditing(false)} goal={g} periodKey={g.period_key} />}
      {closing && <CloseGoalDialog goal={g} open onClose={() => setClosing(false)} />}
      {habit && (
        <HabitFormDialog
          open
          onClose={() => setHabit(false)}
          goalId={g.id}
          defaultStartsOn={g.starts_on.localeCompare(today) > 0 ? g.starts_on : today}
        />
      )}
    </>
  );
}

function ProgressCard({
  d,
  today,
  run,
}: {
  d: GoalDetail;
  today: string;
  run: (p: Promise<unknown>, ok: string) => Promise<void>;
}) {
  const g = d.goal;
  const m = useGoalMutations();
  const [on, setOn] = useState(today);
  const [num, setNum] = useState("");
  const [note, setNote] = useState("");
  const isCum = g.progress_type === "cumulative";
  const isMilestone = g.progress_type === "milestone";
  const add = () => {
    const v = num.trim().replace(",", ".");
    if (!isMilestone && !/^-?\d+(\.\d{1,3})?$/.test(v))
      return void run(Promise.reject(new Error("Escribe un número válido")), "");
    void run(
      m.addProgress.mutateAsync({
        id: g.id,
        body: {
          occurred_on: on,
          note,
          ...(isMilestone ? { completed: true } : isCum ? { amount: v } : { value: v }),
        },
      }),
      "Progreso registrado",
    );
    setNum("");
    setNote("");
  };
  return (
    <Card title="Progreso del resultado">
      <p className="mb-2 text-sm text-muted">
        {isCum
          ? `Cada registro suma a la meta (${g.unit}). Una misma fuente aporta una sola vez a cada objetivo.`
          : isMilestone
            ? "Marca el hito cuando se complete."
            : `Mediciones de valor actual (${g.unit}); cuenta la última, sin proyección lineal.`}
      </p>
      {d.events.length > 0 && (
        <ul className="mb-3 divide-y divide-line text-sm">
          {d.events.map((e) => (
            <li key={e.id} className="flex flex-wrap items-center gap-2 py-1.5">
              <span className="tabular-nums">{e.occurred_on}</span>
              <span className="font-medium">
                {e.completed
                  ? "Hito completado"
                  : `${isCum ? "+" : ""}${Number(e.amount ?? e.value)} ${g.unit ?? ""}`}
              </span>
              {e.source_type !== "manual" && <Badge>{LINK_TYPES[e.source_type] ?? e.source_type}</Badge>}
              {e.note && <span className="text-muted">{e.note}</span>}
              {e.source_type === "manual" && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="ml-auto"
                  onClick={() =>
                    void run(m.deleteProgress.mutateAsync({ id: g.id, eventId: e.id }), "Registro eliminado")
                  }
                >
                  Quitar
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="grid gap-2 sm:grid-cols-[10rem_8rem_1fr_auto] sm:items-end">
        <TextField label="Fecha" type="date" max={today} value={on} onChange={(e) => setOn(e.target.value)} />
        {!isMilestone && (
          <TextField
            label={isCum ? `Cantidad (${g.unit})` : `Valor (${g.unit})`}
            inputMode="decimal"
            value={num}
            onChange={(e) => setNum(e.target.value)}
          />
        )}
        <TextField label="Nota" value={note} onChange={(e) => setNote(e.target.value)} />
        <Button onClick={add} loading={m.addProgress.isPending}>
          {isMilestone ? "Marcar hito" : "Registrar"}
        </Button>
      </div>
    </Card>
  );
}

function LinksCard({ d, run }: { d: GoalDetail; run: (p: Promise<unknown>, ok: string) => Promise<void> }) {
  const g = d.goal;
  const m = useGoalMutations();
  const [type, setType] = useState("reading");
  const [src, setSrc] = useState("");
  const options = useQuery({
    queryKey: ["link-options", type],
    queryFn: async (): Promise<{ id: string; label: string }[]> => {
      if (type === "reading")
        return unwrap(await api.GET("/readings", { params: { query: {} } })).map((r) => ({
          id: r.id,
          label: `${r.title} (${r.status})`,
        }));
      if (type === "task")
        return unwrap(
          await api.GET("/tasks", { params: { query: { status: "open", limit: 200 } } }),
        ).items.map((t) => ({ id: t.id, label: t.title }));
      if (type === "content")
        return unwrap(await api.GET("/content", { params: { query: {} } })).map((c) => ({
          id: c.id,
          label: `${c.title} (${c.status})`,
        }));
      return unwrap(await api.GET("/finance/debts")).map((x) => ({ id: x.debt.id, label: x.debt.name }));
    },
  });
  const labelOf = (srcType: string, id: string) =>
    srcType === type ? (options.data?.find((o) => o.id === id)?.label ?? id.slice(0, 8)) : id.slice(0, 8);
  return (
    <Card title="Enlaces que aportan">
      <p className="mb-2 text-sm text-muted">
        Relación explícita: terminar una lectura suma 1 a un objetivo de libros; saldar una deuda completa un
        hito. Un abono parcial no lo completa. Sin búsquedas por texto.
      </p>
      {d.links.length > 0 && (
        <ul className="mb-3 space-y-1 text-sm">
          {d.links.map((l) => (
            <li key={`${l.src_type}-${l.src_id}`} className="flex flex-wrap items-center gap-2">
              <Badge>{LINK_TYPES[l.src_type] ?? l.src_type}</Badge>
              <span>{labelOf(l.src_type, l.src_id)}</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void run(
                    m.removeLink.mutateAsync({ id: g.id, srcType: l.src_type, srcId: l.src_id }),
                    "Enlace quitado",
                  )
                }
              >
                Quitar
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="grid gap-2 sm:grid-cols-[10rem_1fr_auto] sm:items-end">
        <SelectField
          label="Tipo"
          value={type}
          onChange={(e) => {
            setType(e.target.value);
            setSrc("");
          }}
        >
          {Object.entries(LINK_TYPES).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <SelectField label="Elemento" value={src} onChange={(e) => setSrc(e.target.value)}>
          <option value="">
            {options.isPending ? "Cargando…" : options.isError ? "No disponible" : "Elige…"}
          </option>
          {(options.data ?? []).map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))}
        </SelectField>
        <Button
          disabled={!src}
          onClick={() =>
            void run(m.addLink.mutateAsync({ id: g.id, body: { src_type: type, src_id: src } }), "Enlazado")
          }
        >
          Enlazar
        </Button>
      </div>
    </Card>
  );
}
