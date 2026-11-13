// Dashboard financiero del mes: consumo, pagos de deuda, ingreso ganado, financiamiento y flujo separados (corrige el
// "neto" del vault que mezclaba compras con tarjeta y su pago posterior), presupuesto, ritmo, proyección etiquetada,
// ingresos registrados/confirmados/esperados, 6 meses, histórico, top 5 y por proyecto. Cada cifra enlaza a sus
// movimientos fuente.
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { ButtonLink } from "../../components/ui/Button";
import { Badge, Card, KpiCard, PageHeader, ProgressBar } from "../../components/ui/Display";
import { EmptyState, QueryState } from "../../components/ui/States";
import { formatShortDate } from "../../lib/format";
import { ConfirmOccurrenceDialog, type OccurrenceRef } from "./ConfirmOccurrenceDialog";
import { currentMonth, FinanceNav, Money, MonthPicker, Stat } from "./components";
import { type FinanceDashboard, useDashboard } from "./financeApi";
import { monthLabel, OCCURRENCE_STATE_LABEL, RULE_KIND_LABEL } from "./labels";
import { formatCents, formatCentsShort, pct } from "./money";

/** Rango del mes para enlazar a movimientos fuente. */
export function monthRange(month: string, daysInMonth: number) {
  return { from: `${month}-01`, to: `${month}-${String(daysInMonth).padStart(2, "0")}` };
}

export function movementsLink(month: string, days: number, extra: Record<string, string> = {}) {
  const { from, to } = monthRange(month, days);
  return `/finanzas/movimientos?${new URLSearchParams({ from, to, ...extra }).toString()}`;
}

export function FinanceDashboardPage() {
  const [sp, setSp] = useSearchParams();
  const month = sp.get("month") ?? currentMonth();
  const q = useDashboard(sp.get("month") ?? undefined);
  return (
    <div>
      <PageHeader
        title="Finanzas"
        subtitle={`Resumen de ${monthLabel(month)}`}
        actions={
          <MonthPicker
            value={q.data?.month ?? month}
            onChange={(m) => setSp(m === currentMonth() ? {} : { month: m }, { replace: true })}
          />
        }
      />
      <FinanceNav />
      <QueryState query={q} loadingRows={6}>
        {(d) => <Dashboard d={d} />}
      </QueryState>
    </div>
  );
}

function Dashboard({ d }: { d: FinanceDashboard }) {
  const t = d.totals;
  const link = (extra: Record<string, string> = {}) => movementsLink(d.month, d.days_in_month, extra);
  const [confirming, setConfirming] = useState<OccurrenceRef | null>(null);
  const capUsed = d.spending_cap_cents > 0 ? pct(d.spent_cents, d.spending_cap_cents) : null;
  const daily = d.daily.map((p) => ({
    day: p.day,
    Gastado: p.spent_cents ?? null,
    Ritmo: p.pace_cents ?? null,
  }));
  const months = d.last_six_months.map((m) => ({
    mes: monthLabel(m.month),
    Consumo: m.consumption_cents,
    "Ingreso ganado": m.earned_income_cents,
    Presupuesto: m.spending_cap_cents ?? null,
  }));

  return (
    <div className="space-y-4">
      {t.excluded_foreign_count > 0 && (
        <p role="note" className="rounded-xl border border-warn bg-surface p-3 text-sm">
          {t.excluded_foreign_count} movimiento(s) en otra moneda sin conversión no se suman a estos totales.{" "}
          <Link to={link()}>Revisar</Link>
        </p>
      )}

      <section aria-label="Flujo del mes" className="grid grid-cols-2 gap-3 lg:grid-cols-3">
        <KpiCard
          label="Consumo"
          value={formatCents(t.consumption_cents)}
          sub={<Link to={link({ kind: "expense" })}>Una vez por compra, aun con tarjeta</Link>}
        />
        <KpiCard
          label="Pagos de deuda"
          value={formatCents(t.debt_payments_cents)}
          sub="Tarjetas y abonos (no es consumo)"
        />
        <KpiCard
          label="Ingreso ganado"
          value={formatCents(t.earned_income_cents)}
          sub={<Link to={link({ kind: "income" })}>Ver ingresos</Link>}
        />
        <KpiCard
          label="Financiamiento"
          value={formatCents(t.financing_cents)}
          sub={<Link to={link({ kind: "loan_received" })}>Préstamos recibidos</Link>}
        />
        <KpiCard
          label="Neto de consumo"
          value={formatCents(t.consumption_net_cents)}
          tone={t.consumption_net_cents < 0 ? "danger" : "ok"}
          sub="Ingreso ganado − consumo"
        />
        <KpiCard
          label="Flujo de efectivo"
          value={formatCents(t.cash_net_cents)}
          tone={t.cash_net_cents < 0 ? "danger" : undefined}
          sub={`Entró ${formatCents(t.cash_in_cents)} · salió ${formatCents(t.cash_out_cents)}`}
        />
      </section>
      {(t.other_income_cents > 0 || t.investment_cents > 0) && (
        <p className="text-sm text-muted">
          Otros ingresos: {formatCents(t.other_income_cents)} · Inversión: {formatCents(t.investment_cents)}{" "}
          (no se cuentan como consumo).
        </p>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card
          title="Presupuesto del mes"
          actions={
            <ButtonLink to={`/finanzas/presupuesto?month=${d.month}`} size="sm">
              Editar
            </ButtonLink>
          }
        >
          {d.spending_cap_cents > 0 ? (
            <>
              {d.budget_is_proposal && (
                <p className="mb-2">
                  <Badge tone="warn">Propuesta copiada del mes anterior</Badge>
                </p>
              )}
              <ProgressBar
                label="Gastado vs tope"
                value={d.spent_cents}
                max={d.spending_cap_cents}
                tone={d.budget_remaining_cents < 0 ? "danger" : capUsed && capUsed > 85 ? "warn" : "ok"}
                valueText={`${formatCents(d.spent_cents)} de ${formatCents(d.spending_cap_cents)}`}
              />
              <dl className="mt-3">
                <Stat label={d.budget_remaining_cents < 0 ? "Excedido" : "Me queda"}>
                  <Money
                    cents={Math.abs(d.budget_remaining_cents)}
                    className={d.budget_remaining_cents < 0 ? "text-danger" : "text-ok"}
                  />
                </Stat>
                <Stat
                  label="Proyección fin de mes"
                  hint="Estimación: variable al ritmo diario + suscripciones previstas"
                >
                  <Money
                    cents={d.projection_cents}
                    className={d.projection_cents > d.spending_cap_cents ? "text-danger" : ""}
                  />
                </Stat>
                <Stat label="Variable a la fecha" hint={`${d.days_elapsed} de ${d.days_in_month} días`}>
                  <Money cents={d.variable_spent_cents} />
                </Stat>
                <Stat label="Suscripciones del mes">
                  <Money cents={d.subscriptions_month_cents} />
                </Stat>
              </dl>
            </>
          ) : (
            <EmptyState title="Sin presupuesto este mes">
              Define cuánto puedes gastar por categoría para ver lo que te queda y el ritmo.
            </EmptyState>
          )}
        </Card>

        <Card title="Ingresos del mes">
          <dl>
            <Stat label="Confirmados" hint="Manuales y ocurrencias conciliadas">
              <Money cents={d.income.confirmed_cents} />
            </Stat>
            <Stat
              label="Registrados sin conciliar"
              hint="Ingresos fijos generados; aún no prueban el depósito"
            >
              <Money cents={d.income.pending_cents} />
            </Stat>
            <Stat label="Esperados aún sin registrar" hint="Proyección por ingresos fijos">
              <Money cents={d.income.projected_cents} />
            </Stat>
            <Stat label="Esperado del mes (fijos)">
              <Money cents={d.income.expected_fixed_cents} />
            </Stat>
            <Stat label="Esperado − presupuesto" hint="Lo que planeas que te quede">
              <Money cents={d.income.expected_minus_budget_cents} signed />
            </Stat>
          </dl>
        </Card>
      </div>

      <Card title="Gasto acumulado vs ritmo presupuestado">
        {d.daily.some((p) => p.spent_cents) ? (
          <div
            className="h-64"
            role="img"
            aria-label="Gráfica de gasto acumulado del mes frente al ritmo del presupuesto"
          >
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={daily} margin={{ left: 4, right: 8, top: 8, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--c-border)" />
                <XAxis dataKey="day" tick={{ fill: "var(--c-muted)", fontSize: 12 }} />
                <YAxis
                  tickFormatter={formatCentsShort}
                  width={56}
                  tick={{ fill: "var(--c-muted)", fontSize: 12 }}
                />
                <Tooltip formatter={(v) => formatCents(Number(v))} labelFormatter={(l) => `Día ${l}`} />
                <Legend />
                <Line
                  type="monotone"
                  dataKey="Gastado"
                  stroke="var(--c-accent)"
                  strokeWidth={2}
                  dot={false}
                  connectNulls={false}
                />
                <Line
                  type="linear"
                  dataKey="Ritmo"
                  stroke="var(--c-muted)"
                  strokeDasharray="5 5"
                  dot={false}
                />
              </LineChart>
            </ResponsiveContainer>
          </div>
        ) : (
          <EmptyState title="Aún no hay gastos este mes" />
        )}
        <p className="mt-2 text-sm text-muted">
          Línea sólida: lo gastado (sin pagos de deuda). Punteada: hasta dónde deberías ir para no pasarte del
          tope.
        </p>
      </Card>

      <Card title="Por categoría vs presupuesto">
        {d.by_category.length === 0 ? (
          <EmptyState title="Sin gastos ni presupuesto por categoría" />
        ) : (
          <ul className="space-y-3">
            {d.by_category.map((c) => (
              <li key={c.category_id ?? c.category_name}>
                {c.budget_cents ? (
                  <ProgressBar
                    label={c.category_name}
                    value={c.spent_cents}
                    max={c.budget_cents}
                    tone={(c.remaining_cents ?? 0) < 0 ? "danger" : "ok"}
                    valueText={`${formatCents(c.spent_cents)} de ${formatCents(c.budget_cents)} · ${
                      (c.remaining_cents ?? 0) < 0
                        ? `excedido ${formatCents(-(c.remaining_cents ?? 0))}`
                        : `quedan ${formatCents(c.remaining_cents ?? 0)}`
                    }`}
                  />
                ) : (
                  <p className="flex justify-between text-sm">
                    <span>
                      {c.category_name} <span className="text-muted">(sin presupuesto)</span>
                    </span>
                    <Money cents={c.spent_cents} />
                  </p>
                )}
                {c.category_id && (
                  <Link className="text-sm" to={link({ category_id: c.category_id })}>
                    Ver movimientos
                  </Link>
                )}
              </li>
            ))}
          </ul>
        )}
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card
          title="Suscripciones e ingresos fijos del mes"
          actions={
            <ButtonLink to="/finanzas/recurrentes" size="sm">
              Administrar
            </ButtonLink>
          }
        >
          {d.occurrences.length === 0 ? (
            <EmptyState title="Sin recurrentes este mes" />
          ) : (
            <ul className="divide-y divide-line">
              {d.occurrences.map((o) => (
                <li
                  key={`${o.rule_id}-${o.date}`}
                  className="flex flex-wrap items-center justify-between gap-2 py-2"
                >
                  <span className="min-w-0">
                    <span className="block font-medium">{o.rule_name}</span>
                    <span className="block text-sm text-muted">
                      {RULE_KIND_LABEL[o.kind] ?? o.kind} · {formatShortDate(o.date)} ·{" "}
                      {OCCURRENCE_STATE_LABEL[o.state] ?? o.state}
                    </span>
                  </span>
                  <span className="flex items-center gap-2">
                    <Money cents={o.amount_cents} />
                    {o.state === "pending" && o.transaction_id && (
                      <button
                        type="button"
                        className="text-sm font-medium text-link underline"
                        onClick={() =>
                          setConfirming({
                            transaction_id: o.transaction_id ?? "",
                            transaction_version: o.transaction_version ?? 1,
                            amount_cents: o.amount_cents,
                            date: o.date,
                            rule_name: o.rule_name,
                          })
                        }
                      >
                        Confirmar
                      </button>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card
          title="Cuentas y tarjetas"
          actions={
            <ButtonLink to="/finanzas/cuentas" size="sm">
              Ver
            </ButtonLink>
          }
        >
          <dl>
            <Stat
              label="Débito/efectivo estimado"
              hint="Saldo observado + movimientos posteriores; no es dato del banco"
            >
              <Money cents={d.accounts.cash_estimated_cents} />
            </Stat>
            <Stat label="Crédito disponible estimado">
              <Money cents={d.accounts.cards_available_cents} /> de{" "}
              <Money cents={d.accounts.cards_limit_cents} />
            </Stat>
            {d.accounts.without_snapshot > 0 && (
              <Stat label="Sin saldo observado" hint="No se incluyen en los totales">
                {d.accounts.without_snapshot}
              </Stat>
            )}
            {d.accounts.oldest_snapshot_days != null && (
              <Stat label="Dato más antiguo">hace {d.accounts.oldest_snapshot_days} días</Stat>
            )}
          </dl>
        </Card>
      </div>

      <Card title="Últimos 6 meses">
        <div
          className="h-64"
          role="img"
          aria-label="Gráfica de consumo e ingreso ganado de los últimos seis meses"
        >
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={months} margin={{ left: 4, right: 8, top: 8, bottom: 0 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--c-border)" />
              <XAxis dataKey="mes" tick={{ fill: "var(--c-muted)", fontSize: 12 }} />
              <YAxis
                tickFormatter={formatCentsShort}
                width={56}
                tick={{ fill: "var(--c-muted)", fontSize: 12 }}
              />
              <Tooltip formatter={(v) => (v == null ? "—" : formatCents(Number(v)))} />
              <Legend />
              <Bar dataKey="Ingreso ganado" fill="var(--c-link)" />
              <Bar dataKey="Consumo" fill="var(--c-warn)" />
              <Bar dataKey="Presupuesto" fill="var(--c-border)" />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card title="Gastos más grandes del mes">
          {d.top_expenses.length === 0 ? (
            <EmptyState title="Sin gastos este mes" />
          ) : (
            <ol className="divide-y divide-line">
              {d.top_expenses.map((r) => (
                <li key={r.id} className="flex justify-between gap-3 py-2">
                  <span className="min-w-0">
                    <Link to={link({ q: r.concept })} className="block break-words">
                      {r.concept}
                    </Link>
                    <span className="text-sm text-muted">
                      {formatShortDate(r.occurred_on)} · {r.category_name ?? "Sin categoría"}
                      {r.from_subscription ? " · suscripción" : ""}
                    </span>
                  </span>
                  <Money cents={r.amount_cents} />
                </li>
              ))}
            </ol>
          )}
        </Card>
        <Card title="Gastos por proyecto">
          {d.by_project.length === 0 ? (
            <EmptyState title="Sin gastos asignados a proyectos" />
          ) : (
            <ul className="divide-y divide-line">
              {d.by_project.map((p) => (
                <li key={p.project_id} className="flex justify-between gap-3 py-2">
                  <Link to={`/finanzas/proyectos/${p.project_id}?month=${d.month}`}>{p.project_name}</Link>
                  <Money cents={p.spent_cents} />
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      <Card title="Histórico mensual">
        {d.history.length === 0 ? (
          <EmptyState title="Sin historial todavía" />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[32rem] border-collapse text-left text-sm">
              <caption className="sr-only">Histórico mensual de presupuesto, gasto e ingresos</caption>
              <thead className="bg-surface-2">
                <tr>
                  <th scope="col" className="px-3 py-2">
                    Mes
                  </th>
                  <th scope="col" className="px-3 py-2 text-right">
                    Presupuesto
                  </th>
                  <th scope="col" className="px-3 py-2 text-right">
                    Gastado
                  </th>
                  <th scope="col" className="px-3 py-2 text-right">
                    Ingreso ganado
                  </th>
                  <th scope="col" className="px-3 py-2 text-right">
                    Neto de consumo
                  </th>
                </tr>
              </thead>
              <tbody>
                {d.history.map((m) => (
                  <tr key={m.month} className="border-t border-line">
                    <th scope="row" className="px-3 py-2 font-normal">
                      <Link to={`/finanzas?month=${m.month}`}>{monthLabel(m.month)}</Link>
                    </th>
                    <td className="px-3 py-2 text-right">
                      <Money cents={m.spending_cap_cents ?? null} />
                    </td>
                    <td
                      className={`px-3 py-2 text-right ${m.spending_cap_cents && m.spent_cents > m.spending_cap_cents ? "text-danger" : ""}`}
                    >
                      <Money cents={m.spent_cents} />
                    </td>
                    <td className="px-3 py-2 text-right">
                      <Money cents={m.earned_income_cents} />
                    </td>
                    <td className="px-3 py-2 text-right">
                      <Money cents={m.consumption_net_cents} signed />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
      {confirming && <ConfirmOccurrenceDialog occurrence={confirming} onClose={() => setConfirming(null)} />}
    </div>
  );
}
