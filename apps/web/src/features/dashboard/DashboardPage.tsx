// Dashboard general (equivalente a Home.md / home.js con las correcciones C1, C2, C12 y C13): objetivos vs avance,
// hábitos separados, fallas devocionales, 30 días, peso sin interpolar, energía, paz, finanzas y comparaciones por
// el mismo tramo (u opcionalmente periodos completos, etiquetado). Cada métrica enlaza a su fuente.
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { DatePicker } from "../../components/ui/DatePicker";
import { Badge, Card, KpiCard, PageHeader } from "../../components/ui/Display";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import { formatMoney, formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { GoalCard } from "../goals/GoalCard";
import { complianceText, streakText } from "../habits/HabitsPage";
import { asCompliance, asStreaks, asToday } from "../habits/habitsApi";
import { formatNumber, formatPct } from "../today/numbers";

type Dashboard = Schemas["Dashboard"];

export function useDashboard(date: string, compare: "same_span" | "full") {
  return useQuery({
    queryKey: ["dashboard", date, compare],
    queryFn: async () => unwrap(await api.GET("/dashboard", { params: { query: { date, compare } } })),
  });
}

/** Formato de un valor de comparación según su unidad. */
export function formatMetric(v: number | null | undefined, unit: string, currency = "MXN"): string {
  if (v === null || v === undefined) return "—";
  switch (unit) {
    case "pct":
      return `${formatNumber(v, 0)}%`;
    case "kg":
      return `${formatNumber(v, 1)} kg`;
    case "cents":
      return formatMoney(v, currency);
    case "score":
      return formatNumber(v, 1);
    default:
      return formatNumber(v, 0);
  }
}

export function DashboardPage() {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const [params, setParams] = useSearchParams();
  const date = params.get("date") ?? today;
  const compare = params.get("comparar") === "completo" ? "full" : "same_span";
  const q = useDashboard(date, compare);
  const set = (p: Record<string, string>) =>
    setParams({
      ...(date !== today ? { date } : {}),
      ...(compare === "full" ? { comparar: "completo" } : {}),
      ...p,
    });

  return (
    <div className="space-y-4">
      <PageHeader
        title="Dashboard"
        subtitle={
          date === today
            ? "Hoy cuenta aparte; lo cerrado no incluye el día abierto"
            : `Al ${formatShortDate(date)}`
        }
        actions={
          <DatePicker value={date} max={today} onChange={(d) => setParams(d === today ? {} : { date: d })} />
        }
      />
      {q.isPending && <LoadingState rows={6} label="Calculando métricas…" />}
      {q.isError && <ErrorState error={q.error} onRetry={() => q.refetch()} />}
      {q.data && (
        <Body
          d={q.data}
          currency={me?.settings.currency ?? "MXN"}
          compare={compare}
          onCompare={(c) =>
            set({ comparar: c === "full" ? "completo" : "", ...(date !== today ? { date } : {}) })
          }
        />
      )}
    </div>
  );
}

function Body({
  d,
  currency,
  compare,
  onCompare,
}: {
  d: Dashboard;
  currency: string;
  compare: string;
  onCompare: (c: "same_span" | "full") => void;
}) {
  const w = d.weight;
  const dev = d.devotional;
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <KpiCard
          label="Peso (último)"
          value={w.last ? `${formatNumber(w.last.kg, 1)} kg` : "—"}
          sub={
            w.last ? (
              <>
                {formatShortDate(w.last.date)}
                {w.goal_kg !== null && w.goal_kg !== undefined && ` · meta ${formatNumber(w.goal_kg, 1)} kg`}
                {w.remaining_kg !== null &&
                  w.remaining_kg !== undefined &&
                  ` · faltan ${formatNumber(Math.abs(w.remaining_kg), 1)}`}
              </>
            ) : (
              "Sin mediciones"
            )
          }
        />
        <KpiCard
          label="Cambio de peso"
          value={
            w.change_week !== null && w.change_week !== undefined
              ? `${w.change_week > 0 ? "+" : ""}${formatNumber(w.change_week, 1)} kg`
              : "—"
          }
          sub={
            w.change_month !== null && w.change_month !== undefined
              ? `Mes: ${w.change_month > 0 ? "+" : ""}${formatNumber(w.change_month, 1)} kg`
              : "Se necesitan 2 mediciones reales"
          }
        />
        {dev ? (
          <KpiCard
            label="Fallas devocionales del mes"
            value={
              dev.max_allowed !== null && dev.max_allowed !== undefined
                ? `${dev.failures_total}/${dev.max_allowed}`
                : dev.failures_total
            }
            tone={
              dev.over_limit
                ? "danger"
                : dev.max_allowed !== null && dev.max_allowed !== undefined
                  ? "ok"
                  : undefined
            }
            sub={
              <Link to={dev.route}>
                {dev.explicit_failures} "no" + {dev.unrecorded} sin registro · {dev.closed_days} días cerrados
              </Link>
            }
          />
        ) : (
          <KpiCard label="Devocional" value="—" sub="Sin hábito devocional" />
        )}
        <KpiCard
          label="Consumo del mes"
          value={formatMoney(d.finance.month.consumption_cents, currency)}
          sub={
            <Link to={d.finance.route}>
              Semana: {formatMoney(d.finance.week.consumption_cents, currency)}
            </Link>
          }
        />
      </div>
      {dev?.note && <p className="text-xs text-muted">{dev.note}</p>}

      <Card title="Objetivos del mes" actions={<Link to="/objetivos">Objetivos →</Link>}>
        {d.month_goals.length ? (
          <div className="grid gap-3 md:grid-cols-2">
            {d.month_goals.map((g) => (
              <GoalCard key={g.goal.id} view={g} />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted">
            Sin objetivos este mes. <Link to="/objetivos/planificar">Planificar</Link>
          </p>
        )}
      </Card>
      <Card title="Objetivos de la semana">
        {d.week_goals.length ? (
          <div className="grid gap-3 md:grid-cols-2">
            {d.week_goals.map((g) => (
              <GoalCard key={g.goal.id} view={g} />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted">Sin objetivos esta semana.</p>
        )}
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <HabitBlock title="Hábitos fijos" items={d.habits.fixed} />
        <HabitBlock title="Hábitos del mes" items={d.habits.month} />
        <HabitBlock title="Hábitos de la semana" items={d.habits.week} />
        {d.habits.other.length > 0 && <HabitBlock title="Otros hábitos" items={d.habits.other} />}
      </div>

      <Heatmap days={d.last_30_days} />

      <div className="grid gap-4 lg:grid-cols-2">
        <Card title="Peso" actions={<Link to="/habitos">Historial →</Link>}>
          {w.points.length < 1 ? (
            <p className="text-sm text-muted">Sin mediciones. Anota tu peso en Hoy cuando te peses.</p>
          ) : (
            <div
              className="h-56"
              role="img"
              aria-label={`Peso: ${w.points.length} mediciones reales; último ${formatNumber(w.last?.kg, 1)} kg`}
            >
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={w.points} margin={{ left: -16, right: 8, top: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--c-border)" />
                  <XAxis dataKey="date" tickFormatter={formatShortDate} fontSize={11} />
                  <YAxis domain={["auto", "auto"]} fontSize={11} />
                  <Tooltip
                    formatter={(v) => [`${v} kg`, "Peso"]}
                    labelFormatter={(l) => formatShortDate(String(l))}
                  />
                  {w.goal_kg !== null && w.goal_kg !== undefined && (
                    <ReferenceLine
                      y={w.goal_kg}
                      stroke="var(--c-ok)"
                      strokeDasharray="4 4"
                      label={{ value: "meta", fontSize: 11 }}
                    />
                  )}
                  {/* connectNulls false: solo mediciones reales, sin interpolar días sin dato. */}
                  <Line
                    type="linear"
                    dataKey="kg"
                    stroke="var(--c-accent)"
                    dot
                    connectNulls={false}
                    isAnimationActive={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
          <p className="mt-1 text-xs text-muted">
            Solo puntos medidos; no se interpolan días sin dato ni se proyecta linealmente.
            {w.direction &&
              ` Dirección: ${w.direction === "decrease" ? "bajar" : w.direction === "increase" ? "subir" : "mantener"}.`}
          </p>
        </Card>
        <Card title="Energía y paz">
          {d.energy.length === 0 ? (
            <p className="text-sm text-muted">Sin registros de energía.</p>
          ) : (
            <div className="h-44" role="img" aria-label={`Energía diaria: ${d.energy.length} registros`}>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={d.energy} margin={{ left: -24, right: 8, top: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--c-border)" />
                  <XAxis dataKey="date" tickFormatter={formatShortDate} fontSize={11} />
                  <YAxis domain={[1, 5]} ticks={[1, 2, 3, 4, 5]} fontSize={11} />
                  <Tooltip
                    formatter={(v) => [v, "Energía"]}
                    labelFormatter={(l) => formatShortDate(String(l))}
                  />
                  <Line type="linear" dataKey="energy" stroke="var(--c-warn)" dot isAnimationActive={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
          <p className="mt-3 text-sm font-medium">Paz semanal (1–5)</p>
          {d.peace.length === 0 ? (
            <p className="text-sm text-muted">Sin revisiones semanales con paz.</p>
          ) : (
            <ul className="mt-1 flex flex-wrap gap-2 text-sm">
              {d.peace.map((p) => (
                <li key={p.iso_week}>
                  <Link to={p.route} className="rounded-full border border-line px-2 py-0.5 no-underline">
                    {p.iso_week}: {p.peace}
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      <Card title="Finanzas" actions={<Link to={d.finance.route}>Finanzas →</Link>}>
        <FinanceTable month={d.finance.month} week={d.finance.week} currency={currency} />
      </Card>

      <Card
        title="Comparaciones"
        actions={
          <fieldset className="flex gap-1 rounded-lg bg-surface-2 p-1 text-sm">
            <legend className="sr-only">Tipo de comparación</legend>
            {(
              [
                ["same_span", "Mismo tramo"],
                ["full", "Periodos completos"],
              ] as const
            ).map(([k, label]) => (
              <label key={k} className="cursor-pointer">
                <input
                  type="radio"
                  name="compare"
                  className="peer sr-only"
                  checked={compare === k}
                  onChange={() => onCompare(k)}
                />
                <span className="block rounded-md px-2 py-1 peer-checked:bg-surface peer-checked:shadow">
                  {label}
                </span>
              </label>
            ))}
          </fieldset>
        }
      >
        <div className="grid gap-4 lg:grid-cols-2">
          <ComparisonTable title="Semana" c={d.week_comparison} currency={currency} />
          <ComparisonTable title="Mes" c={d.month_comparison} currency={currency} />
        </div>
      </Card>
    </div>
  );
}

function HabitBlock({ title, items }: { title: string; items: Schemas["HabitStat"][] }) {
  return (
    <Card title={title}>
      {items.length === 0 ? (
        <p className="text-sm text-muted">Sin hábitos.</p>
      ) : (
        <ul className="divide-y divide-line">
          {items.map((h) => {
            const t = asToday(h.today);
            const week = asCompliance(h.week);
            const month = asCompliance(h.month);
            const s = asStreaks(h.streaks);
            return (
              <li key={h.habit.id} className="py-2 text-sm">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <Link to={h.route} className="font-medium">
                    {h.habit.name}
                  </Link>
                  <Badge tone={t.status === "done" ? "ok" : t.required_today && !t.status ? "warn" : "muted"}>
                    Hoy:{" "}
                    {t.status === "done"
                      ? "✓"
                      : t.status === "not_done"
                        ? "no"
                        : t.status === "not_applicable"
                          ? "no aplica"
                          : t.required_today
                            ? "pendiente"
                            : "—"}
                    {t.week_target ? ` · ${t.week_done ?? 0}/${t.week_target} sem` : ""}
                  </Badge>
                </div>
                <p className="text-muted">
                  Semana {formatPct(week.rate)} · Mes {complianceText(month)} · {streakText(s)}
                </p>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

function Heatmap({ days }: { days: Schemas["HeatDay"][] }) {
  const tone = (r: number | null | undefined, required: number) => {
    if (!required) return "bg-surface-2";
    if (r === null || r === undefined) return "border border-accent";
    if (r >= 1) return "bg-ok";
    if (r >= 0.66) return "bg-ok/70";
    if (r >= 0.33) return "bg-warn/60";
    return "bg-danger/60";
  };
  return (
    <Card title="Últimos 30 días">
      <ol className="grid grid-cols-7 gap-1 sm:grid-cols-10 lg:grid-cols-15">
        {days.map((d) => (
          <li key={d.date}>
            <Link
              to={d.route}
              title={`${d.date}: ${d.required ? `${d.done}/${d.required}` : "nada exigible"}${d.unrecorded ? ` · ${d.unrecorded} sin registro` : ""}${d.energy ? ` · energía ${d.energy}` : ""}`}
              className={`flex aspect-square items-center justify-center rounded text-[10px] no-underline ${tone(d.ratio, d.required)}`}
            >
              <span aria-hidden>{Number(d.date.slice(8))}</span>
              <span className="sr-only">
                {d.date}: {d.required ? `${d.done} de ${d.required} cumplidos` : "nada exigible"}
                {d.unrecorded ? `, ${d.unrecorded} sin registro` : ""}
              </span>
            </Link>
          </li>
        ))}
      </ol>
      <p className="mt-2 text-xs text-muted">
        Verde: todo cumplido · ámbar/rojo: parcial · contorno: día abierto · gris: nada exigible. Los días sin
        registro cuentan como no cumplidos y se detallan al pasar el cursor.
      </p>
    </Card>
  );
}

function FinanceTable({
  month,
  week,
  currency,
}: {
  month: Schemas["FinanceSummary"];
  week: Schemas["FinanceSummary"];
  currency: string;
}) {
  const rows: [string, keyof Schemas["FinanceSummary"]][] = [
    ["Consumo", "consumption_cents"],
    ["Pagos de deuda", "debt_payments_cents"],
    ["Ingreso ganado", "earned_income_cents"],
    ["Financiamiento (préstamos)", "financing_cents"],
    ["Inversión", "investment_cents"],
    ["Entradas de efectivo", "cash_in_cents"],
    ["Salidas de efectivo", "cash_out_cents"],
  ];
  return (
    <>
      <table className="w-full text-sm">
        <thead className="text-left text-muted">
          <tr>
            <th className="py-1 font-medium">Concepto</th>
            <th className="text-right font-medium">Semana</th>
            <th className="text-right font-medium">Mes</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([label, k]) => (
            <tr key={k} className="border-t border-line">
              <th scope="row" className="py-1 text-left font-normal">
                {label}
              </th>
              <td className="text-right tabular-nums">{formatMoney(week[k], currency)}</td>
              <td className="text-right tabular-nums">{formatMoney(month[k], currency)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {(month.excluded_foreign_count > 0 || week.excluded_foreign_count > 0) && (
        <p className="mt-2 text-xs text-warn">
          {month.excluded_foreign_count} movimiento(s) en otra moneda sin conversión no se suman.
        </p>
      )}
      <p className="mt-1 text-xs text-muted">
        Solo movimientos confirmados. Consumo cuenta una vez aunque se pague con tarjeta.
      </p>
    </>
  );
}

function ComparisonTable({
  title,
  c,
  currency,
}: {
  title: string;
  c: Schemas["Comparison"];
  currency: string;
}) {
  return (
    <section>
      <h3 className="font-semibold">{title}</h3>
      <p className="mb-2 text-xs text-muted">
        {c.label} ({c.mode === "full" ? "periodos completos" : "mismo tramo"})
      </p>
      <table className="w-full text-sm">
        <thead className="text-left text-muted">
          <tr>
            <th className="py-1 font-medium">Métrica</th>
            <th className="text-right font-medium">Actual</th>
            <th className="text-right font-medium">Anterior</th>
            <th className="text-right font-medium">Cambio</th>
          </tr>
        </thead>
        <tbody>
          {c.metrics.map((m) => (
            <tr key={m.key} className="border-t border-line">
              <th scope="row" className="py-1 text-left font-normal">
                <Link to={m.route}>{m.label}</Link>
              </th>
              <td className="text-right tabular-nums">{formatMetric(m.current, m.unit, currency)}</td>
              <td className="text-right tabular-nums">{formatMetric(m.previous, m.unit, currency)}</td>
              <td
                className={`text-right tabular-nums ${m.better === true ? "text-ok" : m.better === false ? "text-danger" : ""}`}
              >
                {m.delta === null || m.delta === undefined
                  ? "—"
                  : `${m.delta > 0 ? "+" : ""}${formatMetric(m.delta, m.unit, currency)}`}
                <span className="sr-only">
                  {m.better === true ? " (favorable)" : m.better === false ? " (desfavorable)" : ""}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
