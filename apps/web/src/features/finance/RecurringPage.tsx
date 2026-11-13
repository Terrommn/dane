// Suscripciones e ingresos fijos. Una ocurrencia por regla y mes (idempotente en el servidor); las generadas quedan
// pendientes de conciliación hasta que las confirmas; las futuras se muestran como proyectadas. Cambiar o cancelar una
// regla no altera ocurrencias ya creadas.
import { useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { ConfirmOccurrenceDialog, type OccurrenceRef } from "./ConfirmOccurrenceDialog";
import {
  AccountSelect,
  CategorySelect,
  currentMonth,
  FinanceNav,
  Money,
  MoneyField,
  MonthPicker,
  SimpleSelect,
  useVersionedSave,
} from "./components";
import {
  type FinanceRuleBody,
  type RecurringRule,
  ruleBody,
  useAccounts,
  useAreaOptions,
  useCategories,
  useOccurrences,
  useProjectOptions,
  useRecurring,
  useRecurringMutations,
} from "./financeApi";
import {
  dayRuleLabel,
  monthLabel,
  OCCURRENCE_STATE_LABEL,
  RULE_KIND_LABEL,
  RULE_STATUS_LABEL,
} from "./labels";
import { centsToInput, parseMoney } from "./money";

export function RecurringPage() {
  const [sp, setSp] = useSearchParams();
  const month = sp.get("month") ?? currentMonth();
  const rules = useRecurring();
  const occ = useOccurrences(sp.get("month") ?? undefined);
  const accounts = useAccounts(true);
  const { materialize } = useRecurringMutations();
  const toast = useToast();
  const [editing, setEditing] = useState<RecurringRule | "subscription" | "fixed_income" | null>(null);
  const [confirming, setConfirming] = useState<OccurrenceRef | null>(null);
  const accName = (id?: string | null) => accounts.data?.find((a) => a.account.id === id)?.account.name;

  const byKind = (k: string) => (rules.data ?? []).filter((r) => r.kind === k);
  const occStates = (occ.data ?? []).reduce<Record<string, number>>((acc, o) => {
    acc[o.state] = (acc[o.state] ?? 0) + o.amount_cents * (o.kind === "fixed_income" ? 1 : -1);
    return acc;
  }, {});

  return (
    <div>
      <PageHeader
        title="Recurrentes"
        subtitle="Suscripciones e ingresos fijos"
        actions={
          <>
            <Button variant="secondary" onClick={() => setEditing("fixed_income")}>
              + Ingreso fijo
            </Button>
            <Button onClick={() => setEditing("subscription")}>+ Suscripción</Button>
          </>
        }
      />
      <FinanceNav />

      <Card
        title={`Ocurrencias de ${monthLabel(month)}`}
        className="mb-4"
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <MonthPicker
              value={month}
              onChange={(m) => setSp(m === currentMonth() ? {} : { month: m }, { replace: true })}
            />
            <Button
              size="sm"
              variant="secondary"
              loading={materialize.isPending}
              onClick={async () => {
                try {
                  const r = await materialize.mutateAsync();
                  toast.show(
                    r.created
                      ? `${r.created} ocurrencia(s) registradas como pendientes`
                      : "No había ocurrencias vencidas por registrar",
                    { tone: "ok" },
                  );
                } catch (e) {
                  toast.show(errorMessage(e), { tone: "error" });
                }
              }}
            >
              Registrar vencidas ahora
            </Button>
          </div>
        }
      >
        <p className="mb-3 text-sm text-muted">
          Registrada = generada automáticamente el día programado, pendiente de conciliar (no afecta saldos).
          Confirmada = viste el cargo o depósito. Proyectada = aún no llega su día.
        </p>
        <QueryState
          query={occ}
          isEmpty={(d) => d.length === 0}
          empty={<EmptyState title="Sin ocurrencias este mes" />}
        >
          {(list) => (
            <>
              <ul className="divide-y divide-line">
                {list.map((o) => (
                  <li
                    key={`${o.rule_id}-${o.date}`}
                    className="flex flex-wrap items-center justify-between gap-2 py-2"
                  >
                    <span className="min-w-0">
                      <span className="block font-medium">{o.rule_name}</span>
                      <span className="block text-sm text-muted">
                        {RULE_KIND_LABEL[o.kind] ?? o.kind} · {formatShortDate(o.date)}
                      </span>
                    </span>
                    <span className="flex flex-wrap items-center gap-2">
                      <Badge tone={o.state === "confirmed" ? "ok" : o.state === "pending" ? "warn" : "muted"}>
                        {OCCURRENCE_STATE_LABEL[o.state] ?? o.state}
                      </Badge>
                      <Money cents={o.amount_cents} />
                      {o.state === "pending" && o.transaction_id && (
                        <Button
                          size="sm"
                          onClick={() =>
                            setConfirming({
                              transaction_id: o.transaction_id ?? "",
                              transaction_version: o.transaction_version ?? 1,
                              amount_cents: o.amount_cents,
                              date: o.date,
                              rule_name: o.rule_name,
                              account_id: rules.data?.find((r) => r.id === o.rule_id)?.account_id ?? null,
                            })
                          }
                        >
                          Confirmar
                        </Button>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
              <dl className="mt-3 grid gap-x-6 text-sm sm:grid-cols-3">
                {(["confirmed", "pending", "projected"] as const).map((s) => (
                  <div key={s} className="flex justify-between gap-2">
                    <dt className="text-muted">{OCCURRENCE_STATE_LABEL[s]?.split(" ·")[0]}</dt>
                    <dd>
                      <Money cents={occStates[s] ?? 0} signed />
                    </dd>
                  </div>
                ))}
              </dl>
            </>
          )}
        </QueryState>
      </Card>

      <QueryState query={rules}>
        {() => (
          <div className="grid gap-4 lg:grid-cols-2">
            {(["subscription", "fixed_income"] as const).map((k) => (
              <Card key={k} title={k === "subscription" ? "Suscripciones" : "Ingresos fijos"}>
                {byKind(k).length === 0 ? (
                  <EmptyState title={k === "subscription" ? "Sin suscripciones" : "Sin ingresos fijos"} />
                ) : (
                  <ul className="divide-y divide-line">
                    {byKind(k).map((r) => (
                      <li key={r.id} className="py-2">
                        <button
                          type="button"
                          className="flex w-full items-start justify-between gap-3 text-left"
                          onClick={() => setEditing(r)}
                        >
                          <span className="min-w-0">
                            <span className="block font-medium">
                              {r.name}{" "}
                              {r.status !== "active" && (
                                <Badge>{RULE_STATUS_LABEL[r.status] ?? r.status}</Badge>
                              )}
                            </span>
                            <span className="block text-sm text-muted">
                              {dayRuleLabel(r.day_kind, r.day)}
                              {accName(r.account_id) ? ` · ${accName(r.account_id)}` : ""}
                            </span>
                          </span>
                          <Money cents={r.amount_cents} currency={r.currency} />
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
            ))}
          </div>
        )}
      </QueryState>

      {editing && (
        <RuleDialog
          rule={typeof editing === "string" ? null : editing}
          kind={typeof editing === "string" ? editing : (editing.kind as FinanceRuleBody["kind"])}
          onClose={() => setEditing(null)}
        />
      )}
      {confirming && <ConfirmOccurrenceDialog occurrence={confirming} onClose={() => setConfirming(null)} />}
    </div>
  );
}

const RULE_LABELS: Record<string, string> = {
  name: "Nombre",
  amount_cents: "Monto (centavos)",
  day_kind: "Regla de día",
  day: "Día",
  status: "Estado",
  ends_on: "Fin",
  notes_md: "Notas",
};

function RuleDialog({
  rule,
  kind,
  onClose,
}: {
  rule: RecurringRule | null;
  kind: FinanceRuleBody["kind"];
  onClose: () => void;
}) {
  const { me } = useAuth();
  const accounts = useAccounts();
  const categories = useCategories();
  const areas = useAreaOptions();
  const projects = useProjectOptions();
  const { create, update } = useRecurringMutations();
  const toast = useToast();
  const [f, setF] = useState({
    name: rule?.name ?? "",
    amount: rule ? centsToInput(rule.amount_cents) : "",
    day_kind: (rule?.day_kind ?? "day") as FinanceRuleBody["day_kind"],
    day: rule?.day?.toString() ?? "1",
    category_id: rule?.category_id ?? "",
    account_id: rule?.account_id ?? "",
    area_id: rule?.area_id ?? "",
    project_id: rule?.project_id ?? "",
    status: (rule?.status ?? "active") as FinanceRuleBody["status"],
    starts_on: rule?.starts_on ?? todayIso(me?.settings.timezone),
    ends_on: rule?.ends_on ?? "",
    notes_md: rule?.notes_md ?? "",
  });
  const [err, setErr] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const set = (k: keyof typeof f, v: string) => setF((x) => ({ ...x, [k]: v }));

  const versioned = useVersionedSave<FinanceRuleBody>({
    save: (body) => update.mutateAsync({ id: rule?.id ?? "", body }),
    labels: RULE_LABELS,
    toBody: (cur) => ruleBody(cur as RecurringRule),
    onSaved: () => {
      toast.show("Regla actualizada; las ocurrencias ya creadas no cambian", { tone: "ok" });
      onClose();
    },
    onError: (e) => setFormError(errorMessage(e)),
  });

  const submit = async () => {
    const e: Record<string, string> = {};
    if (!f.name.trim()) e.name = "Nombre del servicio o fuente";
    const p = parseMoney(f.amount);
    if (!p.ok) e.amount = p.error;
    const day = Number(f.day);
    if (f.day_kind === "day" && (!Number.isInteger(day) || day < 1 || day > 31)) e.day = "Día del 1 al 31";
    if (f.ends_on && f.ends_on < f.starts_on) e.ends_on = "Debe ser posterior al inicio";
    setErr(e);
    if (Object.keys(e).length || !p.ok) return;
    const body: FinanceRuleBody = {
      kind,
      name: f.name.trim(),
      amount_cents: p.cents,
      currency: rule?.currency ?? me?.settings.currency ?? "MXN",
      day_kind: f.day_kind,
      day: f.day_kind === "day" ? day : null,
      category_id: f.category_id || null,
      account_id: f.account_id || null,
      area_id: f.area_id || null,
      project_id: f.project_id || null,
      status: f.status,
      starts_on: f.starts_on || null,
      ends_on: f.ends_on || null,
      notes_md: f.notes_md,
    };
    try {
      if (rule) await versioned.run({ ...body, version: rule.version });
      else {
        await create.mutateAsync(body);
        toast.show(`${RULE_KIND_LABEL[kind]} creada`, { tone: "ok" });
        onClose();
      }
    } catch (x) {
      if (x instanceof ApiError && x.field)
        setErr({ [x.field === "amount_cents" ? "amount" : x.field]: x.message });
      else setFormError(errorMessage(x));
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={rule ? `Editar ${rule.name}` : `Nueva ${(RULE_KIND_LABEL[kind] ?? kind).toLowerCase()}`}
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
      {rule && (
        <p className="mb-3 text-sm text-muted">
          Los cambios aplican a ocurrencias futuras; las ya registradas conservan su monto y fecha.
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          label={kind === "subscription" ? "Servicio" : "Fuente"}
          value={f.name}
          onChange={(e) => set("name", e.target.value)}
          error={err.name}
        />
        <MoneyField
          label="Monto"
          value={f.amount}
          onChange={(e) => set("amount", e.target.value)}
          error={err.amount}
        />
        <SelectField
          label="Día de cobro/ingreso"
          value={f.day_kind}
          onChange={(e) => set("day_kind", e.target.value)}
        >
          <option value="day">Día del mes</option>
          <option value="last">Último día del mes</option>
          <option value="penultimate">Penúltimo día del mes</option>
        </SelectField>
        {f.day_kind === "day" && (
          <TextField
            label="Día"
            inputMode="numeric"
            value={f.day}
            onChange={(e) => set("day", e.target.value)}
            error={err.day}
            hint="Si el mes es más corto se usa el último día."
          />
        )}
        <CategorySelect
          categories={categories.data ?? []}
          kind={kind === "subscription" ? "expense" : "income"}
          value={f.category_id}
          onChange={(v) => set("category_id", v)}
          emptyLabel="Sin categoría"
          error={err.category_id}
        />
        <AccountSelect
          label={kind === "subscription" ? "Se cobra a" : "Entra a"}
          value={f.account_id}
          onChange={(v) => set("account_id", v)}
          accounts={accounts.data ?? []}
          filter={kind === "fixed_income" ? (a) => a.account.kind !== "credit_card" : undefined}
          error={err.account_id}
        />
        <SimpleSelect
          label="Área"
          value={f.area_id}
          onChange={(v) => set("area_id", v)}
          options={areas.data ?? []}
          emptyLabel="Sin área"
        />
        <SimpleSelect
          label="Proyecto"
          value={f.project_id}
          onChange={(v) => set("project_id", v)}
          options={projects.data ?? []}
          emptyLabel="Sin proyecto"
        />
        <SelectField label="Estado" value={f.status} onChange={(e) => set("status", e.target.value)}>
          {Object.entries(RULE_STATUS_LABEL).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Desde"
          type="date"
          value={f.starts_on}
          onChange={(e) => set("starts_on", e.target.value)}
          hint="No se generan ocurrencias anteriores."
        />
        <TextField
          label="Hasta (opcional)"
          type="date"
          value={f.ends_on}
          onChange={(e) => set("ends_on", e.target.value)}
          error={err.ends_on}
        />
        <TextArea
          label="Notas"
          className="sm:col-span-2"
          rows={2}
          value={f.notes_md}
          onChange={(e) => set("notes_md", e.target.value)}
        />
      </div>
      {versioned.dialog}
    </Dialog>
  );
}
