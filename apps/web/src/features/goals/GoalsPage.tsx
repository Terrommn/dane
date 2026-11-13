// Objetivos: del mes, de la semana e historial (vista Objetivos.base: agrupado por tipo, periodo, inicio, fin, estado).
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { EmptyState, QueryState } from "../../components/ui/States";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { GoalCard } from "./GoalCard";
import { GoalFormDialog } from "./GoalForm";
import { GOAL_STATUS, type GoalView, useGoals } from "./goalsApi";
import {
  formatMonth,
  formatWeek,
  isoWeekKey,
  isValidMonthKey,
  isValidWeekKey,
  monthKey,
  shiftMonth,
  shiftWeek,
} from "./periods";

type Tab = "mes" | "semana" | "historial";

export function GoalsPage() {
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const [params, setParams] = useSearchParams();
  const tab = (params.get("vista") as Tab | null) ?? "mes";
  const month = isValidMonthKey(params.get("mes")) ? (params.get("mes") as string) : monthKey(today);
  const week = isValidWeekKey(params.get("semana")) ? (params.get("semana") as string) : isoWeekKey(today);
  const set = (p: Record<string, string>) => setParams({ vista: tab, mes: month, semana: week, ...p });

  return (
    <div className="space-y-4">
      <PageHeader
        title="Objetivos"
        subtitle="Resultado y adherencia a hábitos se miden por separado"
        actions={
          <ButtonLink to={`/objetivos/planificar?week=${week}`} variant="primary">
            Planificar
          </ButtonLink>
        }
      />
      <div role="tablist" aria-label="Vista" className="flex gap-1 rounded-xl bg-surface-2 p-1">
        {(
          [
            ["mes", "Del mes"],
            ["semana", "De la semana"],
            ["historial", "Historial"],
          ] as const
        ).map(([k, label]) => (
          <button
            key={k}
            type="button"
            role="tab"
            aria-selected={tab === k}
            onClick={() => set({ vista: k })}
            className={`min-h-10 flex-1 rounded-lg px-3 text-sm font-medium ${tab === k ? "bg-surface shadow" : "text-muted"}`}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "mes" && (
        <PeriodGoals
          kind="month"
          periodKey={month}
          label={formatMonth(month)}
          onPrev={() => set({ mes: shiftMonth(month, -1) })}
          onNext={() => set({ mes: shiftMonth(month, 1) })}
        />
      )}
      {tab === "semana" && (
        <PeriodGoals
          kind="week"
          periodKey={week}
          label={formatWeek(week)}
          onPrev={() => set({ semana: shiftWeek(week, -1) })}
          onNext={() => set({ semana: shiftWeek(week, 1) })}
        />
      )}
      {tab === "historial" && <History />}
    </div>
  );
}

function PeriodGoals({
  kind,
  periodKey,
  label,
  onPrev,
  onNext,
}: {
  kind: "month" | "week";
  periodKey: string;
  label: string;
  onPrev: () => void;
  onNext: () => void;
}) {
  const goals = useGoals({ kind, period_key: periodKey });
  const [creating, setCreating] = useState(false);
  return (
    <Card
      title={<span className="capitalize">{label}</span>}
      actions={
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" onClick={onPrev} aria-label="Periodo anterior">
            ‹
          </Button>
          <Button size="sm" variant="secondary" onClick={onNext} aria-label="Periodo siguiente">
            ›
          </Button>
          <Button size="sm" onClick={() => setCreating(true)}>
            Nuevo
          </Button>
        </div>
      }
    >
      <QueryState
        query={goals}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Sin objetivos en este periodo"
            action={<Button onClick={() => setCreating(true)}>Crear objetivo</Button>}
          >
            {kind === "week"
              ? "Define primero los del mes y luego los de la semana."
              : "Uno a cuatro objetivos concretos (p. ej. uno por área)."}
          </EmptyState>
        }
      >
        {(list) => (
          <div className="grid gap-3 md:grid-cols-2">
            {list.map((g) => (
              <GoalCard key={g.goal.id} view={g} />
            ))}
          </div>
        )}
      </QueryState>
      {creating && (
        <GoalFormDialog open onClose={() => setCreating(false)} kind={kind} periodKey={periodKey} />
      )}
    </Card>
  );
}

function History() {
  const [archived, setArchived] = useState(false);
  const goals = useGoals({ archived });
  return (
    <Card
      title="Historial"
      actions={
        <Button size="sm" variant="secondary" onClick={() => setArchived((v) => !v)}>
          {archived ? "Ocultar archivados" : "Incluir archivados"}
        </Button>
      }
    >
      <QueryState
        query={goals}
        isEmpty={(d) => d.length === 0}
        empty={<p className="text-sm text-muted">Sin objetivos todavía.</p>}
      >
        {(list) => (
          <div className="space-y-4">
            {(["month", "week"] as const).map((k) => {
              const items = list
                .filter((g) => g.goal.kind === k)
                .sort((a, b) => b.goal.starts_on.localeCompare(a.goal.starts_on));
              if (!items.length) return null;
              return (
                <section key={k}>
                  <h3 className="mb-2 font-semibold">{k === "month" ? "Mensuales" : "Semanales"}</h3>
                  <HistoryTable items={items} />
                </section>
              );
            })}
          </div>
        )}
      </QueryState>
    </Card>
  );
}

function HistoryTable({ items }: { items: GoalView[] }) {
  return (
    <>
      <ul className="space-y-2 md:hidden">
        {items.map((g) => (
          <li key={g.goal.id} className="rounded-lg border border-line p-2 text-sm">
            <Link to={`/objetivos/${g.goal.id}`} className="font-medium">
              {g.goal.title}
            </Link>
            <p className="text-muted">
              {g.goal.period_key} · {g.goal.starts_on} → {g.goal.ends_on} · {GOAL_STATUS[g.goal.status]}
              {g.closure_pending && " · cierre pendiente"}
            </p>
          </li>
        ))}
      </ul>
      <table className="hidden w-full text-sm md:table">
        <thead className="text-left text-muted">
          <tr>
            <th className="py-1 font-medium">Objetivo</th>
            <th className="font-medium">Periodo</th>
            <th className="font-medium">Inicio</th>
            <th className="font-medium">Fin</th>
            <th className="font-medium">Estado</th>
          </tr>
        </thead>
        <tbody>
          {items.map((g) => (
            <tr key={g.goal.id} className="border-t border-line">
              <td className="py-1.5">
                <Link to={`/objetivos/${g.goal.id}`}>{g.goal.title}</Link>
              </td>
              <td>{g.goal.period_key}</td>
              <td>{g.goal.starts_on}</td>
              <td>{g.goal.ends_on}</td>
              <td>
                {GOAL_STATUS[g.goal.status]}{" "}
                {g.closure_pending && <Badge tone="warn">cierre pendiente</Badge>}
                {g.goal.archived_at && <Badge>archivado</Badge>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}
