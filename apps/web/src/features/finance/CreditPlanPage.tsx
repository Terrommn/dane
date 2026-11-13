// Plan de crédito (meta, pasos, reglas, tarjetas excluidas, % máximo de uso) y tarjeta sugerida explicable. El 30% es
// una regla personal inicial del vault, no una garantía financiera. No se ejecutan pagos ni hay conexión con bancos.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, TextField } from "../../components/ui/Field";
import { ErrorState, LoadingState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { FinanceNav, Money, MoneyField } from "./components";
import {
  type CardSuggestion,
  type CreditPlan,
  useAccounts,
  useCardSuggestion,
  useCreditPlan,
  usePlanMutation,
} from "./financeApi";
import { parseMoney } from "./money";

interface Step {
  text: string;
  done: boolean;
}
interface Rule {
  text: string;
}

export function CreditPlanPage() {
  const q = useCreditPlan();
  return (
    <div>
      <PageHeader
        title="Plan de crédito"
        subtitle="Reglas para usar tus tarjetas y la tarjeta sugerida del día"
      />
      <FinanceNav />
      <div className="grid gap-4 lg:grid-cols-2">
        <SuggestionCard />
        {q.isPending ? (
          <LoadingState rows={4} />
        ) : q.isError ? (
          <ErrorState error={q.error} onRetry={() => q.refetch()} />
        ) : (
          <PlanEditor key={q.data.version} plan={q.data} refetch={() => q.refetch()} />
        )}
      </div>
    </div>
  );
}

export function SuggestionCard({ compact }: { compact?: boolean }) {
  const { me } = useAuth();
  const [date, setDate] = useState(todayIso(me?.settings.timezone));
  const [amountText, setAmountText] = useState("");
  const parsed = amountText.trim() ? parseMoney(amountText) : null;
  const amount = parsed?.ok ? parsed.cents : null;
  const q = useCardSuggestion(date, amount);
  return (
    <Card title="Tarjeta sugerida">
      <div className="grid gap-3 sm:grid-cols-2">
        {!compact && (
          <TextField label="Fecha" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        )}
        <MoneyField
          label="¿Cuánto planeas gastar?"
          value={amountText}
          onChange={(e) => setAmountText(e.target.value)}
          error={parsed && !parsed.ok ? parsed.error : undefined}
          hint="Sin monto la sugerencia es provisional."
        />
      </div>
      <div className="mt-3">
        <QueryState query={q}>{(s) => <SuggestionResult s={s} />}</QueryState>
      </div>
    </Card>
  );
}

export function SuggestionResult({ s }: { s: CardSuggestion }) {
  return (
    <div aria-live="polite">
      <div className="rounded-xl border border-accent bg-surface-2 p-3">
        <p className="text-sm text-muted">Usa hoy</p>
        <p className="text-xl font-semibold">
          {s.card_name ?? s.debit_account_name ?? "Ninguna tarjeta conviene; usa débito o efectivo"}
          {s.provisional && (
            <>
              {" "}
              <Badge tone="warn">Provisional</Badge>
            </>
          )}
        </p>
        <p className="mt-1 text-sm">{s.explanation}</p>
        <p className="mt-1 text-xs text-muted">
          Uso máximo del plan: {s.max_usage_pct}%. Regla personal, no garantía financiera.
        </p>
      </div>
      {s.cards.length > 0 && (
        <ul className="mt-3 space-y-2">
          {s.cards.map((c) => (
            <li key={c.account_id} className="rounded-lg border border-line p-2 text-sm">
              <p className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-medium">{c.name}</span>
                {c.eligible ? <Badge tone="ok">Candidata</Badge> : <Badge>Excluida</Badge>}
              </p>
              <p className="text-muted">
                {c.available_cents != null && (
                  <>
                    Disponible <Money cents={c.available_cents} />
                    {" · "}
                  </>
                )}
                {c.days_to_cut != null &&
                  c.next_cut_on &&
                  `Corte ${formatShortDate(c.next_cut_on)} (en ${c.days_to_cut} días)`}
                {c.usage_after_pct != null && ` · uso tras el gasto ${c.usage_after_pct}%`}
              </p>
              {c.reasons.length > 0 && (
                <ul className="mt-1 list-disc pl-5">
                  {c.reasons.map((r) => (
                    <li key={r}>{r}</li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function asSteps(v: unknown[]): Step[] {
  return v.map((x) => {
    const o = x as { text?: unknown; done?: unknown };
    return { text: String(o.text ?? ""), done: o.done === true };
  });
}
function asRules(v: unknown[]): Rule[] {
  return v.map((x) => ({ text: String((x as { text?: unknown }).text ?? "") }));
}

function PlanEditor({ plan, refetch }: { plan: CreditPlan; refetch: () => void }) {
  const accounts = useAccounts();
  const save = usePlanMutation();
  const toast = useToast();
  const [goal, setGoal] = useState(plan.goal_text);
  const [target, setTarget] = useState(plan.target_date ?? "");
  const [steps, setSteps] = useState<Step[]>(asSteps(plan.steps));
  const [rules, setRules] = useState<Rule[]>(asRules(plan.rules));
  const [maxPct, setMaxPct] = useState(String(plan.max_usage_pct));
  const [excluded, setExcluded] = useState<string[]>(plan.excluded_account_ids);
  const [forbid, setForbid] = useState(plan.forbid_cards_with_debt);
  const [error, setError] = useState<string | null>(null);
  const [conflictVersion, setConflictVersion] = useState<number | null>(null);

  const cards = (accounts.data ?? []).filter((a) => a.account.kind === "credit_card");

  const submit = async (version = plan.version) => {
    const pct = Number(maxPct);
    if (!Number.isInteger(pct) || pct < 1 || pct > 100)
      return setError("El uso máximo debe ser un entero de 1 a 100");
    setError(null);
    const body: Schemas["PlanBody"] = {
      version,
      goal_text: goal,
      target_date: target || null,
      steps: steps.filter((s) => s.text.trim()) as unknown as Schemas["PlanBody"]["steps"],
      rules: rules.filter((r) => r.text.trim()) as unknown as Schemas["PlanBody"]["rules"],
      max_usage_pct: pct,
      excluded_account_ids: excluded,
      forbid_cards_with_debt: forbid,
    };
    try {
      await save.mutateAsync(body);
      toast.show("Plan de crédito guardado", { tone: "ok" });
      setConflictVersion(null);
    } catch (e) {
      if (e instanceof ApiError && e.isConflict)
        setConflictVersion((e.current as { version?: number } | null)?.version ?? version + 1);
      else setError(errorMessage(e));
    }
  };

  return (
    <Card title="Plan">
      {conflictVersion !== null && (
        <div role="alert" className="mb-3 rounded-lg border border-danger p-3 text-sm">
          <p className="font-medium">El plan cambió en otro dispositivo; tus cambios no se han guardado.</p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={refetch}>
              Ver la versión del servidor
            </Button>
            <Button size="sm" onClick={() => submit(conflictVersion)}>
              Guardar la mía sobre ella
            </Button>
          </div>
        </div>
      )}
      {error && (
        <p role="alert" className="mb-3 text-sm text-danger">
          {error}
        </p>
      )}
      <div className="space-y-3">
        <TextField
          label="Meta"
          value={goal}
          onChange={(e) => setGoal(e.target.value)}
          placeholder="Qué quieres lograr con tu crédito"
        />
        <TextField
          label="Fecha objetivo"
          type="date"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
        />
        <TextField
          label="Uso máximo por tarjeta (%)"
          inputMode="numeric"
          value={maxPct}
          onChange={(e) => setMaxPct(e.target.value)}
          hint="30% es la regla inicial del vault; ajústala si quieres."
        />
        <CheckboxField
          label="No sugerir tarjetas con deuda pendiente"
          checked={forbid}
          onChange={(e) => setForbid(e.target.checked)}
        />
        <fieldset>
          <legend className="mb-1 text-sm font-medium">Tarjetas excluidas por el plan</legend>
          {cards.length === 0 ? (
            <p className="text-sm text-muted">Sin tarjetas registradas.</p>
          ) : (
            <div className="space-y-2">
              {cards.map((c) => (
                <CheckboxField
                  key={c.account.id}
                  label={c.account.name}
                  checked={excluded.includes(c.account.id)}
                  onChange={(e) =>
                    setExcluded((xs) =>
                      e.target.checked ? [...xs, c.account.id] : xs.filter((x) => x !== c.account.id),
                    )
                  }
                />
              ))}
            </div>
          )}
        </fieldset>
        <fieldset>
          <legend className="mb-1 text-sm font-medium">Pasos</legend>
          <ul className="space-y-2">
            {steps.map((s, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: lista editable sin ID propio
              <li key={i} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  aria-label={`Paso ${i + 1} hecho`}
                  checked={s.done}
                  onChange={(e) =>
                    setSteps((xs) => xs.map((x, j) => (j === i ? { ...x, done: e.target.checked } : x)))
                  }
                  className="h-5 w-5 accent-[var(--c-accent)]"
                />
                <input
                  aria-label={`Paso ${i + 1}`}
                  value={s.text}
                  onChange={(e) =>
                    setSteps((xs) => xs.map((x, j) => (j === i ? { ...x, text: e.target.value } : x)))
                  }
                  className="min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 py-2"
                />
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`Quitar paso ${i + 1}`}
                  onClick={() => setSteps((xs) => xs.filter((_, j) => j !== i))}
                >
                  ✕
                </Button>
              </li>
            ))}
          </ul>
          <Button
            size="sm"
            variant="secondary"
            className="mt-2"
            onClick={() => setSteps((xs) => [...xs, { text: "", done: false }])}
          >
            + Paso
          </Button>
        </fieldset>
        <fieldset>
          <legend className="mb-1 text-sm font-medium">Reglas</legend>
          <ul className="space-y-2">
            {rules.map((r, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: lista editable sin ID propio
              <li key={i} className="flex items-center gap-2">
                <input
                  aria-label={`Regla ${i + 1}`}
                  value={r.text}
                  onChange={(e) =>
                    setRules((xs) => xs.map((x, j) => (j === i ? { text: e.target.value } : x)))
                  }
                  className="min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 py-2"
                />
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`Quitar regla ${i + 1}`}
                  onClick={() => setRules((xs) => xs.filter((_, j) => j !== i))}
                >
                  ✕
                </Button>
              </li>
            ))}
          </ul>
          <Button
            size="sm"
            variant="secondary"
            className="mt-2"
            onClick={() => setRules((xs) => [...xs, { text: "" }])}
          >
            + Regla
          </Button>
        </fieldset>
        <Button onClick={() => submit()} loading={save.isPending}>
          Guardar plan
        </Button>
      </div>
    </Card>
  );
}
