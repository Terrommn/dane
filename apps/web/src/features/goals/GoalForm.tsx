// Crear/editar objetivo mensual o semanal. El cierre (logrado/no logrado) es una acción aparte (CloseGoalDialog).
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { useAreas } from "../projects/projectsApi";
import { type Goal, type GoalBody, goalToBody, PROGRESS_TYPE, useGoalMutations, useGoals } from "./goalsApi";
import { formatMonth, formatWeek, isValidMonthKey, isValidWeekKey, shiftWeek } from "./periods";

export interface GoalFormValues {
  kind: "month" | "week";
  period_key: string;
  title: string;
  area_id: string;
  motivation: string;
  success_criteria: string;
  parent_goal_id: string;
  progress_type: string;
  unit: string;
  baseline: string;
  target: string;
  direction: string;
}

const DEC = /^-?\d+(\.\d{1,3})?$/;

/** Valida y arma el cuerpo. Devuelve el mensaje de error en español si algo no cuadra. */
export function valuesToBody(v: GoalFormValues): { ok: true; body: GoalBody } | { ok: false; error: string } {
  if (!v.title.trim()) return { ok: false, error: "Escribe el objetivo" };
  if (v.kind === "month" && !isValidMonthKey(v.period_key)) return { ok: false, error: "Mes inválido" };
  if (v.kind === "week" && !isValidWeekKey(v.period_key)) return { ok: false, error: "Semana inválida" };
  const measured = v.progress_type === "cumulative" || v.progress_type === "current_value";
  if (measured) {
    if (!v.unit.trim()) return { ok: false, error: "Indica la unidad (libros, sesiones, kg, $…)" };
    if (!DEC.test(v.target.trim())) return { ok: false, error: "Indica la meta numérica" };
    if (v.baseline.trim() && !DEC.test(v.baseline.trim())) return { ok: false, error: "Línea base inválida" };
  }
  if (v.progress_type === "current_value" && !v.direction)
    return { ok: false, error: "Indica si el valor debe subir o bajar" };
  return {
    ok: true,
    body: {
      kind: v.kind,
      period_key: v.period_key,
      title: v.title.trim(),
      area_id: v.area_id || null,
      motivation: v.motivation,
      success_criteria: v.success_criteria,
      parent_goal_id: v.kind === "week" && v.parent_goal_id ? v.parent_goal_id : null,
      progress_type: v.progress_type,
      unit: measured ? v.unit.trim() : v.unit.trim() || null,
      baseline: measured && v.baseline.trim() ? v.baseline.trim() : null,
      target: measured ? v.target.trim() : null,
      direction:
        v.progress_type === "current_value"
          ? v.direction
          : v.progress_type === "cumulative"
            ? "increase"
            : null,
    },
  };
}

function initial(
  goal: Goal | null,
  kind: "month" | "week",
  periodKey: string,
  parentId?: string,
): GoalFormValues {
  return {
    kind: (goal?.kind as "month" | "week") ?? kind,
    period_key: goal?.period_key ?? periodKey,
    title: goal?.title ?? "",
    area_id: goal?.area_id ?? "",
    motivation: goal?.motivation ?? "",
    success_criteria: goal?.success_criteria ?? "",
    parent_goal_id: goal?.parent_goal_id ?? parentId ?? "",
    progress_type: goal?.progress_type ?? "none",
    unit: goal?.unit ?? "",
    baseline: goal?.baseline ? String(Number(goal.baseline)) : "",
    target: goal?.target ? String(Number(goal.target)) : "",
    direction: goal?.direction ?? "",
  };
}

export function GoalFormDialog({
  open,
  onClose,
  goal = null,
  kind = "month",
  periodKey,
  parentId,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  goal?: Goal | null;
  kind?: "month" | "week";
  periodKey: string;
  parentId?: string;
  onSaved?: (g: Goal) => void;
}) {
  const [v, setV] = useState<GoalFormValues>(() => initial(goal, kind, periodKey, parentId));
  const [err, setErr] = useState<string | null>(null);
  const areas = useAreas();
  const { create, update } = useGoalMutations();
  const toast = useToast();
  // Posibles objetivos mensuales a los que apoya una semana: el mes de su lunes y el siguiente.
  const monthOfWeek = v.period_key.length >= 7 ? v.period_key.slice(0, 4) : "";
  const months = useGoals({ kind: "month", status: "active" });
  const set = (p: Partial<GoalFormValues>) => setV((x) => ({ ...x, ...p }));
  const editing = goal !== null;

  const submit = async () => {
    setErr(null);
    const r = valuesToBody(v);
    if (!r.ok) return setErr(r.error);
    try {
      const saved = editing
        ? await update.mutateAsync({
            id: goal.id,
            body: { ...goalToBody(goal, r.body), kind: null, period_key: null },
          })
        : await create.mutateAsync(r.body);
      toast.show(editing ? "Objetivo guardado" : "Objetivo creado", { tone: "ok" });
      onSaved?.(saved.goal);
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.isConflict)
        setErr("Otro dispositivo cambió este objetivo. Cierra y vuelve a abrir para ver la versión actual.");
      else setErr(errorMessage(e));
    }
  };

  const ptype = PROGRESS_TYPE[v.progress_type];
  return (
    <Dialog
      open={open}
      onClose={onClose}
      size="lg"
      title={
        editing
          ? "Editar objetivo"
          : v.kind === "month"
            ? "Nuevo objetivo del mes"
            : "Nuevo objetivo de la semana"
      }
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={() => void submit()} loading={create.isPending || update.isPending}>
            Guardar
          </Button>
        </>
      }
    >
      <div className="grid gap-3 sm:grid-cols-2">
        {!editing && (
          <>
            <SelectField
              label="Tipo"
              value={v.kind}
              onChange={(e) => set({ kind: e.target.value as "month" | "week" })}
            >
              <option value="month">Mensual</option>
              <option value="week">Semanal</option>
            </SelectField>
            <PeriodField kind={v.kind} value={v.period_key} onChange={(period_key) => set({ period_key })} />
          </>
        )}
        <TextField
          className="sm:col-span-2"
          label="Objetivo"
          value={v.title}
          onChange={(e) => set({ title: e.target.value })}
          placeholder="Concreto y medible"
        />
        <SelectField label="Área" value={v.area_id} onChange={(e) => set({ area_id: e.target.value })}>
          <option value="">Sin área</option>
          {(areas.data ?? []).map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        {v.kind === "week" && (
          <SelectField
            label="Apoya a (objetivo del mes)"
            value={v.parent_goal_id}
            onChange={(e) => set({ parent_goal_id: e.target.value })}
          >
            <option value="">Ninguno</option>
            {(months.data ?? [])
              .filter((m) => !monthOfWeek || m.goal.period_key.startsWith(monthOfWeek))
              .map((m) => (
                <option key={m.goal.id} value={m.goal.id}>
                  {m.goal.period_key} · {m.goal.title}
                </option>
              ))}
          </SelectField>
        )}
        <TextArea
          label="Por qué importa"
          rows={2}
          value={v.motivation}
          onChange={(e) => set({ motivation: e.target.value })}
        />
        <TextArea
          label="Cómo sé que lo logré"
          rows={2}
          value={v.success_criteria}
          onChange={(e) => set({ success_criteria: e.target.value })}
        />
        <SelectField
          className="sm:col-span-2"
          label="Medición del resultado"
          value={v.progress_type}
          onChange={(e) => set({ progress_type: e.target.value })}
          hint={ptype?.hint}
        >
          {Object.entries(PROGRESS_TYPE).map(([k, t]) => (
            <option key={k} value={k}>
              {t.label}
            </option>
          ))}
        </SelectField>
        {(v.progress_type === "cumulative" || v.progress_type === "current_value") && (
          <>
            <TextField
              label="Unidad"
              value={v.unit}
              onChange={(e) => set({ unit: e.target.value })}
              placeholder="libros, sesiones, kg"
            />
            <TextField
              label="Meta"
              inputMode="decimal"
              value={v.target}
              onChange={(e) => set({ target: e.target.value })}
            />
            <TextField
              label="Línea base (opcional)"
              inputMode="decimal"
              value={v.baseline}
              onChange={(e) => set({ baseline: e.target.value })}
            />
            {v.progress_type === "current_value" && (
              <SelectField
                label="Dirección"
                value={v.direction}
                onChange={(e) => set({ direction: e.target.value })}
              >
                <option value="">Elige…</option>
                <option value="decrease">Bajar</option>
                <option value="increase">Subir</option>
              </SelectField>
            )}
          </>
        )}
        {err && (
          <p role="alert" className="text-sm text-danger sm:col-span-2">
            {err}
          </p>
        )}
      </div>
    </Dialog>
  );
}

function PeriodField({
  kind,
  value,
  onChange,
}: {
  kind: "month" | "week";
  value: string;
  onChange: (k: string) => void;
}) {
  if (kind === "month") {
    const v = isValidMonthKey(value) ? value : String(value).slice(0, 7);
    return (
      <TextField
        label="Mes"
        type="month"
        value={v}
        onChange={(e) => onChange(e.target.value)}
        hint={isValidMonthKey(v) ? formatMonth(v) : undefined}
      />
    );
  }
  const base = isValidWeekKey(value) ? value : null;
  const options = base ? [-2, -1, 0, 1, 2, 3, 4].map((n) => shiftWeek(base, n)) : [];
  return (
    <SelectField label="Semana" value={base ?? ""} onChange={(e) => onChange(e.target.value)}>
      {!base && <option value="">Elige…</option>}
      {options.map((k) => (
        <option key={k} value={k}>
          {formatWeek(k)}
        </option>
      ))}
    </SelectField>
  );
}
