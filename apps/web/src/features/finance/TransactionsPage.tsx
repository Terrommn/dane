// Movimientos: histórico con filtros (en la URL, para enlazar desde el dashboard), orden, paginación y totales del
// servidor; captura individual y múltiple; detalle con editar, anular y confirmar ocurrencias pendientes.
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import { BatchCaptureDialog } from "./BatchCaptureDialog";
import { ConfirmOccurrenceDialog } from "./ConfirmOccurrenceDialog";
import { FinanceNav, Money, SimpleSelect, Stat } from "./components";
import {
  type Transaction,
  type TxFilters,
  useAccounts,
  useCategories,
  useDebts,
  usePeopleOptions,
  useProjectOptions,
  useTransactions,
  useTxMutations,
} from "./financeApi";
import { ORIGIN_LABEL, TX_KIND_LABEL, TX_KINDS, TX_STATUS_LABEL, type TxKind } from "./labels";
import { TxFormDialog } from "./TxFormDialog";
import { flowSign } from "./txForm";

const PAGE = 25;
const FILTER_KEYS = [
  "from",
  "to",
  "kind",
  "status",
  "category_id",
  "account_id",
  "project_id",
  "person_id",
  "debt_id",
  "q",
  "sort",
] as const;

export function filtersFromParams(sp: URLSearchParams): TxFilters {
  const f: Record<string, string> = {};
  for (const k of FILTER_KEYS) {
    const v = sp.get(k);
    if (v) f[k] = v;
  }
  const offset = Number(sp.get("offset") ?? 0);
  return { ...(f as TxFilters), limit: PAGE, offset: Number.isFinite(offset) && offset > 0 ? offset : 0 };
}

export function TransactionsPage() {
  const [sp, setSp] = useSearchParams();
  const filters = useMemo(() => filtersFromParams(sp), [sp]);
  const q = useTransactions(filters);
  const accounts = useAccounts(true);
  const categories = useCategories(true);
  const projects = useProjectOptions();
  const people = usePeopleOptions();
  const debts = useDebts();
  const [creating, setCreating] = useState(false);
  const [batchOpen, setBatchOpen] = useState(false);
  const [selected, setSelected] = useState<Transaction | null>(null);

  const accName = (id?: string | null) => accounts.data?.find((a) => a.account.id === id)?.account.name;
  const catName = (id?: string | null) => categories.data?.find((c) => c.id === id)?.name;

  const setFilter = (k: string, v: string) => {
    const next = new URLSearchParams(sp);
    if (v) next.set(k, v);
    else next.delete(k);
    next.delete("offset");
    setSp(next, { replace: true });
  };
  const setOffset = (o: number) => {
    const next = new URLSearchParams(sp);
    if (o > 0) next.set("offset", String(o));
    else next.delete("offset");
    setSp(next);
  };
  const activeFilters = FILTER_KEYS.filter((k) => sp.get(k) && k !== "sort").length;

  return (
    <div>
      <PageHeader
        title="Movimientos"
        subtitle="Gastos, ingresos, transferencias, pagos y ajustes"
        actions={
          <>
            <Button variant="secondary" onClick={() => setBatchOpen(true)}>
              Captura múltiple
            </Button>
            <Button onClick={() => setCreating(true)}>+ Movimiento</Button>
          </>
        }
      />
      <FinanceNav />

      <Card title="Filtros" className="mb-4">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <TextField
            label="Buscar concepto"
            type="search"
            value={sp.get("q") ?? ""}
            onChange={(e) => setFilter("q", e.target.value)}
          />
          <TextField
            label="Desde"
            type="date"
            value={sp.get("from") ?? ""}
            onChange={(e) => setFilter("from", e.target.value)}
          />
          <TextField
            label="Hasta"
            type="date"
            value={sp.get("to") ?? ""}
            onChange={(e) => setFilter("to", e.target.value)}
          />
          <SelectField
            label="Tipo"
            value={sp.get("kind") ?? ""}
            onChange={(e) => setFilter("kind", e.target.value)}
          >
            <option value="">Todos</option>
            {TX_KINDS.map((k) => (
              <option key={k} value={k}>
                {TX_KIND_LABEL[k]}
              </option>
            ))}
          </SelectField>
          <SelectField
            label="Estado"
            value={sp.get("status") ?? ""}
            onChange={(e) => setFilter("status", e.target.value)}
          >
            <option value="">Confirmados y pendientes</option>
            {Object.entries(TX_STATUS_LABEL).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </SelectField>
          <SimpleSelect
            label="Categoría"
            value={sp.get("category_id") ?? ""}
            onChange={(v) => setFilter("category_id", v)}
            options={(categories.data ?? []).map((c) => ({
              id: c.id,
              name: `${c.name} (${c.kind === "expense" ? "gasto" : "ingreso"})`,
            }))}
            emptyLabel="Todas"
          />
          <SimpleSelect
            label="Cuenta o tarjeta"
            value={sp.get("account_id") ?? ""}
            onChange={(v) => setFilter("account_id", v)}
            options={(accounts.data ?? []).map((a) => ({ id: a.account.id, name: a.account.name }))}
            emptyLabel="Todas"
          />
          <SimpleSelect
            label="Proyecto"
            value={sp.get("project_id") ?? ""}
            onChange={(v) => setFilter("project_id", v)}
            options={(projects.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
            emptyLabel="Todos"
          />
          <SimpleSelect
            label="Persona"
            value={sp.get("person_id") ?? ""}
            onChange={(v) => setFilter("person_id", v)}
            options={(people.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
            emptyLabel="Todas"
          />
          <SimpleSelect
            label="Deuda"
            value={sp.get("debt_id") ?? ""}
            onChange={(v) => setFilter("debt_id", v)}
            options={(debts.data ?? []).map((d) => ({ id: d.debt.id, name: d.debt.name }))}
            emptyLabel="Todas"
          />
          <SelectField
            label="Orden"
            value={sp.get("sort") ?? "date_desc"}
            onChange={(e) => setFilter("sort", e.target.value === "date_desc" ? "" : e.target.value)}
          >
            <option value="date_desc">Más recientes</option>
            <option value="date_asc">Más antiguos</option>
            <option value="amount_desc">Monto mayor</option>
            <option value="amount_asc">Monto menor</option>
          </SelectField>
          {activeFilters > 0 && (
            <div className="flex items-end">
              <Button variant="ghost" onClick={() => setSp(new URLSearchParams(), { replace: true })}>
                Limpiar filtros ({activeFilters})
              </Button>
            </div>
          )}
        </div>
      </Card>

      {q.isPending ? (
        <LoadingState rows={5} />
      ) : q.isError ? (
        <ErrorState error={q.error} onRetry={() => q.refetch()} />
      ) : (
        <>
          <Card title="Totales del filtro" className="mb-4">
            <dl className="grid gap-x-6 sm:grid-cols-2">
              <Stat label="Movimientos">{q.data.totals.count}</Stat>
              <Stat label="Gastos confirmados" hint="Solo movimientos de tipo gasto">
                <Money cents={q.data.totals.expense_cents} />
              </Stat>
              <Stat label="Entradas confirmadas" hint="Ingresos y préstamos recibidos">
                <Money cents={q.data.totals.income_cents} />
              </Stat>
              <Stat label="Pendientes de conciliar" hint="No afectan saldos ni totales confirmados">
                <Money cents={q.data.totals.pending_cents} />
              </Stat>
              {q.data.totals.foreign_count > 0 && (
                <Stat label="En otra moneda sin conversión" hint="Excluidos de los totales">
                  {q.data.totals.foreign_count}
                </Stat>
              )}
            </dl>
          </Card>
          {q.data.items.length === 0 ? (
            <EmptyState
              title={activeFilters ? "Sin movimientos con estos filtros" : "Aún no hay movimientos"}
              action={<Button onClick={() => setCreating(true)}>Registrar el primero</Button>}
            />
          ) : (
            <TxList items={q.data.items} accName={accName} catName={catName} onOpen={setSelected} />
          )}
          {q.data.total > PAGE && (
            <nav className="mt-3 flex items-center justify-between gap-2" aria-label="Paginación">
              <Button
                variant="secondary"
                size="sm"
                disabled={(filters.offset ?? 0) === 0}
                onClick={() => setOffset(Math.max(0, (filters.offset ?? 0) - PAGE))}
              >
                Anterior
              </Button>
              <span className="text-sm text-muted" aria-live="polite">
                {(filters.offset ?? 0) + 1}–{Math.min((filters.offset ?? 0) + PAGE, q.data.total)} de{" "}
                {q.data.total}
              </span>
              <Button
                variant="secondary"
                size="sm"
                disabled={(filters.offset ?? 0) + PAGE >= q.data.total}
                onClick={() => setOffset((filters.offset ?? 0) + PAGE)}
              >
                Siguiente
              </Button>
            </nav>
          )}
        </>
      )}

      <TxFormDialog open={creating} onClose={() => setCreating(false)} />
      <BatchCaptureDialog open={batchOpen} onClose={() => setBatchOpen(false)} />
      {selected && (
        <TxDetailDialog tx={selected} accName={accName} catName={catName} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}

export function TxList({
  items,
  accName,
  catName,
  onOpen,
}: {
  items: Transaction[];
  accName: (id?: string | null) => string | undefined;
  catName: (id?: string | null) => string | undefined;
  onOpen: (t: Transaction) => void;
}) {
  const amount = (t: Transaction) => {
    const sign = flowSign(t.kind, t.adjustment_sign);
    return (
      <span className={t.status === "void" ? "line-through text-muted" : ""}>
        <Money
          cents={sign === 0 ? t.amount_cents : sign * t.amount_cents}
          currency={t.currency}
          signed={sign !== 0 && t.status === "confirmed"}
        />
      </span>
    );
  };
  const statusBadge = (t: Transaction) =>
    t.status === "confirmed" ? null : (
      <Badge tone={t.status === "void" ? "muted" : "warn"}>{TX_STATUS_LABEL[t.status] ?? t.status}</Badge>
    );
  const where = (t: Transaction) =>
    t.kind === "transfer" || t.kind === "card_payment"
      ? `${accName(t.account_id) ?? "?"} → ${accName(t.counter_account_id) ?? "?"}`
      : (accName(t.account_id) ?? (t.financed_by_person_id ? "Pagado por otra persona" : "—"));
  return (
    <>
      <div className="hidden overflow-x-auto rounded-xl border border-line md:block">
        <table className="w-full border-collapse text-left text-sm">
          <caption className="sr-only">Movimientos</caption>
          <thead className="bg-surface-2">
            <tr>
              <th scope="col" className="px-3 py-2">
                Fecha
              </th>
              <th scope="col" className="px-3 py-2">
                Concepto
              </th>
              <th scope="col" className="px-3 py-2">
                Tipo
              </th>
              <th scope="col" className="px-3 py-2">
                Categoría
              </th>
              <th scope="col" className="px-3 py-2">
                Cuenta
              </th>
              <th scope="col" className="px-3 py-2 text-right">
                Monto
              </th>
            </tr>
          </thead>
          <tbody>
            {items.map((t) => (
              <tr key={t.id} className="border-t border-line bg-surface hover:bg-surface-2">
                <td className="whitespace-nowrap px-3 py-2">{formatShortDate(t.occurred_on)}</td>
                <td className="px-3 py-2">
                  <button
                    type="button"
                    className="text-left text-link underline-offset-2 hover:underline"
                    onClick={() => onOpen(t)}
                  >
                    {t.concept}
                  </button>{" "}
                  {statusBadge(t)}
                </td>
                <td className="px-3 py-2">{TX_KIND_LABEL[t.kind as TxKind] ?? t.kind}</td>
                <td className="px-3 py-2">{catName(t.category_id) ?? "—"}</td>
                <td className="px-3 py-2">{where(t)}</td>
                <td className="px-3 py-2 text-right">{amount(t)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="space-y-2 md:hidden" aria-label="Movimientos">
        {items.map((t) => (
          <li key={t.id}>
            <button
              type="button"
              onClick={() => onOpen(t)}
              className="w-full rounded-xl border border-line bg-surface p-3 text-left"
            >
              <span className="flex items-start justify-between gap-3">
                <span className="min-w-0">
                  <span className="block break-words font-medium">{t.concept}</span>
                  <span className="block text-sm text-muted">
                    {formatShortDate(t.occurred_on)} · {TX_KIND_LABEL[t.kind as TxKind] ?? t.kind}
                    {catName(t.category_id) ? ` · ${catName(t.category_id)}` : ""}
                  </span>
                  <span className="block text-sm text-muted">{where(t)}</span>
                </span>
                <span className="shrink-0 text-right">
                  {amount(t)}
                  <span className="block">{statusBadge(t)}</span>
                </span>
              </span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

function TxDetailDialog({
  tx,
  accName,
  catName,
  onClose,
}: {
  tx: Transaction;
  accName: (id?: string | null) => string | undefined;
  catName: (id?: string | null) => string | undefined;
  onClose: () => void;
}) {
  const { voidTx } = useTxMutations();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [voiding, setVoiding] = useState(false);
  const [confirming, setConfirming] = useState(false);
  if (editing) return <TxFormDialog open editing={tx} onClose={onClose} />;
  if (confirming)
    return (
      <ConfirmOccurrenceDialog
        occurrence={{
          transaction_id: tx.id,
          transaction_version: tx.version,
          amount_cents: tx.amount_cents,
          date: tx.occurred_on,
          rule_name: tx.concept,
          account_id: tx.account_id ?? null,
        }}
        onClose={onClose}
      />
    );
  return (
    <>
      <Dialog
        open={!voiding}
        onClose={onClose}
        title={tx.concept}
        footer={
          tx.status === "void" ? (
            <Button variant="secondary" onClick={onClose}>
              Cerrar
            </Button>
          ) : (
            <>
              <Button variant="danger" onClick={() => setVoiding(true)}>
                Anular
              </Button>
              {tx.status === "pending_reconciliation" && (
                <Button variant="secondary" onClick={() => setConfirming(true)}>
                  Confirmar / corregir
                </Button>
              )}
              <Button onClick={() => setEditing(true)}>Editar</Button>
            </>
          )
        }
      >
        <dl>
          <Stat label="Tipo">{TX_KIND_LABEL[tx.kind as TxKind] ?? tx.kind}</Stat>
          <Stat label="Estado">{TX_STATUS_LABEL[tx.status] ?? tx.status}</Stat>
          <Stat label="Origen">{ORIGIN_LABEL[tx.origin] ?? tx.origin}</Stat>
          <Stat label="Fecha">{formatShortDate(tx.occurred_on)}</Stat>
          <Stat label="Monto">
            <Money cents={tx.amount_cents} currency={tx.currency} />
          </Stat>
          {tx.original_currency && (
            <Stat label="Monto original" hint={`Tasa ${tx.fx_rate} del ${tx.fx_date} · ${tx.fx_source}`}>
              <Money cents={tx.original_amount_cents ?? 0} currency={tx.original_currency} />
            </Stat>
          )}
          {tx.category_id && <Stat label="Categoría">{catName(tx.category_id) ?? "—"}</Stat>}
          {tx.account_id && <Stat label="Cuenta">{accName(tx.account_id) ?? "—"}</Stat>}
          {tx.counter_account_id && <Stat label="Destino">{accName(tx.counter_account_id) ?? "—"}</Stat>}
          {tx.financed_by_person_id && <Stat label="Pagado por">Otra persona (préstamo familiar)</Stat>}
          {tx.recurrence_period && <Stat label="Ocurrencia">{tx.recurrence_period}</Stat>}
          {tx.notes_md && <Stat label="Notas">{tx.notes_md}</Stat>}
        </dl>
        {tx.status === "pending_reconciliation" && (
          <p className="mt-3 text-sm text-muted">
            Generado por una regla recurrente: la fecha programada no prueba el cobro. No afecta saldos ni
            totales confirmados hasta que lo confirmes.
          </p>
        )}
      </Dialog>
      <ConfirmDialog
        open={voiding}
        danger
        title="Anular movimiento"
        message="Deja de contar en saldos y totales, pero se conserva en el historial."
        confirmLabel="Anular"
        loading={voidTx.isPending}
        onClose={() => setVoiding(false)}
        onConfirm={async () => {
          try {
            await voidTx.mutateAsync({ id: tx.id, version: tx.version });
            toast.show("Movimiento anulado", { tone: "ok" });
            onClose();
          } catch (e) {
            toast.show(errorMessage(e), { tone: "error" });
            setVoiding(false);
          }
        }}
      />
    </>
  );
}
