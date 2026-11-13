// Revisión semanal: resumen calculado (fuentes enlazadas) + texto editable, paz 1–5 aportada por el usuario,
// tres prioridades, cierre de objetivos como acción separada y paso a planificar la semana siguiente.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, TextArea, TextField } from "../../components/ui/Field";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { formatMoney, formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { CloseGoalDialog } from "../goals/CloseGoalDialog";
import { GoalCard } from "../goals/GoalCard";
import type { Goal } from "../goals/goalsApi";
import { formatWeek, isoWeekKey, isValidWeekKey, shiftWeek } from "../goals/periods";
import { asCompliance } from "../habits/habitsApi";
import { formatNumber, formatPct } from "../today/numbers";

type Body = Schemas["WeeklyBody"];
type Review = Schemas["WeeklyReview"];

const TEXT_FIELDS: { key: keyof Body & keyof Review; label: string; rows?: number }[] = [
  { key: "what_happened", label: "Qué pasó" },
  { key: "what_didnt", label: "Qué no pasó (y por qué)" },
  { key: "content_md", label: "Contenido publicado / métricas", rows: 3 },
  { key: "habits_md", label: "Hábitos y devocional", rows: 3 },
  { key: "goals_md", label: "Objetivos y adherencia", rows: 3 },
  { key: "went_badly", label: "Patrones: lo que salió mal", rows: 3 },
  { key: "went_well", label: "Patrones: lo que salió bien", rows: 3 },
  { key: "areas_md", label: "Proyectos por área", rows: 3 },
  { key: "people_md", label: "Personas con las que hablé", rows: 3 },
];

export function formFromReview(r: Review | null | undefined): Required<
  Pick<
    Body,
    | "what_happened"
    | "what_didnt"
    | "content_md"
    | "habits_md"
    | "goals_md"
    | "went_well"
    | "went_badly"
    | "areas_md"
    | "people_md"
  >
> & {
  peace: number | null;
  priorities: { text: string; reason: string }[];
  completed: boolean;
} {
  const pr = ((r?.priorities ?? []) as unknown as { text?: string; reason?: string }[]).map((p) => ({
    text: p.text ?? "",
    reason: p.reason ?? "",
  }));
  while (pr.length < 3) pr.push({ text: "", reason: "" });
  return {
    what_happened: r?.what_happened ?? "",
    what_didnt: r?.what_didnt ?? "",
    content_md: r?.content_md ?? "",
    habits_md: r?.habits_md ?? "",
    goals_md: r?.goals_md ?? "",
    went_well: r?.went_well ?? "",
    went_badly: r?.went_badly ?? "",
    areas_md: r?.areas_md ?? "",
    people_md: r?.people_md ?? "",
    peace: r?.peace ?? null,
    priorities: pr.slice(0, 3),
    completed: r?.completed_at != null,
  };
}

/** Texto sugerido a partir del resumen calculado (el usuario lo edita; no se guarda solo). */
export function suggestFromSummary(s: Schemas["WeeklySummary"]): Partial<Record<string, string>> {
  const bullets = (xs: string[]) => xs.map((x) => `- ${x}`).join("\n");
  return {
    what_happened: bullets(s.tasks_done.map((t) => `${t.title}${t.area_name ? ` (${t.area_name})` : ""}`)),
    what_didnt: bullets(s.tasks_not_done.map((t) => `${t.title} — ¿por qué?`)),
    content_md: bullets(
      s.content_published.map(
        (c) =>
          `${c.title}${c.views !== null && c.views !== undefined ? ` · ${c.views} views` : ""}${c.retention_pct ? ` · ${c.retention_pct}% retención` : ""}`,
      ),
    ),
    habits_md: bullets(s.fixed_habits.map((h) => `${h.name}: ${h.done}/${h.target}`)),
    goals_md: bullets(
      s.goals.map((g) => {
        const a = asCompliance(g.habit_adherence);
        return `${g.goal.title}: hábitos ${formatPct(a.rate)}${g.goal.status !== "active" ? ` · ${g.goal.status === "achieved" ? "logrado" : "no logrado"}` : ""}`;
      }),
    ),
    went_well: bullets(s.went_well.map((r) => r.text)),
    went_badly: bullets(s.went_badly.map((r) => r.text)),
    areas_md: bullets(s.areas.map((a) => `${a.area_name}: ${a.done} hechas, ${a.open} abiertas`)),
    people_md: bullets(s.people.map((p) => `${p.person_name}${p.topic ? `: ${p.topic}` : ""}`)),
  };
}

export function WeeklyPage() {
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const [params, setParams] = useSearchParams();
  const week = isValidWeekKey(params.get("week")) ? (params.get("week") as string) : isoWeekKey(today);
  const summary = useQuery({
    queryKey: ["weekly", "summary", week],
    queryFn: async () =>
      unwrap(await api.GET("/weekly-reviews/{iso_week}/summary", { params: { path: { iso_week: week } } })),
  });
  const review = useQuery({
    queryKey: ["weekly", "review", week],
    queryFn: async () =>
      unwrap(await api.GET("/weekly-reviews/{iso_week}", { params: { path: { iso_week: week } } })),
  });
  return (
    <div className="space-y-4">
      <PageHeader
        title="Revisión semanal"
        subtitle={formatWeek(week)}
        actions={
          <>
            <Button
              variant="secondary"
              size="sm"
              aria-label="Semana anterior"
              onClick={() => setParams({ week: shiftWeek(week, -1) })}
            >
              ‹
            </Button>
            <Button
              variant="secondary"
              size="sm"
              aria-label="Semana siguiente"
              onClick={() => setParams({ week: shiftWeek(week, 1) })}
            >
              ›
            </Button>
          </>
        }
      />
      {(summary.isPending || review.isPending) && <LoadingState rows={5} />}
      {summary.isError && <ErrorState error={summary.error} onRetry={() => summary.refetch()} />}
      {review.isError && <ErrorState error={review.error} onRetry={() => review.refetch()} />}
      {summary.data && review.data && (
        <Editor
          key={`${week}-${review.data.version}`}
          week={week}
          summary={summary.data}
          view={review.data}
          currency={me?.settings.currency ?? "MXN"}
        />
      )}
    </div>
  );
}

function Editor({
  week,
  summary: s,
  view,
  currency,
}: {
  week: string;
  summary: Schemas["WeeklySummary"];
  view: Schemas["WeeklyReviewView"];
  currency: string;
}) {
  const qc = useQueryClient();
  const toast = useToast();
  const [f, setF] = useState(() => formFromReview(view.review));
  const [closing, setClosing] = useState<Goal | null>(null);
  const [conflict, setConflict] = useState<{ mine: Body; current: Review } | null>(null);
  const save = useMutation({
    mutationFn: async (body: Body) =>
      unwrap(await api.PUT("/weekly-reviews/{iso_week}", { params: { path: { iso_week: week } }, body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["weekly"] }),
  });

  const body = (): Body => ({
    ...Object.fromEntries(TEXT_FIELDS.map((t) => [t.key, f[t.key as keyof typeof f] as string])),
    peace: f.peace,
    priorities: f.priorities
      .filter((p) => p.text.trim())
      .map((p) => ({ text: p.text.trim(), reason: p.reason.trim() })),
    completed: f.completed,
    version: view.version,
  });

  const submit = async (b: Body = body()) => {
    try {
      await save.mutateAsync(b);
      toast.show("Revisión guardada", { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) {
        const cur = (e.current as { review?: Review }).review ?? (e.current as Review);
        setConflict({ mine: b, current: cur });
      } else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const fill = () => {
    const sug = suggestFromSummary(s);
    setF((x) => {
      const n = { ...x } as Record<string, unknown>;
      for (const [k, v] of Object.entries(sug)) if (v && !String(n[k] ?? "").trim()) n[k] = v;
      return n as typeof x;
    });
    toast.show("Completé los campos vacíos con el resumen. Revísalos antes de guardar.");
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {s.fixed_habits.map((h) => (
          <div key={h.habit_id} className="rounded-xl border border-line bg-surface p-3">
            <p className="text-sm text-muted">{h.name}</p>
            <p className="text-2xl font-semibold tabular-nums">
              {h.done}/{h.target}
            </p>
          </div>
        ))}
        <div className="rounded-xl border border-line bg-surface p-3">
          <p className="text-sm text-muted">Gasto de la semana</p>
          <p className="text-2xl font-semibold tabular-nums">
            {formatMoney(s.expenses_total_cents, currency)}
          </p>
          <p className="text-xs text-muted">
            {s.expenses_count} movimientos · energía prom. {formatNumber(s.energy_avg, 1)}
          </p>
        </div>
      </div>
      {s.is_open && (
        <p className="text-xs text-muted">
          La semana sigue en curso: hoy y los días futuros no cuentan como faltantes.
        </p>
      )}

      <Card
        title="Objetivos de la semana"
        actions={<Link to={view.planning_route}>Planificar {view.next_week} →</Link>}
      >
        {s.goals.length === 0 ? (
          <p className="text-sm text-muted">Sin objetivos esta semana.</p>
        ) : (
          <ul className="grid gap-3 md:grid-cols-2">
            {s.goals.map((g) => (
              <li key={g.goal.id}>
                <GoalCard view={g} compact />
                {g.goal.status === "active" && (
                  <Button size="sm" variant="secondary" className="mt-1" onClick={() => setClosing(g.goal)}>
                    Cerrar objetivo
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-2 text-xs text-muted">
          Cerrar un objetivo es una decisión aparte; guardar la revisión no lo cierra.
        </p>
      </Card>

      <Card
        title="Tu revisión"
        actions={
          <Button size="sm" variant="secondary" onClick={fill}>
            Prellenar con el resumen
          </Button>
        }
      >
        <fieldset className="mb-4">
          <legend className="mb-1 text-sm font-medium">Paz de la semana (1–5, tú decides)</legend>
          <div className="flex flex-wrap gap-1.5">
            {[1, 2, 3, 4, 5].map((n) => (
              <label key={n}>
                <input
                  type="radio"
                  name="peace"
                  className="peer sr-only"
                  checked={f.peace === n}
                  onChange={() => setF({ ...f, peace: n })}
                />
                <span className="flex h-11 w-11 cursor-pointer items-center justify-center rounded-lg border border-line text-lg peer-checked:bg-accent peer-checked:text-accent-fg peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent">
                  {n}
                </span>
              </label>
            ))}
            {f.peace !== null && (
              <Button variant="ghost" size="sm" onClick={() => setF({ ...f, peace: null })}>
                Sin dato
              </Button>
            )}
          </div>
        </fieldset>
        <div className="grid gap-3 md:grid-cols-2">
          {TEXT_FIELDS.map((t) => (
            <TextArea
              key={t.key}
              label={t.label}
              rows={t.rows ?? 4}
              value={f[t.key as keyof typeof f] as string}
              onChange={(e) => setF({ ...f, [t.key]: e.target.value })}
            />
          ))}
        </div>
        <fieldset className="mt-4">
          <legend className="mb-2 text-sm font-medium">Prioridades de la próxima semana (hasta 3)</legend>
          <div className="space-y-2">
            {f.priorities.map((p, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: tres posiciones fijas
              <div key={i} className="grid gap-2 sm:grid-cols-2">
                <TextField
                  label={`${i + 1}. Prioridad`}
                  value={p.text}
                  onChange={(e) =>
                    setF({
                      ...f,
                      priorities: f.priorities.map((x, j) => (j === i ? { ...x, text: e.target.value } : x)),
                    })
                  }
                />
                <TextField
                  label="Por qué (una línea)"
                  value={p.reason}
                  onChange={(e) =>
                    setF({
                      ...f,
                      priorities: f.priorities.map((x, j) =>
                        j === i ? { ...x, reason: e.target.value } : x,
                      ),
                    })
                  }
                />
              </div>
            ))}
          </div>
        </fieldset>
        <CheckboxField
          className="mt-4"
          label="Revisión terminada"
          checked={f.completed}
          onChange={(e) => setF({ ...f, completed: e.target.checked })}
        />
        <div className="mt-4 flex flex-wrap gap-2">
          <Button onClick={() => void submit()} loading={save.isPending}>
            Guardar revisión
          </Button>
          <ButtonLink to={view.planning_route}>Planificar la semana siguiente</ButtonLink>
        </div>
      </Card>

      <Sources s={s} currency={currency} />

      {closing && <CloseGoalDialog goal={closing} open onClose={() => setClosing(null)} />}
      {conflict && (
        <ConflictDialog
          open
          mine={conflict.mine as Record<string, unknown>}
          theirs={conflict.current as unknown as Record<string, unknown>}
          onClose={() => setConflict(null)}
          onResolve={(r) => {
            const c = conflict;
            setConflict(null);
            if (r.choice === "theirs") {
              void qc.invalidateQueries({ queryKey: ["weekly"] });
              return;
            }
            const b = r.choice === "merged" ? (r.body as Body) : c.mine;
            void submit({ ...b, version: c.current.version });
          }}
        />
      )}
    </div>
  );
}

function Sources({ s, currency }: { s: Schemas["WeeklySummary"]; currency: string }) {
  return (
    <Card title="Resumen calculado y fuentes">
      <div className="grid gap-4 md:grid-cols-2">
        <List
          title="Pendientes hechos"
          items={s.tasks_done.map((t) => ({ k: t.id, text: t.title, sub: t.area_name ?? "" }))}
        />
        <List
          title="Planeado y aún abierto"
          items={s.tasks_not_done.map((t) => ({ k: t.id, text: t.title, sub: t.area_name ?? "" }))}
        />
        <List
          title="Lo que salió bien"
          items={s.went_well.map((r) => ({
            k: r.id,
            text: r.text,
            sub: formatShortDate(r.date),
            to: `/hoy?date=${r.date}`,
          }))}
        />
        <List
          title="Lo que salió mal"
          items={s.went_badly.map((r) => ({
            k: r.id,
            text: r.text,
            sub: formatShortDate(r.date),
            to: `/hoy?date=${r.date}`,
          }))}
        />
        <List
          title="Personas"
          items={s.people.map((p) => ({
            k: p.id,
            text: p.person_name,
            sub: p.topic,
            to: `/personas/${p.person_id}`,
          }))}
        />
        <List
          title="Contenido publicado"
          items={s.content_published.map((c) => ({
            k: c.id,
            text: c.title,
            sub: c.views !== null && c.views !== undefined ? `${c.views} views` : "",
            to: "/contenido",
          }))}
        />
        <List
          title="Devlog"
          items={s.devlog.map((d) => ({
            k: d.id,
            text: d.repo || "sesión",
            sub: formatShortDate(d.on_date),
            to: `/dev?date=${d.on_date}`,
          }))}
        />
        <List
          title="Devocionales"
          items={s.devotionals.map((d) => ({
            k: d.date,
            text: d.passage || "(sin pasaje)",
            sub: d.actions.join("; "),
            to: `/devocionales/${d.date}`,
          }))}
        />
      </div>
      <p className="mt-3 text-sm">
        Inbox pendiente: <Link to="/inbox">{s.inbox_pending}</Link> · Gasto:{" "}
        {formatMoney(s.expenses_total_cents, currency)}
      </p>
      {s.evidence.length > 0 && (
        <details className="mt-3">
          <summary className="cursor-pointer text-sm font-medium">Evidencias ({s.evidence.length})</summary>
          <ul className="mt-2 flex flex-wrap gap-1.5">
            {s.evidence.map((e) => (
              <li key={`${e.entity_type}-${e.entity_id ?? e.label}`}>
                <Link to={e.route} className="no-underline">
                  <Badge>{e.label}</Badge>
                </Link>
              </li>
            ))}
          </ul>
        </details>
      )}
    </Card>
  );
}

function List({
  title,
  items,
}: {
  title: string;
  items: { k: string; text: string; sub?: string; to?: string }[];
}) {
  return (
    <section>
      <h3 className="text-sm font-semibold">{title}</h3>
      {items.length === 0 ? (
        <p className="text-sm text-muted">—</p>
      ) : (
        <ul className="mt-1 space-y-0.5 text-sm">
          {items.map((i) => (
            <li key={i.k} className="break-words">
              {i.to ? <Link to={i.to}>{i.text}</Link> : i.text}
              {i.sub && <span className="text-muted"> · {i.sub}</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
