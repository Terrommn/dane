// Presupuesto mensual: snapshot propio de cada mes (al no existir se propone la copia del mes anterior), líneas por
// categoría de gasto y rubros por proyecto (la línea de su categoría es la suma de los rubros; las palabras clave
// solo sugieren el rubro, una asignación explícita en el gasto prevalece). Editar este mes no toca los anteriores.
import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextField } from "../../components/ui/Field";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { ApiError, errorMessage } from "../../lib/errors";
import { currentMonth, FinanceNav, Money, MoneyField, MonthPicker, Stat } from "./components";
import {
  type BudgetView,
  useBudget,
  useBudgetMutation,
  useCategories,
  useProjectOptions,
} from "./financeApi";
import { monthLabel, ROLE_LABEL } from "./labels";
import { centsToInput, parseMoney } from "./money";

interface ItemRow {
  key: string;
  id: string | null;
  project_id: string;
  category_id: string;
  name: string;
  amount: string;
  keywords: string;
}

/** Arma el cuerpo del PUT; devuelve errores por clave de línea/rubro. */
export function buildBudgetBody(
  version: number,
  lines: Record<string, string>,
  items: ItemRow[],
): { ok: true; body: Schemas["BudgetBody"] } | { ok: false; errors: Record<string, string> } {
  const errors: Record<string, string> = {};
  const outLines: Schemas["BudgetLine"][] = [];
  for (const [category_id, text] of Object.entries(lines)) {
    if (!text.trim()) continue;
    const p = parseMoney(text, { allowZero: true });
    if (!p.ok) errors[`line:${category_id}`] = p.error;
    else outLines.push({ category_id, amount_cents: p.cents });
  }
  const outItems: Schemas["BudgetItem"][] = [];
  const seen = new Set<string>();
  items.forEach((it, i) => {
    if (!it.name.trim() && !it.amount.trim()) return;
    if (!it.project_id) errors[`item:${it.key}`] = "Elige el proyecto";
    else if (!it.category_id) errors[`item:${it.key}`] = "Elige la categoría";
    else if (!it.name.trim()) errors[`item:${it.key}`] = "Nombre del rubro";
    const p = parseMoney(it.amount, { allowZero: true });
    if (!p.ok) errors[`item:${it.key}`] = p.error;
    const dup = `${it.project_id}:${it.name.trim().toLowerCase()}`;
    if (seen.has(dup)) errors[`item:${it.key}`] = "Rubro repetido en el proyecto";
    seen.add(dup);
    if (p.ok && it.project_id && it.category_id && it.name.trim())
      outItems.push({
        id: it.id,
        project_id: it.project_id,
        category_id: it.category_id,
        name: it.name.trim(),
        amount_cents: p.cents,
        keywords: it.keywords
          .split(",")
          .map((k) => k.trim())
          .filter(Boolean),
        position: i,
      });
  });
  if (Object.keys(errors).length) return { ok: false, errors };
  return { ok: true, body: { version, lines: outLines, items: outItems } };
}

let rowSeq = 0;
const itemRow = (i?: Schemas["BudgetItem"]): ItemRow => ({
  key: `r${++rowSeq}`,
  id: i?.id ?? null,
  project_id: i?.project_id ?? "",
  category_id: i?.category_id ?? "",
  name: i?.name ?? "",
  amount: i ? centsToInput(i.amount_cents) : "",
  keywords: (i?.keywords ?? []).join(", "),
});

export function BudgetPage() {
  const [sp, setSp] = useSearchParams();
  const month = sp.get("month") ?? currentMonth();
  const q = useBudget(month);
  return (
    <div>
      <PageHeader
        title="Presupuesto"
        subtitle={`Cuánto puedes gastar en ${monthLabel(month)}`}
        actions={
          <MonthPicker
            value={month}
            onChange={(m) => setSp(m === currentMonth() ? {} : { month: m }, { replace: true })}
          />
        }
      />
      <FinanceNav />
      {q.isPending ? (
        <LoadingState rows={6} />
      ) : q.isError ? (
        <ErrorState error={q.error} onRetry={() => q.refetch()} />
      ) : (
        <BudgetEditor
          key={`${month}-${q.data.version}-${q.data.persisted}`}
          month={month}
          view={q.data}
          refetch={() => q.refetch()}
        />
      )}
    </div>
  );
}

function BudgetEditor({ month, view, refetch }: { month: string; view: BudgetView; refetch: () => void }) {
  const categories = useCategories();
  const projects = useProjectOptions();
  const save = useBudgetMutation(month);
  const toast = useToast();
  const initialLines = useMemo(
    () =>
      Object.fromEntries(
        view.lines.filter((l) => !l.from_items).map((l) => [l.category_id, centsToInput(l.amount_cents)]),
      ),
    [view],
  );
  const [lines, setLines] = useState<Record<string, string>>(initialLines);
  const [items, setItems] = useState<ItemRow[]>(() => view.items.map((i) => itemRow(i)));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState<number | null>(null);
  const [dirty, setDirty] = useState(!view.persisted);

  useEffect(() => setDirty(!view.persisted), [view.persisted]);

  const expenseCats = (categories.data ?? []).filter((c) => c.kind === "expense" && !c.archived_at);
  const fromItems = new Map(view.lines.filter((l) => l.from_items).map((l) => [l.category_id, l]));
  const itemSums = useMemo(() => {
    const m = new Map<string, number>();
    for (const it of items) {
      const p = parseMoney(it.amount, { allowZero: true });
      if (p.ok && it.category_id) m.set(it.category_id, (m.get(it.category_id) ?? 0) + p.cents);
    }
    return m;
  }, [items]);

  const submit = async (version = view.version) => {
    const r = buildBudgetBody(version, lines, items);
    if (!r.ok) return setErrors(r.errors);
    setErrors({});
    try {
      await save.mutateAsync(r.body);
      toast.show(`Presupuesto de ${monthLabel(month)} guardado`, { tone: "ok" });
      setConflict(null);
      setDirty(false);
    } catch (e) {
      if (e instanceof ApiError && e.isConflict) {
        const cur = e.current as { version?: number } | null;
        setConflict(cur?.version ?? version + 1);
      } else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const setItem = (key: string, patch: Partial<ItemRow>) => {
    setItems((xs) => xs.map((x) => (x.key === key ? { ...x, ...patch } : x)));
    setDirty(true);
  };

  return (
    <div className="space-y-4">
      {!view.persisted && (
        <p role="note" className="rounded-xl border border-warn bg-surface p-3 text-sm">
          Este mes aún no tiene presupuesto propio. Se muestra la propuesta copiada de{" "}
          {view.copied_from ? monthLabel(view.copied_from) : "el mes anterior"}; guárdala (o ajústala) para
          fijarla. El primer gasto del mes también la fija.
        </p>
      )}
      {conflict !== null && (
        <div role="alert" className="rounded-xl border border-danger bg-surface p-3 text-sm">
          <p className="font-medium">El presupuesto cambió en otro dispositivo.</p>
          <p className="mt-1 text-muted">Tus cambios siguen aquí y no se han guardado.</p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={() => refetch()}>
              Ver la versión del servidor
            </Button>
            <Button size="sm" onClick={() => submit(conflict)}>
              Guardar la mía sobre ella
            </Button>
          </div>
        </div>
      )}

      <Card title="Resumen">
        <dl className="grid gap-x-6 sm:grid-cols-2">
          <Stat label="Tope de gasto" hint="Sin categorías de pago de deuda">
            <Money cents={view.spending_cap_cents} />
          </Stat>
          <Stat label="Total presupuestado" hint="Incluye pagos de deuda presupuestados">
            <Money cents={view.total_cents} />
          </Stat>
        </dl>
        <p className="mt-2 text-sm text-muted">Los totales se recalculan al guardar.</p>
      </Card>

      <Card title="Por categoría">
        <ul className="grid gap-3 sm:grid-cols-2">
          {expenseCats.map((c) => {
            const fi = fromItems.get(c.id);
            const sum = itemSums.get(c.id);
            return (
              <li key={c.id}>
                {fi || sum !== undefined ? (
                  <div className="rounded-lg border border-line p-3">
                    <p className="text-sm font-medium">{c.name}</p>
                    <p className="text-sm text-muted">Suma de rubros de proyecto (abajo)</p>
                    <p className="font-semibold">
                      <Money cents={sum ?? fi?.amount_cents ?? 0} />
                    </p>
                  </div>
                ) : (
                  <MoneyField
                    label={c.name}
                    hint={ROLE_LABEL[c.role] && c.role !== "consumption" ? ROLE_LABEL[c.role] : undefined}
                    value={lines[c.id] ?? ""}
                    error={errors[`line:${c.id}`]}
                    onChange={(e) => {
                      setLines((l) => ({ ...l, [c.id]: e.target.value }));
                      setDirty(true);
                    }}
                  />
                )}
              </li>
            );
          })}
        </ul>
      </Card>

      <Card
        title="Rubros por proyecto"
        actions={
          <Button size="sm" variant="secondary" onClick={() => setItems((x) => [...x, itemRow()])}>
            + Rubro
          </Button>
        }
      >
        <p className="mb-3 text-sm text-muted">
          Ej. Negocio: Servidores, WhatsApp, Ads. Las palabras clave sugieren el rubro de un gasto sin
          asignación explícita.
        </p>
        {items.length === 0 ? (
          <p className="text-sm text-muted">Sin rubros. Agrega uno si un proyecto tiene gastos propios.</p>
        ) : (
          <ul className="space-y-3">
            {items.map((it) => (
              <li key={it.key} className="rounded-xl border border-line p-3">
                <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
                  <SelectField
                    label="Proyecto"
                    value={it.project_id}
                    onChange={(e) => setItem(it.key, { project_id: e.target.value })}
                  >
                    <option value="">Elige</option>
                    {(projects.data ?? []).map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </SelectField>
                  <SelectField
                    label="Categoría"
                    value={it.category_id}
                    onChange={(e) => setItem(it.key, { category_id: e.target.value })}
                  >
                    <option value="">Elige</option>
                    {expenseCats.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </SelectField>
                  <TextField
                    label="Rubro"
                    value={it.name}
                    onChange={(e) => setItem(it.key, { name: e.target.value })}
                  />
                  <MoneyField
                    label="Monto"
                    value={it.amount}
                    onChange={(e) => setItem(it.key, { amount: e.target.value })}
                  />
                  <TextField
                    label="Palabras clave"
                    hint="Separadas por coma"
                    value={it.keywords}
                    onChange={(e) => setItem(it.key, { keywords: e.target.value })}
                  />
                </div>
                {errors[`item:${it.key}`] && (
                  <p role="alert" className="mt-1 text-sm text-danger">
                    {errors[`item:${it.key}`]}
                  </p>
                )}
                <div className="mt-2 flex flex-wrap gap-3">
                  {it.project_id && (
                    <Link className="text-sm" to={`/finanzas/proyectos/${it.project_id}?month=${month}`}>
                      Ver gastos del proyecto
                    </Link>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setItems((x) => x.filter((y) => y.key !== it.key));
                      setDirty(true);
                    }}
                  >
                    Quitar rubro
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <div className="sticky bottom-[calc(5rem+var(--safe-bottom))] z-10 flex flex-wrap items-center justify-end gap-2 rounded-xl border border-line bg-surface p-3 md:bottom-3">
        {dirty ? <Badge tone="warn">Cambios sin guardar</Badge> : <Badge tone="ok">Guardado</Badge>}
        <Button onClick={() => submit()} loading={save.isPending}>
          {view.persisted ? "Guardar presupuesto" : "Fijar presupuesto del mes"}
        </Button>
      </div>
    </div>
  );
}
