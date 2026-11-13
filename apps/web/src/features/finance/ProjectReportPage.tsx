// Gastos de un proyecto derivados del registro general (sin duplicar movimientos): rubros vs presupuesto, avisos de
// desajuste entre el desglose y la categoría, suscripciones del proyecto, histórico mensual y detalle.
import { Link, useParams, useSearchParams } from "react-router";
import { Badge, Card, PageHeader, ProgressBar } from "../../components/ui/Display";
import { EmptyState, QueryState } from "../../components/ui/States";
import { formatShortDate } from "../../lib/format";
import { currentMonth, FinanceNav, Money, MonthPicker, Stat } from "./components";
import { useProjectReport } from "./financeApi";
import { monthLabel } from "./labels";
import { formatCents } from "./money";

export function ProjectReportPage() {
  const { id = "" } = useParams();
  const [sp, setSp] = useSearchParams();
  const month = sp.get("month") ?? currentMonth();
  const q = useProjectReport(id, sp.get("month") ?? undefined);
  return (
    <div>
      <PageHeader
        title={q.data ? `Gastos de ${q.data.project_name}` : "Gastos del proyecto"}
        subtitle={monthLabel(month)}
        actions={
          <MonthPicker
            value={month}
            onChange={(m) => setSp(m === currentMonth() ? {} : { month: m }, { replace: true })}
          />
        }
      />
      <FinanceNav />
      <QueryState query={q} loadingRows={4}>
        {(r) => (
          <div className="space-y-4">
            {r.warnings.map((w) => (
              <p key={w} role="note" className="rounded-xl border border-warn bg-surface p-3 text-sm">
                {w}
              </p>
            ))}
            <Card
              title="Rubros del mes"
              actions={<Link to={`/finanzas/presupuesto?month=${r.month}`}>Editar rubros</Link>}
            >
              <dl className="mb-3">
                <Stat label="Gastado">
                  <Money cents={r.spent_cents} />
                </Stat>
                <Stat label="Presupuesto">
                  <Money cents={r.budget_cents} />
                </Stat>
                <Stat label="Suscripciones del proyecto (mes)">
                  <Money cents={r.subscriptions_month_cents} />
                </Stat>
              </dl>
              {r.rubros.length === 0 ? (
                <EmptyState title="Sin rubros ni gastos este mes" />
              ) : (
                <ul className="space-y-3">
                  {r.rubros.map((x) =>
                    x.budget_cents ? (
                      <li key={x.budget_item_id ?? x.name}>
                        <ProgressBar
                          label={x.name}
                          value={x.spent_cents}
                          max={x.budget_cents}
                          tone={(x.remaining_cents ?? 0) < 0 ? "danger" : "ok"}
                          valueText={`${formatCents(x.spent_cents)} de ${formatCents(x.budget_cents)}`}
                        />
                      </li>
                    ) : (
                      <li key={x.budget_item_id ?? x.name} className="flex justify-between text-sm">
                        <span>
                          {x.name}{" "}
                          {!x.budget_item_id && <Badge tone="warn">revisa el concepto o asigna rubro</Badge>}
                        </span>
                        <Money cents={x.spent_cents} />
                      </li>
                    ),
                  )}
                </ul>
              )}
            </Card>
            {r.subscriptions.length > 0 && (
              <Card title="Suscripciones del proyecto">
                <ul className="divide-y divide-line">
                  {r.subscriptions.map((s) => (
                    <li key={s.rule_id} className="flex justify-between py-2">
                      <span>{s.name}</span>
                      <Money cents={s.amount_cents} />
                    </li>
                  ))}
                </ul>
              </Card>
            )}
            <Card title="Por mes">
              <ul className="divide-y divide-line">
                {r.by_month.map((m) => (
                  <li key={m.month} className="flex justify-between gap-3 py-2">
                    <Link to={`?month=${m.month}`}>{monthLabel(m.month)}</Link>
                    <span>
                      <Money cents={m.spent_cents} />
                      {m.budget_cents != null && (
                        <span className="text-muted"> de {formatCents(m.budget_cents)}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            </Card>
            <Card
              title="Detalle"
              actions={
                <Link to={`/finanzas/movimientos?project_id=${r.project_id}`}>Ver en movimientos</Link>
              }
            >
              {r.expenses.length === 0 ? (
                <EmptyState title="Sin gastos este mes" />
              ) : (
                <ul className="divide-y divide-line">
                  {r.expenses.map((e) => (
                    <li key={e.id} className="flex justify-between gap-3 py-2 text-sm">
                      <span className="min-w-0">
                        <span className="block break-words">{e.concept}</span>
                        <span className="text-muted">
                          {formatShortDate(e.occurred_on)} · {e.category_name ?? "Sin categoría"}
                        </span>
                      </span>
                      <Money cents={e.amount_cents} />
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </div>
        )}
      </QueryState>
    </div>
  );
}
