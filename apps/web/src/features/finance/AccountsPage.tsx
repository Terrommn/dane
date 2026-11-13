// Cuentas y tarjetas: alta/edición, conciliación manual (saldo observado con su momento), saldo/disponible estimado
// con la antigüedad del dato, uso %, próximos corte y pago. No hay conexión con bancos: todo dato lo aporta el usuario.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader, ProgressBar } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { FinanceNav, Money, MoneyField, Stat, useVersionedSave } from "./components";
import {
  type Account,
  type AccountView,
  accountBody,
  useAccountMutations,
  useAccounts,
  useSnapshots,
} from "./financeApi";
import { ACCOUNT_KIND_LABEL, ACCOUNT_STATUS_LABEL, SEMANTICS_LABEL } from "./labels";
import { centsToInput, formatCents, parseMoney } from "./money";

export function AccountsPage() {
  const [showArchived, setShowArchived] = useState(false);
  const q = useAccounts(showArchived);
  const [editing, setEditing] = useState<Account | "new" | null>(null);
  const [snapshotFor, setSnapshotFor] = useState<AccountView | null>(null);
  const [historyFor, setHistoryFor] = useState<AccountView | null>(null);
  return (
    <div>
      <PageHeader
        title="Cuentas y tarjetas"
        subtitle="Saldos observados por ti y estimados con tus movimientos (sin conexión al banco)"
        actions={<Button onClick={() => setEditing("new")}>+ Cuenta o tarjeta</Button>}
      />
      <FinanceNav />
      <CheckboxField
        className="mb-3"
        label="Mostrar archivadas"
        checked={showArchived}
        onChange={(e) => setShowArchived(e.target.checked)}
      />
      <QueryState
        query={q}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Sin cuentas todavía"
            action={<Button onClick={() => setEditing("new")}>Agregar cuenta o tarjeta</Button>}
          >
            Registra tus cuentas de débito y tus tarjetas para ver saldos estimados y la tarjeta sugerida del
            día.
          </EmptyState>
        }
      >
        {(list) => (
          <div className="grid gap-3 md:grid-cols-2">
            {list.map((v) => (
              <AccountCard
                key={v.account.id}
                v={v}
                onEdit={() => setEditing(v.account)}
                onSnapshot={() => setSnapshotFor(v)}
                onHistory={() => setHistoryFor(v)}
              />
            ))}
          </div>
        )}
      </QueryState>
      {editing && (
        <AccountFormDialog account={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
      {snapshotFor && <SnapshotDialog v={snapshotFor} onClose={() => setSnapshotFor(null)} />}
      {historyFor && <SnapshotHistoryDialog v={historyFor} onClose={() => setHistoryFor(null)} />}
    </div>
  );
}

export function AccountCard({
  v,
  onEdit,
  onSnapshot,
  onHistory,
}: {
  v: AccountView;
  onEdit: () => void;
  onSnapshot: () => void;
  onHistory: () => void;
}) {
  const a = v.account;
  const card = a.kind === "credit_card";
  const age = v.snapshot_age_days;
  return (
    <Card
      title={
        <span className="flex flex-wrap items-center gap-2">
          {a.name}
          <Badge>{ACCOUNT_KIND_LABEL[a.kind] ?? a.kind}</Badge>
          {a.status !== "active" && <Badge tone="warn">{ACCOUNT_STATUS_LABEL[a.status] ?? a.status}</Badge>}
          {a.is_default_debit && <Badge tone="accent">Débito por defecto</Badge>}
          {a.archived_at && <Badge>Archivada</Badge>}
        </span>
      }
    >
      {a.bank && <p className="-mt-2 mb-2 text-sm text-muted">{a.bank}</p>}
      <dl>
        <Stat
          label={card ? "Disponible estimado" : "Saldo estimado"}
          hint="Observado + movimientos confirmados posteriores; no es un dato del banco"
        >
          {v.estimated_cents != null ? (
            <Money cents={v.estimated_cents} currency={a.currency} />
          ) : (
            "Sin saldo observado"
          )}
        </Stat>
        {v.snapshot && (
          <Stat
            label={card ? "Disponible observado" : "Saldo observado"}
            hint={`${formatDateTime(v.snapshot.observed_at)}${age != null ? ` · hace ${age} día${age === 1 ? "" : "s"}` : ""}`}
          >
            <Money cents={v.snapshot.balance_cents} currency={a.currency} />
          </Stat>
        )}
        {card && (
          <>
            <Stat label="Límite">
              <Money cents={a.credit_limit_cents ?? null} currency={a.currency} />
            </Stat>
            {v.used_cents != null && (
              <Stat label="Usado">
                <Money cents={v.used_cents} currency={a.currency} />
              </Stat>
            )}
            <Stat
              label="Corte / pago"
              hint={v.days_to_pay != null ? `Pago en ${v.days_to_pay} días` : undefined}
            >
              {a.cut_day ? `día ${a.cut_day}` : "sin corte"} / {a.pay_day ? `día ${a.pay_day}` : "—"}
            </Stat>
            {v.next_cut_on && <Stat label="Próximo corte">{formatShortDate(v.next_cut_on)}</Stat>}
            {v.next_pay_on && <Stat label="Próximo pago">{formatShortDate(v.next_pay_on)}</Stat>}
          </>
        )}
      </dl>
      {card && v.usage_pct != null && a.credit_limit_cents ? (
        <div className="mt-3">
          <ProgressBar
            label="Uso de la línea"
            value={Math.max(0, v.usage_pct)}
            max={100}
            tone={v.usage_pct >= 80 ? "danger" : v.usage_pct >= 30 ? "warn" : "ok"}
            valueText={`${v.usage_pct}%`}
          />
        </div>
      ) : null}
      {age != null && age > 14 && (
        <p className="mt-2 text-sm text-warn">
          El saldo observado tiene {age} días: concilia para mejorar el estimado.
        </p>
      )}
      <div className="mt-3 flex flex-wrap gap-2">
        <Button size="sm" onClick={onSnapshot}>
          {card ? "Registrar disponible" : "Registrar saldo"}
        </Button>
        <Button size="sm" variant="secondary" onClick={onHistory}>
          Historial
        </Button>
        <Button size="sm" variant="secondary" onClick={onEdit}>
          Editar
        </Button>
      </div>
    </Card>
  );
}

const ACCOUNT_LABELS: Record<string, string> = {
  name: "Nombre",
  bank: "Banco",
  credit_limit_cents: "Límite",
  cut_day: "Día de corte",
  pay_day: "Día de pago",
  status: "Estado",
  is_default_debit: "Débito por defecto",
  notes_md: "Notas",
  archived: "Archivada",
};

function AccountFormDialog({ account, onClose }: { account: Account | null; onClose: () => void }) {
  const { me } = useAuth();
  const { create, update } = useAccountMutations();
  const toast = useToast();
  const [f, setF] = useState({
    name: account?.name ?? "",
    bank: account?.bank ?? "",
    kind: account?.kind ?? "debit",
    currency: account?.currency ?? me?.settings.currency ?? "MXN",
    limit: centsToInput(account?.credit_limit_cents ?? null),
    cut_day: account?.cut_day?.toString() ?? "",
    pay_day: account?.pay_day?.toString() ?? "",
    status: account?.status ?? "active",
    is_default_debit: account?.is_default_debit ?? false,
    notes_md: account?.notes_md ?? "",
    archived: account?.archived_at != null,
  });
  const [err, setErr] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const card = f.kind === "credit_card";
  const set = (k: keyof typeof f, v: string | boolean) => setF((x) => ({ ...x, [k]: v }));

  const versioned = useVersionedSave<Schemas["AccountBody"]>({
    save: (body) => update.mutateAsync({ id: account?.id ?? "", body }),
    labels: ACCOUNT_LABELS,
    toBody: (cur) => {
      const v = cur as AccountView | Account;
      const acc = "account" in v ? v.account : v;
      return accountBody(acc);
    },
    onSaved: () => {
      toast.show("Cuenta actualizada", { tone: "ok" });
      onClose();
    },
    onError: (e) => setFormError(errorMessage(e)),
  });

  const submit = async () => {
    const e: Record<string, string> = {};
    if (!f.name.trim()) e.name = "Ponle un nombre";
    let limit: number | null = null;
    if (card) {
      const p = parseMoney(f.limit);
      if (!p.ok) e.limit = p.error;
      else limit = p.cents;
      for (const k of ["cut_day", "pay_day"] as const) {
        const n = Number(f[k]);
        if (f[k] && (!Number.isInteger(n) || n < 1 || n > 31)) e[k] = "Día del 1 al 31";
      }
    }
    if (!/^[A-Za-z]{3}$/.test(f.currency)) e.currency = "Moneda ISO de 3 letras";
    setErr(e);
    if (Object.keys(e).length) return;
    const body: Schemas["AccountBody"] = {
      name: f.name.trim(),
      bank: f.bank.trim(),
      kind: f.kind,
      currency: f.currency.toUpperCase(),
      credit_limit_cents: card ? limit : null,
      cut_day: card && f.cut_day ? Number(f.cut_day) : null,
      pay_day: card && f.pay_day ? Number(f.pay_day) : null,
      status: f.status,
      is_default_debit: !card && f.is_default_debit,
      notes_md: f.notes_md,
      archived: f.archived,
    };
    try {
      if (account) await versioned.run({ ...body, version: account.version });
      else {
        await create.mutateAsync(body);
        toast.show("Cuenta creada. Registra su saldo observado para ver el estimado.", { tone: "ok" });
        onClose();
      }
    } catch (x) {
      if (x instanceof ApiError && x.field) setErr({ [x.field]: x.message });
      else setFormError(errorMessage(x));
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={account ? `Editar ${account.name}` : "Nueva cuenta o tarjeta"}
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
        <TextField label="Banco" value={f.bank} onChange={(e) => set("bank", e.target.value)} />
        <SelectField
          label="Tipo"
          value={f.kind}
          disabled={!!account}
          hint={account ? "El tipo no cambia." : undefined}
          onChange={(e) => set("kind", e.target.value)}
        >
          {Object.entries(ACCOUNT_KIND_LABEL).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Moneda"
          value={f.currency}
          maxLength={3}
          disabled={!!account}
          onChange={(e) => set("currency", e.target.value.toUpperCase())}
          error={err.currency}
        />
        {card && (
          <>
            <MoneyField
              label="Límite de crédito"
              value={f.limit}
              onChange={(e) => set("limit", e.target.value)}
              error={err.limit ?? err.credit_limit_cents}
            />
            <TextField
              label="Día de corte"
              inputMode="numeric"
              value={f.cut_day}
              onChange={(e) => set("cut_day", e.target.value)}
              error={err.cut_day}
              hint="Sin día de corte la tarjeta no se sugiere."
            />
            <TextField
              label="Día de pago"
              inputMode="numeric"
              value={f.pay_day}
              onChange={(e) => set("pay_day", e.target.value)}
              error={err.pay_day}
            />
          </>
        )}
        <SelectField label="Estado" value={f.status} onChange={(e) => set("status", e.target.value)}>
          {Object.entries(ACCOUNT_STATUS_LABEL).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        {!card && (
          <CheckboxField
            className="sm:col-span-2"
            label="Débito por defecto"
            hint="Se sugiere cuando ninguna tarjeta conviene."
            checked={f.is_default_debit}
            onChange={(e) => set("is_default_debit", e.target.checked)}
          />
        )}
        <TextArea
          label="Notas"
          className="sm:col-span-2"
          rows={2}
          value={f.notes_md}
          onChange={(e) => set("notes_md", e.target.value)}
        />
        {account && (
          <CheckboxField
            label="Archivada"
            checked={f.archived}
            onChange={(e) => set("archived", e.target.checked)}
          />
        )}
      </div>
      {versioned.dialog}
    </Dialog>
  );
}

/** "2026-11-12T10:30" local → ISO UTC. */
function localInputToIso(v: string): string | null {
  if (!v) return null;
  const d = new Date(v);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}
function nowLocalInput(): string {
  const d = new Date();
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset());
  return d.toISOString().slice(0, 16);
}

function SnapshotDialog({ v, onClose }: { v: AccountView; onClose: () => void }) {
  const card = v.account.kind === "credit_card";
  const { snapshot } = useAccountMutations();
  const toast = useToast();
  const [amount, setAmount] = useState("");
  const [negative, setNegative] = useState(false);
  const [at, setAt] = useState(nowLocalInput());
  const [semantics, setSemantics] = useState("instant");
  const [baseDay, setBaseDay] = useState(at.slice(0, 10));
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const p = parseMoney(amount, { allowZero: true });
    if (!p.ok) return setError(p.error);
    const cents = negative && !card ? -p.cents : p.cents;
    try {
      const res = await snapshot.mutateAsync({
        id: v.account.id,
        body: {
          balance_cents: cents,
          observed_at: semantics === "instant" ? localInputToIso(at) : null,
          observed_on: semantics === "instant" ? null : baseDay,
          semantics,
          note,
        },
      });
      toast.show(
        `Registrado. ${card ? "Disponible" : "Saldo"} estimado: ${formatCents(res.estimated_cents ?? cents, v.account.currency)}`,
        { tone: "ok" },
      );
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={`${card ? "Disponible" : "Saldo"} observado · ${v.account.name}`}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={snapshot.isPending}>
            Registrar
          </Button>
        </>
      }
    >
      <p className="mb-3 text-sm text-muted">
        {card
          ? "Escribe el crédito disponible que ves en la app del banco."
          : "Escribe el saldo que ves en la app del banco."}{" "}
        Los movimientos confirmados posteriores a este momento se suman o restan para estimar el actual.
      </p>
      {error && (
        <p role="alert" className="mb-3 text-sm font-medium text-danger">
          {error}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <MoneyField
          label={card ? "Disponible" : "Saldo"}
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
        {!card && (
          <CheckboxField
            label="Saldo negativo (sobregiro)"
            checked={negative}
            onChange={(e) => setNegative(e.target.checked)}
          />
        )}
        <SelectField
          label="Tipo de observación"
          value={semantics}
          onChange={(e) => setSemantics(e.target.value)}
          className="sm:col-span-2"
        >
          <option value="instant">{SEMANTICS_LABEL.instant}</option>
          <option value={card ? "legacy_after_day" : "legacy_day_start_inclusive"}>
            {SEMANTICS_LABEL[card ? "legacy_after_day" : "legacy_day_start_inclusive"]}
          </option>
        </SelectField>
        {semantics === "instant" ? (
          <TextField
            label="Momento de la observación"
            type="datetime-local"
            value={at}
            onChange={(e) => setAt(e.target.value)}
          />
        ) : (
          <TextField
            label="Día base"
            type="date"
            value={baseDay}
            onChange={(e) => setBaseDay(e.target.value)}
            hint={
              card
                ? "Cuentan los movimientos desde el día siguiente."
                : "Cuentan los movimientos desde ese mismo día."
            }
          />
        )}
        <TextField label="Nota" value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
    </Dialog>
  );
}

function SnapshotHistoryDialog({ v, onClose }: { v: AccountView; onClose: () => void }) {
  const q = useSnapshots(v.account.id);
  return (
    <Dialog open onClose={onClose} title={`Historial de observaciones · ${v.account.name}`}>
      <QueryState
        query={q}
        isEmpty={(d) => d.length === 0}
        empty={<EmptyState title="Sin observaciones registradas" />}
      >
        {(list) => (
          <ul className="divide-y divide-line">
            {list.map((s) => (
              <li key={s.id} className="flex flex-wrap justify-between gap-2 py-2 text-sm">
                <span>
                  <span className="block">{formatDateTime(s.observed_at)}</span>
                  <span className="block text-muted">
                    {SEMANTICS_LABEL[s.semantics] ?? s.semantics}
                    {s.note ? ` · ${s.note}` : ""}
                  </span>
                </span>
                <Money cents={s.balance_cents} currency={v.account.currency} />
              </li>
            ))}
          </ul>
        )}
      </QueryState>
    </Dialog>
  );
}
