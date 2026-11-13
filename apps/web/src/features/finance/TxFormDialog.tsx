// Alta y edición de un movimiento de cualquier tipo. Alta: `client_key` estable por diálogo (reintentar no duplica).
// Edición: versión obligatoria y resolución explícita de conflictos (409).
import { useMemo, useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { CheckboxField, SelectField, TextArea, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { AccountSelect, CategorySelect, MoneyField, SimpleSelect, useVersionedSave } from "./components";
import {
  newClientKey,
  type Transaction,
  useAccounts,
  useBudget,
  useCategories,
  useDebts,
  usePeopleOptions,
  useProjectOptions,
  useTxMutations,
} from "./financeApi";
import { TX_KIND_HINT, TX_KIND_LABEL, TX_KINDS, type TxKind } from "./labels";
import { buildTxBody, emptyTxForm, type TxBody, type TxForm, txToForm } from "./txForm";

export interface TxDefaults {
  kind?: TxKind;
  debt_id?: string;
  account_id?: string;
  counter_account_id?: string;
  concept?: string;
  occurred_on?: string;
}

const FIELD_LABELS: Record<string, string> = {
  concept: "Concepto",
  occurred_on: "Fecha",
  amount_cents: "Monto",
  category_id: "Categoría",
  account_id: "Cuenta",
  counter_account_id: "Cuenta destino",
  notes_md: "Notas",
  project_id: "Proyecto",
  budget_item_id: "Rubro",
};

/** Mapea el campo de un 422 del servidor al campo del formulario. */
function fieldOf(serverField: string | null): keyof TxForm | null {
  if (!serverField) return null;
  const f = serverField.replace(/^items\[\d+\]\./, "");
  if (f === "amount_cents" || f === "amount") return "amount";
  if (f.startsWith("original.")) {
    const k = f.slice("original.".length);
    return (
      (
        {
          fx_rate: "fx_rate",
          fx_source: "fx_source",
          amount_cents: "fx_amount",
          currency: "fx_currency",
        } as const
      )[k as "fx_rate"] ?? "fx_amount"
    );
  }
  const known: (keyof TxForm)[] = [
    "concept",
    "occurred_on",
    "category_id",
    "account_id",
    "counter_account_id",
    "debt_id",
    "currency",
    "financed_by_person_id",
    "budget_item_id",
    "adjustment_sign",
  ];
  return (known as string[]).includes(f) ? (f as keyof TxForm) : null;
}

export function TxFormDialog({
  open,
  onClose,
  editing,
  defaults,
}: {
  open: boolean;
  onClose: () => void;
  editing?: Transaction | null;
  defaults?: TxDefaults;
}) {
  if (!open) return null;
  return <TxFormBody onClose={onClose} editing={editing ?? null} defaults={defaults ?? {}} />;
}

function TxFormBody({
  onClose,
  editing,
  defaults,
}: {
  onClose: () => void;
  editing: Transaction | null;
  defaults: TxDefaults;
}) {
  const { me } = useAuth();
  const base = me?.settings.currency ?? "MXN";
  const today = todayIso(me?.settings.timezone);
  const accounts = useAccounts();
  const categories = useCategories();
  const people = usePeopleOptions();
  const projects = useProjectOptions();
  const debts = useDebts();
  const { create, update } = useTxMutations();
  const toast = useToast();
  const [clientKey] = useState(newClientKey);
  const [form, setForm] = useState<TxForm>(() => {
    if (editing) return txToForm(editing);
    const f = emptyTxForm(defaults.occurred_on ?? today, base, defaults.kind ?? "expense");
    return {
      ...f,
      debt_id: defaults.debt_id ?? "",
      account_id: defaults.account_id ?? "",
      counter_account_id: defaults.counter_account_id ?? "",
      concept: defaults.concept ?? "",
    };
  });
  const [errors, setErrors] = useState<Partial<Record<keyof TxForm, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const month = form.occurred_on.slice(0, 7);
  const budget = useBudget(/^\d{4}-\d{2}$/.test(month) ? month : today.slice(0, 7));
  const rubros = useMemo(
    () => (budget.data?.items ?? []).filter((i) => i.project_id === form.project_id),
    [budget.data, form.project_id],
  );

  const set = <K extends keyof TxForm>(k: K, v: TxForm[K]) => {
    setForm((f) => ({ ...f, [k]: v }));
    setErrors((e) => ({ ...e, [k]: undefined }));
  };

  const accs = accounts.data ?? [];
  const ctx = {
    accounts: accs.map((a) => ({ id: a.account.id, kind: a.account.kind, currency: a.account.currency })),
    categories: (categories.data ?? []).map((c) => ({ id: c.id, kind: c.kind })),
    baseCurrency: base,
  };

  const versioned = useVersionedSave<TxBody>({
    save: (body) => update.mutateAsync({ id: editing?.id ?? "", body }),
    labels: FIELD_LABELS,
    toBody: (cur) => {
      const t = cur as Transaction;
      const r = buildTxBody(txToForm(t), ctx);
      return { ...(r.ok ? r.body : ({} as TxBody)), version: t.version };
    },
    onSaved: () => {
      toast.show("Movimiento actualizado", { tone: "ok" });
      onClose();
    },
    onError: (e) => setFormError(errorMessage(e)),
  });

  const submit = async () => {
    setFormError(null);
    const r = buildTxBody(form, ctx);
    if (!r.ok) {
      setErrors(r.errors);
      return;
    }
    try {
      if (editing) {
        await versioned.run({ ...r.body, version: editing.version });
      } else {
        const res = await create.mutateAsync({ ...r.body, client_key: clientKey });
        toast.show(
          res.created
            ? `${TX_KIND_LABEL[form.kind]} registrado`
            : "Ya estaba registrado (reintento sin duplicar)",
          { tone: "ok" },
        );
        if (res.notice) toast.show(res.notice, { sticky: true });
        onClose();
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 422) {
        const k = fieldOf(e.field);
        if (k) setErrors((x) => ({ ...x, [k]: e.message }));
        else setFormError(e.message);
      } else setFormError(errorMessage(e));
    }
  };

  const k = form.kind;
  const isCard = (id: string) => accs.find((a) => a.account.id === id)?.account.kind === "credit_card";
  const cashOnly = (a: (typeof accs)[number]) => a.account.kind !== "credit_card";
  const cardsOnly = (a: (typeof accs)[number]) => a.account.kind === "credit_card";

  return (
    <Dialog
      open
      onClose={onClose}
      size="lg"
      title={editing ? `Editar ${TX_KIND_LABEL[k].toLowerCase()}` : "Nuevo movimiento"}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={create.isPending || update.isPending}>
            {editing ? "Guardar cambios" : "Registrar"}
          </Button>
        </>
      }
    >
      <form
        className="grid gap-3 sm:grid-cols-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        {formError && (
          <p role="alert" className="rounded-lg border border-danger p-2 text-sm text-danger sm:col-span-2">
            {formError}
          </p>
        )}
        <SelectField
          label="Tipo"
          value={k}
          disabled={!!editing}
          hint={editing ? "El tipo no cambia al editar." : TX_KIND_HINT[k]}
          onChange={(e) => set("kind", e.target.value as TxKind)}
          className="sm:col-span-2"
        >
          {TX_KINDS.map((x) => (
            <option key={x} value={x}>
              {TX_KIND_LABEL[x]}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Fecha real del movimiento"
          type="date"
          value={form.occurred_on}
          onChange={(e) => set("occurred_on", e.target.value)}
          error={errors.occurred_on}
        />
        <TextField
          label="Concepto"
          value={form.concept}
          maxLength={200}
          onChange={(e) => set("concept", e.target.value)}
          error={errors.concept}
        />

        <CheckboxField
          className="sm:col-span-2"
          label="Monto en otra moneda (conversión explícita)"
          hint="Se guarda el monto original y la tasa que indiques; nunca se inventa una cotización."
          checked={form.useFx}
          onChange={(e) => set("useFx", e.target.checked)}
        />
        {form.useFx ? (
          <>
            <TextField
              label="Moneda original"
              value={form.fx_currency}
              maxLength={3}
              onChange={(e) => set("fx_currency", e.target.value.toUpperCase())}
              error={errors.fx_currency}
            />
            <MoneyField
              label={`Monto en ${form.fx_currency || "moneda original"}`}
              value={form.fx_amount}
              onChange={(e) => set("fx_amount", e.target.value)}
              error={errors.fx_amount}
            />
            <TextField
              label={`Tasa (${base} por 1 ${form.fx_currency || "unidad"})`}
              inputMode="decimal"
              value={form.fx_rate}
              onChange={(e) => set("fx_rate", e.target.value)}
              error={errors.fx_rate}
              placeholder="17.2345"
            />
            <TextField
              label="Fecha de la tasa"
              type="date"
              value={form.fx_date}
              onChange={(e) => set("fx_date", e.target.value)}
              error={errors.fx_date}
            />
            <TextField
              label="Fuente de la tasa"
              className="sm:col-span-2"
              value={form.fx_source}
              onChange={(e) => set("fx_source", e.target.value)}
              error={errors.fx_source}
              placeholder="Banco, app, Banxico…"
            />
          </>
        ) : (
          <>
            <MoneyField
              label="Monto"
              value={form.amount}
              onChange={(e) => set("amount", e.target.value)}
              error={errors.amount}
            />
            <TextField
              label="Moneda"
              value={form.currency}
              maxLength={3}
              hint={
                form.currency !== base
                  ? "Sin conversión no se suma a tus totales en la moneda base."
                  : undefined
              }
              onChange={(e) => set("currency", e.target.value.toUpperCase())}
              error={errors.currency}
            />
          </>
        )}

        {(k === "expense" || k === "income" || k === "loan_received") && (
          <CategorySelect
            categories={categories.data ?? []}
            kind={k === "expense" ? "expense" : "income"}
            value={form.category_id}
            onChange={(v) => set("category_id", v)}
            error={errors.category_id}
            emptyLabel={k === "loan_received" ? "Préstamo (por defecto)" : "Elige una categoría"}
          />
        )}

        {k === "expense" && (
          <CheckboxField
            className="sm:col-span-2"
            label="Lo pagó otra persona (préstamo familiar)"
            hint="Cuenta como consumo una sola vez; devolverlo después no es otro gasto."
            checked={form.financed}
            onChange={(e) => {
              set("financed", e.target.checked);
              if (e.target.checked) set("account_id", "");
            }}
          />
        )}
        {k === "expense" && form.financed && (
          <SimpleSelect
            label="¿Quién lo pagó?"
            value={form.financed_by_person_id}
            onChange={(v) => set("financed_by_person_id", v)}
            options={(people.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
            emptyLabel="Elige a la persona"
            error={errors.financed_by_person_id}
          />
        )}

        {!(k === "expense" && form.financed) && (
          <AccountSelect
            label={
              k === "expense"
                ? "Pagado con (cuenta o tarjeta)"
                : k === "income" || k === "loan_received"
                  ? "Entró a"
                  : k === "adjustment"
                    ? "Cuenta a ajustar"
                    : "Sale de"
            }
            value={form.account_id}
            onChange={(v) => set("account_id", v)}
            accounts={accs}
            filter={k === "income" || k === "transfer" || k === "card_payment" ? cashOnly : undefined}
            error={errors.account_id}
            emptyLabel={
              k === "expense" || k === "income" || k === "loan_received" ? "Sin especificar" : "Elige"
            }
            hint={
              k === "expense" && isCard(form.account_id)
                ? "Compra con tarjeta: consumo ahora, el pago de la tarjeta después no se suma otra vez."
                : undefined
            }
          />
        )}
        {(k === "transfer" || k === "card_payment") && (
          <AccountSelect
            label={k === "transfer" ? "Cuenta destino" : "Tarjeta que recibe el pago"}
            value={form.counter_account_id}
            onChange={(v) => set("counter_account_id", v)}
            accounts={accs}
            filter={k === "transfer" ? cashOnly : cardsOnly}
            error={errors.counter_account_id}
            emptyLabel="Elige"
          />
        )}
        {k === "debt_payment" && (
          <SimpleSelect
            label="Deuda que abonas"
            value={form.debt_id}
            onChange={(v) => set("debt_id", v)}
            options={(debts.data ?? [])
              .filter((d) => d.debt.status === "open" || d.debt.id === form.debt_id)
              .map((d) => ({ id: d.debt.id, name: d.debt.name }))}
            emptyLabel="Elige la deuda"
            error={errors.debt_id}
          />
        )}
        {k === "adjustment" && (
          <SelectField
            label="Sentido del ajuste"
            value={form.adjustment_sign}
            onChange={(e) => set("adjustment_sign", e.target.value as "1" | "-1")}
          >
            <option value="1">Sumar al saldo</option>
            <option value="-1">Restar del saldo</option>
          </SelectField>
        )}
        {k === "loan_received" && (
          <CheckboxField
            className="sm:col-span-2"
            label="Registrar también la deuda"
            hint="El préstamo es financiamiento, no ingreso ganado."
            checked={form.track_as_debt}
            onChange={(e) => set("track_as_debt", e.target.checked)}
          />
        )}

        <SimpleSelect
          label="Persona relacionada"
          value={form.person_id}
          onChange={(v) => set("person_id", v)}
          options={(people.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
          emptyLabel="Ninguna"
        />
        <SimpleSelect
          label="Proyecto"
          value={form.project_id}
          onChange={(v) => {
            set("project_id", v);
            set("budget_item_id", "");
          }}
          options={(projects.data ?? []).map((p) => ({ id: p.id, name: p.name }))}
          emptyLabel="Ninguno"
        />
        {k === "expense" && form.project_id && (
          <SimpleSelect
            label="Rubro del presupuesto"
            value={form.budget_item_id}
            onChange={(v) => set("budget_item_id", v)}
            options={rubros.filter((r) => r.id).map((r) => ({ id: r.id as string, name: r.name }))}
            emptyLabel={
              rubros.length ? "Automático por palabras clave" : "El proyecto no tiene rubros este mes"
            }
            hint="Una asignación explícita prevalece sobre las palabras clave."
            error={errors.budget_item_id}
          />
        )}
        <TextArea
          label="Notas"
          className="sm:col-span-2"
          rows={2}
          value={form.notes_md}
          onChange={(e) => set("notes_md", e.target.value)}
        />
        <button type="submit" hidden aria-hidden tabIndex={-1} />
      </form>
      {versioned.dialog}
    </Dialog>
  );
}
