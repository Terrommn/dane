// Confirmar (y opcionalmente corregir) una ocurrencia recurrente pendiente de conciliación. Actualiza la MISMA
// transacción: nunca crea una segunda. Hasta confirmarla no afecta saldos ni flujo confirmado.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { AccountSelect, MoneyField } from "./components";
import { useAccounts, useTxMutations } from "./financeApi";
import { centsToInput, formatCents, parseMoney } from "./money";

export interface OccurrenceRef {
  transaction_id: string;
  transaction_version: number;
  amount_cents: number;
  date: string;
  rule_name: string;
  account_id?: string | null;
}

export function ConfirmOccurrenceDialog({
  occurrence,
  onClose,
}: {
  occurrence: OccurrenceRef;
  onClose: () => void;
}) {
  const accounts = useAccounts();
  const { confirm } = useTxMutations();
  const toast = useToast();
  const [amount, setAmount] = useState(centsToInput(occurrence.amount_cents));
  const [date, setDate] = useState(occurrence.date);
  const [accountId, setAccountId] = useState(occurrence.account_id ?? "");
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const p = parseMoney(amount);
    if (!p.ok) return setError(p.error);
    try {
      await confirm.mutateAsync({
        id: occurrence.transaction_id,
        body: {
          version: occurrence.transaction_version,
          amount_cents: p.cents !== occurrence.amount_cents ? p.cents : null,
          occurred_on: date !== occurrence.date ? date : null,
          account_id: accountId && accountId !== occurrence.account_id ? accountId : null,
        },
      });
      toast.show(`"${occurrence.rule_name}" confirmado por ${formatCents(p.cents)}`, { tone: "ok" });
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.isConflict)
        setError("Otro dispositivo cambió esta ocurrencia; ciérralo y vuelve a abrirlo.");
      else setError(errorMessage(e));
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={`Confirmar ${occurrence.rule_name}`}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={confirm.isPending}>
            Confirmar
          </Button>
        </>
      }
    >
      <p className="mb-3 text-sm text-muted">
        La fecha programada no prueba el cobro: confirma solo si ya viste el cargo o el depósito. Si el monto
        o la fecha real fueron distintos, corrígelos aquí.
      </p>
      {error && (
        <p role="alert" className="mb-3 text-sm font-medium text-danger">
          {error}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <MoneyField label="Monto real" value={amount} onChange={(e) => setAmount(e.target.value)} />
        <TextField label="Fecha real" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        <AccountSelect
          label="Cuenta o tarjeta"
          value={accountId}
          onChange={setAccountId}
          accounts={accounts.data ?? []}
          emptyLabel="Sin cambio"
        />
      </div>
    </Dialog>
  );
}
