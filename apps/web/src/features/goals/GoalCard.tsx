// Tarjeta de objetivo: resultado (progreso cuantitativo, ritmo, proyección etiquetada) separado de la adherencia a
// sus hábitos. No hay proyección lineal para valor actual (peso) ni hitos.
import { Link } from "react-router";
import { Badge, ProgressBar } from "../../components/ui/Display";
import { asCompliance } from "../habits/habitsApi";
import { formatNumber, formatPct } from "../today/numbers";
import { asProgress, describeProgress, GOAL_STATUS, type GoalProgress, type GoalView } from "./goalsApi";

export function paceText(p: GoalProgress, unit?: string | null): string | null {
  if (p.pace_delta === null || p.expected_by_today === null) return null;
  const u = unit ? ` ${unit}` : "";
  const d = Math.abs(p.pace_delta);
  if (d < 0.005) return `Al ritmo esperado (${formatNumber(p.expected_by_today, 2)}${u} a hoy)`;
  return p.pace_delta > 0
    ? `Adelantado ${formatNumber(d, 2)}${u} sobre el ritmo (${formatNumber(p.expected_by_today, 2)}${u} esperado)`
    : `Atrasado ${formatNumber(d, 2)}${u} del ritmo (${formatNumber(p.expected_by_today, 2)}${u} esperado)`;
}

export function statusTone(status: string): "ok" | "danger" | "accent" {
  return status === "achieved" ? "ok" : status === "not_achieved" ? "danger" : "accent";
}

export function GoalCard({ view, compact }: { view: GoalView; compact?: boolean }) {
  const g = view.goal;
  const p = asProgress(view.progress);
  const adherence = asCompliance(view.habit_adherence);
  const measured = g.progress_type !== "none";
  const ratio = p.ratio ?? (p.achieved ? 1 : 0);
  const pace = g.progress_type === "cumulative" ? paceText(p, g.unit) : null;
  return (
    <article className="min-w-0 rounded-xl border border-line bg-surface p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <h3 className="min-w-0 font-semibold">
          <Link to={`/objetivos/${g.id}`}>{g.title}</Link>
        </h3>
        <div className="flex flex-wrap gap-1">
          <Badge tone={statusTone(g.status)}>{GOAL_STATUS[g.status] ?? g.status}</Badge>
          {view.closure_pending && <Badge tone="warn">Cierre pendiente</Badge>}
          {g.archived_at && <Badge>archivado</Badge>}
        </div>
      </div>
      {measured && (
        <div className="mt-2">
          <ProgressBar
            label="Resultado"
            value={Math.min(ratio, 1)}
            max={1}
            valueText={describeProgress(g, p)}
            tone={p.achieved ? "ok" : "accent"}
          />
          {!compact && (
            <ul className="mt-1 space-y-0.5 text-xs text-muted">
              {pace && <li>{pace}</li>}
              {p.remaining !== null && p.remaining > 0 && !p.achieved && (
                <li>
                  Faltan {formatNumber(p.remaining, 2)}
                  {g.unit ? ` ${g.unit}` : ""}
                </li>
              )}
              {p.overachieved && <li>Sobrecumplido</li>}
              {p.target_zero && <li>Meta en 0: no hay porcentaje de avance</li>}
              {g.progress_type === "cumulative" && p.projection_estimate !== null && (
                <li>
                  Estimación al cierre (lineal, no es promesa): {formatNumber(p.projection_estimate, 2)}
                  {g.unit ? ` ${g.unit}` : ""}
                </li>
              )}
              {g.progress_type === "current_value" && p.last_measured_on && (
                <li>Última medición: {p.last_measured_on}</li>
              )}
              {!p.has_data && <li>Sin datos todavía</li>}
            </ul>
          )}
        </div>
      )}
      {view.habits.length > 0 && (
        <p className="mt-2 text-sm">
          <span className="text-muted">Hábitos ({view.habits.length}): </span>
          {adherence.required
            ? `${formatPct(adherence.rate)} cumplidos (${adherence.done}/${adherence.required})${adherence.unrecorded ? ` · ${adherence.unrecorded} sin registro` : ""}`
            : "sin días cerrados aún"}
        </p>
      )}
      {!compact && view.habits.length > 0 && (
        <ul className="mt-1 flex flex-wrap gap-1">
          {view.habits.map((h) => (
            <li key={h.id}>
              <Link
                to={`/habitos/${h.id}`}
                className="rounded-full border border-line px-2 py-0.5 text-xs no-underline"
              >
                {h.name}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}
