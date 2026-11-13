// Hábitos: fijos, del mes, de la semana y personales; estado de hoy, rachas, cumplimiento cerrado de 30 días,
// mapa de calor (vista "Últimos 30 días" de Hábitos.base) y vínculos explícitos acción → hábito.
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { addDays, formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { formatPct } from "../today/numbers";
import { HabitFormDialog } from "./HabitForm";
import {
  asCompliance,
  asStreaks,
  asToday,
  CELL_STATE,
  type Compliance,
  describeFrequency,
  type HabitSummary,
  LINK_TRIGGERS,
  type Streaks,
  useHabitHistory,
  useHabitLinks,
  useHabitMutations,
  useHabits,
} from "./habitsApi";

export function streakText(s: Streaks): string {
  const u = s.unit === "weeks" ? "sem" : "días";
  return `Racha ${s.current} ${u} · máx. ${s.longest}`;
}

/** "86% (19/22) · 2 sin registro". Sin ocurrencias exigibles → "Sin días exigibles". */
export function complianceText(c: Compliance): string {
  if (!c.required) return "Sin días exigibles";
  const parts = [`${formatPct(c.rate)} (${c.done}/${c.required})`];
  if (c.unrecorded) parts.push(`${c.unrecorded} sin registro`);
  if (c.missed) parts.push(`${c.missed} no`);
  if (c.excluded_not_applicable) parts.push(`${c.excluded_not_applicable} no aplica`);
  return parts.join(" · ");
}

const GROUPS: { key: string; title: string; match: (h: HabitSummary) => boolean }[] = [
  { key: "fixed", title: "Fijos", match: (h) => h.habit.kind === "fixed" },
  { key: "month", title: "Del mes", match: (h) => h.habit.kind === "goal" && h.goal_kind === "month" },
  { key: "week", title: "De la semana", match: (h) => h.habit.kind === "goal" && h.goal_kind === "week" },
  {
    key: "other",
    title: "Personales",
    match: (h) => h.habit.kind === "custom" || (h.habit.kind === "goal" && !h.goal_kind),
  },
];

export function HabitsPage() {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const [showArchived, setShowArchived] = useState(false);
  const habits = useHabits(today, showArchived);
  const [creating, setCreating] = useState(false);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Hábitos"
        subtitle="Cumplimiento cerrado (sin hoy) · hoy se muestra aparte"
        actions={
          <>
            <Button variant="secondary" onClick={() => setShowArchived((v) => !v)}>
              {showArchived ? "Ocultar archivados" : "Ver archivados"}
            </Button>
            <Button onClick={() => setCreating(true)}>Nuevo hábito</Button>
          </>
        }
      />
      <QueryState
        query={habits}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Aún no tienes hábitos"
            action={
              <div className="flex flex-wrap justify-center gap-2">
                <Link to="/onboarding">Configurar hábitos fijos</Link>
                <Link to="/objetivos/planificar">Planificar objetivos</Link>
              </div>
            }
          >
            Los fijos (devocional, ejercicio, lectura) se proponen en la configuración; los de objetivos, al
            planificar.
          </EmptyState>
        }
      >
        {(list) => (
          <div className="space-y-4">
            {GROUPS.map((g) => {
              const items = list.filter(g.match);
              if (!items.length) return null;
              return (
                <Card key={g.key} title={g.title}>
                  <ul className="divide-y divide-line">
                    {items.map((h) => (
                      <HabitRow key={h.habit.id} h={h} />
                    ))}
                  </ul>
                </Card>
              );
            })}
          </div>
        )}
      </QueryState>
      <HistoryCard from={addDays(today, -29)} to={today} />
      <LinksCard habits={habits.data ?? []} />
      <HabitFormDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function HabitRow({ h }: { h: HabitSummary }) {
  const t = asToday(h.today);
  const s = asStreaks(h.streaks);
  const c = asCompliance(h.last_30_days);
  const todayLabel =
    t.status === "done"
      ? { text: "Hoy: cumplido", tone: "ok" as const }
      : t.status === "not_done"
        ? { text: "Hoy: no", tone: "danger" as const }
        : t.status === "not_applicable"
          ? { text: "Hoy: no aplica", tone: "muted" as const }
          : t.required_today
            ? { text: "Hoy: pendiente", tone: "warn" as const }
            : { text: "Hoy: no exigible", tone: "muted" as const };
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2.5">
      <div className="min-w-0 flex-1">
        <Link to={`/habitos/${h.habit.id}`} className="font-medium">
          {h.habit.name}
        </Link>
        {h.habit.archived_at && <Badge>archivado</Badge>}
        <p className="text-sm text-muted">
          {describeFrequency(h.current_version)}
          {h.goal_title && ` · ${h.goal_title}`}
          {t.week_target ? ` · ${t.week_done ?? 0}/${t.week_target} esta semana` : ""}
        </p>
      </div>
      <Badge tone={todayLabel.tone}>{todayLabel.text}</Badge>
      <span className="text-sm tabular-nums">{streakText(s)}</span>
      <span className="w-full text-sm text-muted sm:w-auto">30 días: {complianceText(c)}</span>
    </li>
  );
}

function HistoryCard({ from, to }: { from: string; to: string }) {
  const history = useHabitHistory(from, to);
  return (
    <Card title="Últimos 30 días">
      <QueryState
        query={history}
        isEmpty={(d) => d.habits.length === 0}
        empty={<p className="text-sm text-muted">Sin hábitos.</p>}
      >
        {(h) => (
          <>
            <div className="overflow-x-auto">
              <table className="border-separate border-spacing-0.5 text-xs">
                <caption className="sr-only">
                  Estado de cada hábito por día, del más reciente al más antiguo
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="sticky left-0 bg-surface pr-2 text-left font-medium">
                      Día
                    </th>
                    {h.habits.map((x) => (
                      <th
                        key={x.id}
                        scope="col"
                        className="max-w-24 truncate px-1 text-left font-medium"
                        title={x.name}
                      >
                        {x.name}
                      </th>
                    ))}
                    <th scope="col" className="px-1 text-left font-medium">
                      Total
                    </th>
                    <th scope="col" className="px-1 text-left font-medium">
                      Energía
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {h.days.map((d) => (
                    <tr key={d.date}>
                      <th
                        scope="row"
                        className="sticky left-0 bg-surface pr-2 text-left font-normal whitespace-nowrap"
                      >
                        <Link to={`/hoy?date=${d.date}`}>{formatShortDate(d.date)}</Link>
                      </th>
                      {h.habits.map((x) => {
                        const cell = d.cells.find((c) => c.habit_id === x.id);
                        const st = CELL_STATE[cell?.state ?? "not_required"] ?? CELL_STATE.not_required;
                        return (
                          <td key={x.id}>
                            <span
                              title={`${x.name}: ${st?.label}`}
                              className={`flex h-7 w-7 items-center justify-center rounded ${st?.className}`}
                            >
                              <span aria-hidden>{st?.symbol}</span>
                              <span className="sr-only">{st?.label}</span>
                            </span>
                          </td>
                        );
                      })}
                      <td className="px-1 tabular-nums">{d.required ? `${d.done}/${d.required}` : "—"}</td>
                      <td className="px-1 tabular-nums">{d.energy ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <ul className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs" aria-label="Leyenda">
              {Object.entries(CELL_STATE).map(([k, v]) => (
                <li key={k} className="flex items-center gap-1">
                  <span
                    className={`flex h-5 w-5 items-center justify-center rounded ${v.className}`}
                    aria-hidden
                  >
                    {v.symbol}
                  </span>
                  {v.label}
                </li>
              ))}
            </ul>
          </>
        )}
      </QueryState>
    </Card>
  );
}

function LinksCard({ habits }: { habits: HabitSummary[] }) {
  const links = useHabitLinks();
  const { setLink, removeLink } = useHabitMutations();
  const toast = useToast();
  const active = habits.filter((h) => !h.habit.archived_at);
  return (
    <Card title="Vínculos automáticos">
      <p className="mb-3 text-sm text-muted">
        Explícitos y configurables: cada acción marca un solo hábito ese día. Nunca se deducen del texto del
        hábito.
      </p>
      <QueryState query={links}>
        {(ls) => (
          <div className="grid gap-3 sm:grid-cols-2">
            {LINK_TRIGGERS.map((t) => {
              const current = ls.find((l) => l.trigger === t.trigger);
              return (
                <SelectField
                  key={t.trigger}
                  label={t.label}
                  hint={t.hint}
                  value={current?.habit_id ?? ""}
                  onChange={async (e) => {
                    try {
                      if (e.target.value)
                        await setLink.mutateAsync({ trigger: t.trigger, habitId: e.target.value });
                      else await removeLink.mutateAsync(t.trigger);
                      await links.refetch();
                      toast.show("Vínculo actualizado", { tone: "ok" });
                    } catch (err) {
                      toast.show(errorMessage(err), { tone: "error" });
                    }
                  }}
                >
                  <option value="">Sin vínculo</option>
                  {active.map((h) => (
                    <option key={h.habit.id} value={h.habit.id}>
                      {h.habit.name}
                    </option>
                  ))}
                </SelectField>
              );
            })}
          </div>
        )}
      </QueryState>
    </Card>
  );
}
