// Deudas: préstamos, deudas familiares y tarjetas; abonos, saldo y vínculo explícito con un objetivo (saldarla completa
// el hito; un abono parcial no). Abonar a una tarjeta se registra como pago de tarjeta (libera crédito, no es consumo).
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader, ProgressBar } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  AccountSelect,
  FinanceNav,
  Money,
  MoneyField,
  SimpleSelect,
  Stat,
  useVersionedSave,
} from "./components";
import {
  type DebtView,
  useAccounts,
  useDebtMutations,
  useDebts,
  useGoalSearch,
  usePeopleOptions,
} from "./financeApi";
import { DEBT_KIND_LABEL, DEBT_STATUS_LABEL } from "./labels";
import { centsToInput, formatCents, parseMoney } from "./money";
import { type TxDefaults, TxFormDialog } from "./TxFormDialog";

type Debt = Schemas["Debt"];

export function DebtsPage() {
  const q = useDebts();
  const [editing, setEditing] = useState<Debt | "new" | null>(null);
  const [paying, setPaying] = useState<TxDefaults | null>(null);
  return (
    <div>
      <PageHeader
        title="Deudas"
        subtitle="Préstamos, tarjetas y deudas familiares"
        actions={<Button onClick={() => setEditing("new")}>+ Deuda</Button>}
      />
      <FinanceNav />
      <QueryState
        query={q}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Sin deudas registradas"
            action={<Button onClick={() => setEditing("new")}>Registrar deuda</Button>}
          >
            Si alguien te prestó para un gasto, el gasto ya cuenta como consumo; aquí solo das seguimiento a
            lo que debes.
          </EmptyState>
        }
      >
        {(list) => (
          <div className="grid gap-3 md:grid-cols-2">
            {list.map((v) => (
              <DebtCard
                key={v.debt.id}
                v={v}
                onEdit={() => setEditing(v.debt)}
                onPay={() =>
                  setPaying(
                    v.debt.kind === "credit_card" && v.debt.account_id
                      ? {
                          kind: "card_payment",
                          counter_account_id: v.debt.account_id,
                          concept: `Pago ${v.debt.name}`,
                        }
                      : { kind: "debt_payment", debt_id: v.debt.id, concept: `Abono ${v.debt.name}` },
                  )
                }
              />
            ))}
          </div>
        )}
      </QueryState>
      {editing && <DebtDialog debt={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      <TxFormDialog open={!!paying} defaults={paying ?? {}} onClose={() => setPaying(null)} />
    </div>
  );
}

function DebtCard({ v, onEdit, onPay }: { v: DebtView; onEdit: () => void; onPay: () => void }) {
  const d = v.debt;
  const principal = d.principal_cents ?? null;
  return (
    <Card
      title={
        <span className="flex flex-wrap items-center gap-2">
          {d.name}
          <Badge>{DEBT_KIND_LABEL[d.kind] ?? d.kind}</Badge>
          <Badge tone={d.status === "paid" ? "ok" : d.status === "open" ? "warn" : "muted"}>
            {DEBT_STATUS_LABEL[d.status] ?? d.status}
          </Badge>
        </span>
      }
    >
      <dl>
        {principal != null && (
          <Stat label="Monto original">
            <Money cents={principal} currency={d.currency} />
          </Stat>
        )}
        <Stat label="Abonado (confirmado)">
          <Money cents={v.paid_cents} currency={d.currency} />
        </Stat>
        <Stat
          label="Saldo pendiente"
          hint={d.kind === "credit_card" ? "Límite − disponible estimado" : undefined}
        >
          {v.remaining_cents != null ? (
            <Money cents={v.remaining_cents} currency={d.currency} />
          ) : (
            "Desconocido (registra el disponible de la tarjeta)"
          )}
        </Stat>
        {v.last_payment_on && <Stat label="Último abono">{formatShortDate(v.last_payment_on)}</Stat>}
        <Stat label="Desde">{formatShortDate(d.opened_on)}</Stat>
        {d.paid_on && <Stat label="Saldada el">{formatShortDate(d.paid_on)}</Stat>}
        {d.goal_id && <Stat label="Objetivo vinculado">Se completa al saldarla</Stat>}
      </dl>
      {principal != null && principal > 0 && (
        <div className="mt-3">
          <ProgressBar
            label="Avance"
            value={Math.min(v.paid_cents, principal)}
            max={principal}
            tone="ok"
            valueText={`${formatCents(v.paid_cents)} de ${formatCents(principal)}`}
          />
        </div>
      )}
      <div className="mt-3 flex flex-wrap gap-2">
        {d.status === "open" && (
          <Button size="sm" onClick={onPay}>
            {d.kind === "credit_card" ? "Pagar tarjeta" : "Abonar"}
          </Button>
        )}
        <Link
          className="inline-flex min-h-9 items-center text-sm"
          to={`/finanzas/movimientos?debt_id=${d.id}`}
        >
          Ver abonos
        </Link>
        <Button size="sm" variant="secondary" onClick={onEdit}>
          Editar
        </Button>
      </div>
    </Card>
  );
}

function GoalPicker({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  const [q, setQ] = useState("");
  const results = useGoalSearch(q);
  return (
    <div className="sm:col-span-2">
      <TextField
        label="Objetivo que se completa al saldarla (opcional)"
        type="search"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Busca por título del objetivo…"
        hint={
          value
            ? "Hay un objetivo vinculado. Elige otro o quítalo."
            : "Abono parcial no completa el objetivo."
        }
      />
      {value && (
        <Button size="sm" variant="ghost" className="mt-1" onClick={() => onChange("")}>
          Quitar vínculo
        </Button>
      )}
      {results.data && results.data.length > 0 && (
        <ul className="mt-2 divide-y divide-line rounded-lg border border-line">
          {results.data.map((h) => (
            <li key={h.entity_id}>
              <button
                type="button"
                className={`w-full px-3 py-2 text-left ${h.entity_id === value ? "bg-surface-2 font-medium" : ""}`}
                onClick={() => {
                  onChange(h.entity_id);
                  setQ("");
                }}
              >
                {h.title}
                {h.status ? <span className="ml-2 text-sm text-muted">{h.status}</span> : null}
              </button>
            </li>
          ))}
        </ul>
      )}
      {results.data && results.data.length === 0 && q.trim().length >= 2 && (
        <p className="mt-1 text-sm text-muted">Sin objetivos con ese texto.</p>
      )}
    </div>
  );
}

const DEBT_LABELS: Record<string, string> = {
  name: "Nombre",
  principal_cents: "Monto original",
  opened_on: "Desde",
  goal_id: "Objetivo",
  notes_md: "Notas",
  cancelled: "Cancelada",
};

function debtBody(d: Debt, patch: Partial<Schemas["DebtBody"]> = {}): Schemas["DebtBody"] {
  return {
    name: d.name,
    kind: d.kind,
    account_id: d.account_id ?? null,
    person_id: d.person_id ?? null,
    principal_cents: d.principal_cents ?? null,
    currency: d.currency,
    opened_on: d.opened_on,
    goal_id: d.goal_id ?? null,
    notes_md: d.notes_md,
    cancelled: d.status === "cancelled",
    ...patch,
    version: d.version,
  };
}

function DebtDialog({ debt, onClose }: { debt: Debt | null; onClose: () => void }) {
  const { me } = useAuth();
  const accounts = useAccounts();
  const people = usePeopleOptions();
  const { create, update } = useDebtMutations();
  const toast = useToast();
  const [f, setF] = useState({
    name: debt?.name ?? "",
    kind: debt?.kind ?? "loan",
    account_id: debt?.account_id ?? "",
    person_id: debt?.person_id ?? "",
    principal: centsToInput(debt?.principal_cents ?? null),
    opened_on: debt?.opened_on ?? todayIso(me?.settings.timezone),
    goal_id: debt?.goal_id ?? "",
    notes_md: debt?.notes_md ?? "",
    cancelled: debt?.status === "cancelled",
  });
  const [err, setErr] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const set = (k: keyof typeof f, v: string | boolean) => setF((x) => ({ ...x, [k]: v }));
  const card = f.kind === "credit_card";

  const versioned = useVersionedSave<Schemas["DebtBody"]>({
    save: (body) => update.mutateAsync({ id: debt?.id ?? "", body }),
    labels: DEBT_LABELS,
    toBody: (cur) => {
      const c = cur as Debt | DebtView;
      return debtBody("debt" in c ? c.debt : c);
    },
    onSaved: () => {
      toast.show("Deuda actualizada", { tone: "ok" });
      onClose();
    },
    onError: (e) => setFormError(errorMessage(e)),
  });

  const submit = async () => {
    const e: Record<string, string> = {};
    if (!f.name.trim()) e.name = "Nombre de la deuda";
    let principal: number | null = null;
    if (!card) {
      const p = parseMoney(f.principal);
      if (!p.ok) e.principal = p.error;
      else principal = p.cents;
    } else if (!f.account_id) e.account_id = "Elige la tarjeta";
    if (f.kind === "family" && !f.person_id) e.person_id = "¿Quién te prestó?";
    setErr(e);
    if (Object.keys(e).length) return;
    const body: Schemas["DebtBody"] = {
      name: f.name.trim(),
      kind: f.kind,
      account_id: card ? f.account_id || null : null,
      person_id: f.person_id || null,
      principal_cents: principal,
      currency: debt?.currency ?? me?.settings.currency ?? "MXN",
      opened_on: f.opened_on,
      goal_id: f.goal_id || null,
      notes_md: f.notes_md,
      cancelled: f.cancelled,
    };
    try {
      if (debt) await versioned.run({ ...body, version: debt.version });
      else {
        await create.mutateAsync(body);
        toast.show("Deuda registrada", { tone: "ok" });
        onClose();
      }
    } catch (x) {
      if (x instanceof ApiError && x.field)
        setErr({ [x.field === "principal_cents" ? "principal" : x.field]: x.message });
      else setFormError(errorMessage(x));
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={debt ? `Editar ${debt.name}` : "Nueva deuda"}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={create.isPending || update.isPending}>
            Guardar
          </Button>
        </>
      }
    >
      {formError && (
        <p role="alert" className="mb-3 text-sm text-danger">
          {formError}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          label="Nombre"
          value={f.name}
          onChange={(e) => set("name", e.target.value)}
          error={err.name}
        />
        <SelectField
          label="Tipo"
          value={f.kind}
          disabled={!!debt}
          onChange={(e) => set("kind", e.target.value)}
        >
          {Object.entries(DEBT_KIND_LABEL).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        {card ? (
          <AccountSelect
            label="Tarjeta"
            value={f.account_id}
            onChange={(v) => set("account_id", v)}
            accounts={accounts.data ?? []}
            filter={(a) => a.account.kind === "credit_card"}
            error={err.account_id}
            emptyLabel="Elige la tarjeta"
            hint="El saldo se deriva del disponible estimado de la tarjeta."
          />
        ) : (
          <MoneyField
            label="Monto original"
            value={f.principal}
            onChange={(e) => set("principal", e.target.value)}
            error={err.principal}
          />
        )}
        <SimpleSelect
          label={f.kind === "family" ? "¿Quién te prestó?" : "Acreedor (persona, opcional)"}
          value={f.person_id}
          onChange={(v) => set("person_id", v)}
          options={(people.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
          emptyLabel="Ninguna"
          error={err.person_id}
        />
        <TextField
          label="Desde"
          type="date"
          value={f.opened_on}
          onChange={(e) => set("opened_on", e.target.value)}
        />
        <GoalPicker value={f.goal_id} onChange={(v) => set("goal_id", v)} />
        <TextArea
          label="Notas"
          className="sm:col-span-2"
          rows={2}
          value={f.notes_md}
          onChange={(e) => set("notes_md", e.target.value)}
        />
        {debt && (
          <CheckboxField
            label="Cancelada (ya no se da seguimiento)"
            checked={f.cancelled}
            onChange={(e) => set("cancelled", e.target.checked)}
          />
        )}
      </div>
      {versioned.dialog}
    </Dialog>
  );
}
