import { useId } from "react";
import { addDays, formatLongDate } from "../../lib/format";
import { Button } from "./Button";

/** Selector de fecha civil con día anterior/siguiente. `max` evita elegir fechas futuras cuando no aplican. */
export function DatePicker({
  value,
  onChange,
  label = "Fecha",
  max,
}: {
  value: string;
  onChange: (iso: string) => void;
  label?: string;
  max?: string;
}) {
  const id = useId();
  const next = addDays(value, 1);
  return (
    <div className="flex flex-wrap items-end gap-2">
      <Button
        variant="secondary"
        size="sm"
        aria-label="Día anterior"
        onClick={() => onChange(addDays(value, -1))}
      >
        ‹
      </Button>
      <div className="min-w-0">
        <label htmlFor={id} className="sr-only">
          {label}
        </label>
        <input
          id={id}
          type="date"
          value={value}
          max={max}
          onChange={(e) => e.target.value && onChange(e.target.value)}
          className="rounded-lg border border-line bg-surface px-3 py-1.5"
          aria-describedby={`${id}-long`}
        />
        <span id={`${id}-long`} className="sr-only">
          {formatLongDate(value)}
        </span>
      </div>
      <Button
        variant="secondary"
        size="sm"
        aria-label="Día siguiente"
        disabled={max !== undefined && next > max}
        onClick={() => onChange(next)}
      >
        ›
      </Button>
    </div>
  );
}
