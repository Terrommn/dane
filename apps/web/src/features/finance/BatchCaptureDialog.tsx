// Captura múltiple de gastos/ingresos (como `/gasto ayer 250 gasolina, hoy 180 cena`). Cada fila tiene su propia
// `client_key`: dos compras idénticas se conservan como dos movimientos y reenviar el lote no duplica nada.
// El lote es atómico en el servidor: si una fila es inválida no se guarda ninguna (422 con el índice).
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { SelectField, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { MoneyField } from "./components";
import { newClientKey, useAccounts, useCategories, useTxMutations } from "./financeApi";
import { buildTxBody, emptyTxForm, type TxBody } from "./txForm";

export interface BatchRow {
  key: string;
  kind: "expense" | "income";
  occurred_on: string;
  concept: string;
  amount: string;
  category_id: string;
  account_id: string;
}

export function newRow(date: string, kind: BatchRow["kind"] = "expense"): BatchRow {
  return {
    key: newClientKey(),
    kind,
    occurred_on: date,
    concept: "",
    amount: "",
    category_id: "",
    account_id: "",
  };
}

/** Valida y arma los cuerpos del lote; devuelve errores por fila (índice → mensaje). Filas vacías se ignoran. */
export function buildBatch(
  rows: BatchRow[],
  ctx: Parameters<typeof buildTxBody>[1],
): { bodies: TxBody[]; errors: Record<number, string> } {
  const bodies: TxBody[] = [];
  const errors: Record<number, string> = {};
  rows.forEach((r, i) => {
    if (!r.concept.trim() && !r.amount.trim()) return;
    const f = {
      ...emptyTxForm(r.occurred_on, ctx.baseCurrency, r.kind),
      concept: r.concept,
      amount: r.amount,
      category_id: r.category_id,
      account_id: r.account_id,
    };
    const b = buildTxBody(f, ctx);
    if (!b.ok) errors[i] = Object.values(b.errors).filter(Boolean).join(" · ");
    else bodies.push({ ...b.body, client_key: r.key });
  });
  return { bodies, errors };
}

export function BatchCaptureDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null;
  return <BatchBody onClose={onClose} />;
}

function BatchBody({ onClose }: { onClose: () => void }) {
  const { me } = useAuth();
  const base = me?.settings.currency ?? "MXN";
  const today = todayIso(me?.settings.timezone);
  const accounts = useAccounts();
  const categories = useCategories();
  const { batch } = useTxMutations();
  const toast = useToast();
  const [rows, setRows] = useState<BatchRow[]>(() => [newRow(today), newRow(today), newRow(today)]);
  const [errors, setErrors] = useState<Record<number, string>>({});
  const [formError, setFormError] = useState<string | null>(null);

  const ctx = {
    accounts: (accounts.data ?? []).map((a) => ({
      id: a.account.id,
      kind: a.account.kind,
      currency: a.account.currency,
    })),
    categories: (categories.data ?? []).map((c) => ({ id: c.id, kind: c.kind })),
    baseCurrency: base,
  };
  const update = (i: number, patch: Partial<BatchRow>) => {
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));
    setErrors((e) => ({ ...e, [i]: "" }));
  };

  const submit = async () => {
    setFormError(null);
    const { bodies, errors: errs } = buildBatch(rows, ctx);
    if (Object.keys(errs).length) return setErrors(errs);
    if (!bodies.length) return setFormError("Agrega al menos un movimiento");
    try {
      const res = await batch.mutateAsync(bodies);
      const created = res.results.filter((r) => r.created).length;
      const dup = res.results.length - created;
      toast.show(
        `${created} movimiento${created === 1 ? "" : "s"} registrado${created === 1 ? "" : "s"}${dup ? ` · ${dup} ya estaban (reintento)` : ""}`,
        { tone: "ok" },
      );
      for (const n of res.results.map((r) => r.notice).filter(Boolean)) toast.show(n, { sticky: true });
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.status === 422 && e.field) {
        const m = /items\[(\d+)\]/.exec(e.field);
        const filled = rows.map((r, i) => ({ r, i })).filter(({ r }) => r.concept.trim() || r.amount.trim());
        const idx = m ? filled[Number(m[1])]?.i : undefined;
        if (idx !== undefined) return setErrors({ [idx]: e.message });
      }
      setFormError(`${errorMessage(e)}. No se guardó ninguno.`);
    }
  };

  const cats = (kind: string) => (categories.data ?? []).filter((c) => c.kind === kind && !c.archived_at);

  return (
    <Dialog
      open
      size="lg"
      onClose={onClose}
      title="Captura múltiple"
      footer={
        <>
          <Button variant="secondary" onClick={() => setRows((r) => [...r, newRow(today)])}>
            + Fila
          </Button>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={batch.isPending}>
            Registrar todo
          </Button>
        </>
      }
    >
      <p className="mb-3 text-sm text-muted">
        Las filas vacías se ignoran. Dos compras iguales se guardan como dos movimientos distintos.
      </p>
      {formError && (
        <p role="alert" className="mb-3 rounded-lg border border-danger p-2 text-sm text-danger">
          {formError}
        </p>
      )}
      <ol className="space-y-3">
        {rows.map((r, i) => (
          <li key={r.key} className="rounded-xl border border-line p-3">
            <fieldset className="grid gap-2 sm:grid-cols-6">
              <legend className="mb-1 text-sm font-medium">Movimiento {i + 1}</legend>
              <SelectField
                label="Tipo"
                className="sm:col-span-2"
                value={r.kind}
                onChange={(e) => update(i, { kind: e.target.value as BatchRow["kind"], category_id: "" })}
              >
                <option value="expense">Gasto</option>
                <option value="income">Ingreso</option>
              </SelectField>
              <TextField
                label="Fecha"
                type="date"
                className="sm:col-span-2"
                value={r.occurred_on}
                onChange={(e) => update(i, { occurred_on: e.target.value })}
              />
              <MoneyField
                label="Monto"
                className="sm:col-span-2"
                value={r.amount}
                onChange={(e) => update(i, { amount: e.target.value })}
              />
              <TextField
                label="Concepto"
                className="sm:col-span-2"
                value={r.concept}
                onChange={(e) => update(i, { concept: e.target.value })}
              />
              <SelectField
                label="Categoría"
                className="sm:col-span-2"
                value={r.category_id}
                onChange={(e) => update(i, { category_id: e.target.value })}
              >
                <option value="">Elige</option>
                {cats(r.kind).map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </SelectField>
              <SelectField
                label={r.kind === "expense" ? "Pago" : "Cuenta"}
                className="sm:col-span-2"
                value={r.account_id}
                onChange={(e) => update(i, { account_id: e.target.value })}
              >
                <option value="">Sin especificar</option>
                {(accounts.data ?? [])
                  .filter((a) => r.kind === "expense" || a.account.kind !== "credit_card")
                  .map((a) => (
                    <option key={a.account.id} value={a.account.id}>
                      {a.account.name}
                    </option>
                  ))}
              </SelectField>
            </fieldset>
            {errors[i] && (
              <p role="alert" className="mt-2 text-sm font-medium text-danger">
                {errors[i]}
              </p>
            )}
            {rows.length > 1 && (
              <Button
                variant="ghost"
                size="sm"
                className="mt-1"
                onClick={() => setRows((rs) => rs.filter((_, j) => j !== i))}
              >
                Quitar fila
              </Button>
            )}
          </li>
        ))}
      </ol>
    </Dialog>
  );
}
