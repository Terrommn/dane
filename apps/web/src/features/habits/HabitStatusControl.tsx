// Estado de un hábito en un día: cumplido / no / no aplica (con motivo) / sin registrar.
// "Sin registrar" es distinto de "No": en días cerrados cuenta como no cumplido, pero se muestra aparte.
import { useId } from "react";
import { HABIT_STATUS_LABEL, type HabitMark, type HabitStatus } from "../today/checkin";

const OPTIONS: { value: HabitStatus | null; label: string; symbol: string; tone: string }[] = [
  {
    value: "done",
    label: HABIT_STATUS_LABEL.done,
    symbol: "✓",
    tone: "peer-checked:bg-ok peer-checked:text-white",
  },
  {
    value: "not_done",
    label: HABIT_STATUS_LABEL.not_done,
    symbol: "✕",
    tone: "peer-checked:bg-danger peer-checked:text-white",
  },
  {
    value: "not_applicable",
    label: HABIT_STATUS_LABEL.not_applicable,
    symbol: "–",
    tone: "peer-checked:bg-surface-2 peer-checked:text-fg peer-checked:border-fg",
  },
  {
    value: null,
    label: HABIT_STATUS_LABEL.unrecorded,
    symbol: "?",
    tone: "peer-checked:border-dashed peer-checked:border-warn peer-checked:text-warn",
  },
];

export function HabitStatusControl({
  name,
  detail,
  mark,
  unit,
  onChange,
  disabled,
}: {
  name: string;
  /** Contexto: objetivo, "2/4 esta semana", meta por ocurrencia. */
  detail?: string | undefined;
  mark: HabitMark;
  /** Unidad de la cantidad (ej. "min"); si existe se muestra el campo de cantidad. */
  unit?: string | null | undefined;
  onChange: (m: HabitMark) => void;
  disabled?: boolean;
}) {
  const id = useId();
  const group = `habit-${id}`;
  return (
    <fieldset className="min-w-0 rounded-xl border border-line p-3" disabled={disabled}>
      <legend className="sr-only">{name}</legend>
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <p className="min-w-0 font-medium" aria-hidden>
          {name}
        </p>
        {detail && <p className="text-xs text-muted">{detail}</p>}
      </div>
      <div className="mt-2 grid grid-cols-2 gap-1.5 sm:grid-cols-4">
        {OPTIONS.map((o) => (
          <label key={String(o.value)} className="min-w-0">
            <input
              type="radio"
              name={group}
              className="peer sr-only"
              checked={mark.status === o.value}
              onChange={() => onChange({ ...mark, status: o.value })}
            />
            <span
              className={`flex min-h-10 cursor-pointer items-center justify-center gap-1 rounded-lg border border-line px-2 text-sm peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent ${o.tone}`}
            >
              <span aria-hidden>{o.symbol}</span>
              {o.label}
            </span>
          </label>
        ))}
      </div>
      {mark.status === "not_applicable" && (
        <div className="mt-2">
          <label htmlFor={`${group}-reason`} className="mb-1 block text-sm font-medium">
            ¿Por qué no aplica? <span className="text-muted">(no cuenta en el cumplimiento)</span>
          </label>
          <input
            id={`${group}-reason`}
            value={mark.reason}
            onChange={(e) => onChange({ ...mark, reason: e.target.value })}
            aria-invalid={!mark.reason.trim() || undefined}
            className="w-full min-w-0 rounded-lg border border-line bg-surface px-3 py-2"
            placeholder="Ej. viaje, enfermedad"
          />
        </div>
      )}
      {unit && mark.status === "done" && (
        <div className="mt-2 flex items-center gap-2">
          <label htmlFor={`${group}-qty`} className="text-sm">
            Cantidad ({unit})
          </label>
          <input
            id={`${group}-qty`}
            inputMode="decimal"
            value={mark.quantity}
            onChange={(e) => onChange({ ...mark, quantity: e.target.value })}
            className="w-24 rounded-lg border border-line bg-surface px-2 py-1"
          />
        </div>
      )}
    </fieldset>
  );
}
