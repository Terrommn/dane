// Editor de frecuencia: diaria, días seleccionados o N veces por semana (4/semana no significa obligatorio diario).
import { SelectField, TextField } from "../../components/ui/Field";
import { type FrequencyBody, WEEKDAY_BITS } from "./habitsApi";

export function validateFrequency(f: FrequencyBody): string | null {
  if (f.frequency === "weekdays" && !(f.weekdays && f.weekdays > 0)) return "Elige al menos un día";
  if (f.frequency === "times_per_week") {
    const n = f.times_per_week ?? 0;
    if (!(Number.isInteger(n) && n >= 1 && n <= 7)) return "Veces por semana: de 1 a 7";
  }
  if (f.target_quantity && !/^\d+(\.\d{1,2})?$/.test(f.target_quantity.trim()))
    return "Meta por ocurrencia inválida";
  return null;
}

/** Cuerpo limpio para el servidor (sin campos que no aplican a la frecuencia elegida). */
export function cleanFrequency(f: FrequencyBody): FrequencyBody {
  return {
    frequency: f.frequency,
    weekdays: f.frequency === "weekdays" ? (f.weekdays ?? null) : null,
    times_per_week: f.frequency === "times_per_week" ? (f.times_per_week ?? null) : null,
    target_quantity: f.target_quantity?.trim() ? f.target_quantity.trim() : null,
  };
}

export function FrequencyFields({
  value,
  onChange,
  unit,
}: {
  value: FrequencyBody;
  onChange: (v: FrequencyBody) => void;
  unit?: string | null;
}) {
  return (
    <div className="space-y-3">
      <SelectField
        label="Frecuencia"
        value={value.frequency}
        onChange={(e) => onChange({ ...value, frequency: e.target.value })}
      >
        <option value="daily">Diaria</option>
        <option value="weekdays">Días seleccionados</option>
        <option value="times_per_week">N veces por semana (cualquier día)</option>
      </SelectField>
      {value.frequency === "weekdays" && (
        <fieldset>
          <legend className="mb-1 text-sm font-medium">Días</legend>
          <div className="flex flex-wrap gap-1.5">
            {WEEKDAY_BITS.map((d) => {
              const on = ((value.weekdays ?? 0) & d.bit) !== 0;
              return (
                <label key={d.bit}>
                  <input
                    type="checkbox"
                    className="peer sr-only"
                    checked={on}
                    onChange={() => onChange({ ...value, weekdays: (value.weekdays ?? 0) ^ d.bit })}
                  />
                  <span
                    title={d.long}
                    className="flex h-10 w-10 cursor-pointer items-center justify-center rounded-lg border border-line peer-checked:bg-accent peer-checked:text-accent-fg peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent"
                  >
                    <span aria-hidden>{d.short}</span>
                    <span className="sr-only">{d.long}</span>
                  </span>
                </label>
              );
            })}
          </div>
        </fieldset>
      )}
      {value.frequency === "times_per_week" && (
        <TextField
          label="Veces por semana"
          type="number"
          min={1}
          max={7}
          value={value.times_per_week ?? ""}
          onChange={(e) =>
            onChange({ ...value, times_per_week: e.target.value ? Number(e.target.value) : null })
          }
          hint="La semana cuenta si alcanzas N; ningún día individual es obligatorio."
        />
      )}
      <TextField
        label={`Meta por ocurrencia${unit ? ` (${unit})` : ""} — opcional`}
        inputMode="decimal"
        value={value.target_quantity ?? ""}
        onChange={(e) => onChange({ ...value, target_quantity: e.target.value })}
        hint="Ej. 20 (minutos). No suma automáticamente a objetivos con otra unidad."
      />
    </div>
  );
}
