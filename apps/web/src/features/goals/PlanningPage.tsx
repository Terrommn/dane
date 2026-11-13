// Planificar (equivalente a /objetivos del vault): revisar y cerrar lo anterior, definir el mes antes que la semana,
// 1–3 hábitos diarios por objetivo y avisos de carga (~6 mensuales, ~8 combinados; editables en Ajustes, no bloquean).
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { QueryState } from "../../components/ui/States";
import { useAuth } from "../auth/AuthProvider";
import { HabitFormDialog } from "../habits/HabitForm";
import { CloseGoalDialog } from "./CloseGoalDialog";
import { GoalCard } from "./GoalCard";
import { GoalFormDialog } from "./GoalForm";
import { type Goal, type GoalView, type Planning, usePlanning } from "./goalsApi";
import { formatMonth, formatWeek, isValidWeekKey, shiftWeek } from "./periods";

export function PlanningPage() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("week");
  const week = isValidWeekKey(requested) ? requested : null;
  const plan = usePlanning(week);
  return (
    <div className="space-y-4">
      <p>
        <Link to="/objetivos">← Objetivos</Link>
      </p>
      <QueryState query={plan}>
        {(p) => (
          <>
            <PageHeader
              title="Planificar"
              subtitle={formatWeek(p.week)}
              actions={
                <>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setParams({ week: shiftWeek(p.week, -1) })}
                    aria-label="Semana anterior"
                  >
                    ‹
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setParams({ week: shiftWeek(p.week, 1) })}
                    aria-label="Semana siguiente"
                  >
                    ›
                  </Button>
                </>
              }
            />
            <PlanningBody p={p} />
          </>
        )}
      </QueryState>
    </div>
  );
}

function PlanningBody({ p }: { p: Planning }) {
  const { me } = useAuth();
  const s = me?.settings;
  const [closing, setClosing] = useState<Goal | null>(null);
  const [creating, setCreating] = useState<{ kind: "month" | "week"; period: string } | null>(null);
  const [habitFor, setHabitFor] = useState<Goal | null>(null);
  const warnMonth = s?.habit_warn_month ?? 6;
  const warnTotal = s?.habit_warn_total ?? 8;
  const total = p.month_habit_count + p.week_habit_count;

  return (
    <div className="space-y-4">
      <Card title="Carga diaria de hábitos">
        <p className="text-sm">
          Del mes: <strong>{p.month_habit_count}</strong> (referencia ~{warnMonth}) · Combinados con la
          semana: <strong>{total}</strong> (referencia ~{warnTotal})
        </p>
        {p.warnings.length > 0 && (
          <ul className="mt-2 space-y-1">
            {p.warnings.map((w) => (
              <li key={w} role="status" className="rounded-lg border border-warn p-2 text-sm text-warn">
                ⚠ {w}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-2 text-xs text-muted">
          Son avisos, no límites. Ajusta las referencias en <Link to="/ajustes">Ajustes</Link>.
        </p>
      </Card>

      {p.pending_closure.length > 0 && (
        <Card title="Antes de planear: cierra lo anterior">
          <p className="mb-2 text-sm text-muted">
            Periodos terminados que siguen activos. El cierre es tu decisión.
          </p>
          <ul className="space-y-2">
            {p.pending_closure.map((g) => (
              <li key={g.goal.id} className="flex flex-wrap items-center gap-2">
                <Link to={`/objetivos/${g.goal.id}`} className="min-w-0 flex-1">
                  {g.goal.title} <span className="text-muted">({g.goal.period_key})</span>
                </Link>
                <Button size="sm" onClick={() => setClosing(g.goal)}>
                  Cerrar
                </Button>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <Card title={`Cómo te fue (${p.previous_week})`}>
        {p.previous_week_goals.length === 0 ? (
          <p className="text-sm text-muted">Sin objetivos la semana anterior.</p>
        ) : (
          <div className="grid gap-2 md:grid-cols-2">
            {p.previous_week_goals.map((g) => (
              <GoalCard key={g.goal.id} view={g} compact />
            ))}
          </div>
        )}
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <Digest title="Lo que salió mal" items={p.previous_week_reflections.bad} />
          <Digest title="Lo que salió bien" items={p.previous_week_reflections.good} />
        </div>
      </Card>

      {p.define_month_first && (
        <p role="status" className="rounded-xl border border-accent p-3 text-sm">
          Primero define los objetivos del mes (con sus hábitos); la semana debe empujar hacia ellos.
        </p>
      )}

      <GoalsBlock
        title={`Mes: ${formatMonth(p.month)}`}
        goals={p.month_goals}
        onNew={() => setCreating({ kind: "month", period: p.month })}
        onHabit={setHabitFor}
      />
      {p.next_month && (
        <GoalsBlock
          title={`Mes siguiente: ${formatMonth(p.next_month)} (la semana cruza de mes)`}
          goals={p.next_month_goals}
          onNew={() => setCreating({ kind: "month", period: p.next_month as string })}
          onHabit={setHabitFor}
        />
      )}
      <GoalsBlock
        title={`Semana ${p.week}`}
        goals={p.week_goals}
        onNew={() => setCreating({ kind: "week", period: p.week })}
        onHabit={setHabitFor}
        hint="No repitas los hábitos del mes ni los fijos; ataca lo que salió mal la semana pasada."
      />

      {closing && <CloseGoalDialog goal={closing} open onClose={() => setClosing(null)} />}
      {creating && (
        <GoalFormDialog
          open
          onClose={() => setCreating(null)}
          kind={creating.kind}
          periodKey={creating.period}
        />
      )}
      {habitFor && (
        <HabitFormDialog
          open
          onClose={() => setHabitFor(null)}
          goalId={habitFor.id}
          defaultStartsOn={habitFor.starts_on}
        />
      )}
    </div>
  );
}

function Digest({ title, items }: { title: string; items: string[] }) {
  return (
    <div>
      <p className="text-sm font-medium">{title}</p>
      {items.length === 0 ? (
        <p className="text-sm text-muted">Sin reflexiones registradas.</p>
      ) : (
        <ul className="list-disc space-y-0.5 pl-5 text-sm">
          {items.slice(0, 12).map((t, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: textos de reflexión pueden repetirse
            <li key={i}>{t}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

function GoalsBlock({
  title,
  goals,
  onNew,
  onHabit,
  hint,
}: {
  title: string;
  goals: GoalView[];
  onNew: () => void;
  onHabit: (g: Goal) => void;
  hint?: string;
}) {
  return (
    <Card
      title={<span className="capitalize">{title}</span>}
      actions={
        <Button size="sm" onClick={onNew}>
          Nuevo objetivo
        </Button>
      }
    >
      {hint && <p className="mb-2 text-sm text-muted">{hint}</p>}
      {goals.length === 0 ? (
        <p className="text-sm text-muted">Sin objetivos todavía.</p>
      ) : (
        <ul className="space-y-3">
          {goals.map((g) => (
            <li key={g.goal.id}>
              <GoalCard view={g} compact />
              <div className="mt-1 flex flex-wrap items-center gap-2 text-sm">
                <Badge tone={g.habits.length === 0 || g.habits.length > 3 ? "warn" : "muted"}>
                  {g.habits.length} hábitos
                </Badge>
                {g.habits.length === 0 && <span className="text-warn">Propón de 1 a 3 hábitos diarios</span>}
                {g.habits.length > 3 && <span className="text-warn">Más de 3 hábitos para un objetivo</span>}
                <Button size="sm" variant="secondary" onClick={() => onHabit(g.goal)}>
                  Agregar hábito
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
