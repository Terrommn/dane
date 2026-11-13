// Hábitos fijos (devocional/meditación, ejercicio, lectura). Se PROPONEN como ejemplos del vault, desmarcados; solo se
// crean los que el usuario marque y confirme (POST /habits/fixed). Ejercicio es N/semana: no es obligatorio cada día.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Card } from "../../components/ui/Display";
import { CheckboxField, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { cleanFrequency, FrequencyFields, validateFrequency } from "./FrequencyFields";
import { type FrequencyBody, useHabitMutations } from "./habitsApi";

type Key = "devotional" | "exercise" | "reading";
interface Row {
  enabled: boolean;
  name: string;
  freq: FrequencyBody;
}

export function defaultFixedRows(devotionalMode: string): Record<Key, Row> {
  return {
    devotional: {
      enabled: false,
      name: devotionalMode === "meditation" ? "Meditación" : "Devocional",
      freq: { frequency: "daily" },
    },
    exercise: { enabled: false, name: "Ejercicio", freq: { frequency: "times_per_week", times_per_week: 4 } },
    reading: { enabled: false, name: "Lectura", freq: { frequency: "daily" } },
  };
}

/** Cuerpo para el servidor: solo los marcados (los no marcados se omiten para no archivar existentes). */
export function fixedBody(rows: Record<Key, Row>, startsOn?: string): Schemas["FixedHabitsBody"] | string {
  const body: Schemas["FixedHabitsBody"] = { starts_on: startsOn ?? null };
  for (const k of ["devotional", "exercise", "reading"] as const) {
    const r = rows[k];
    if (!r.enabled) continue;
    const err = validateFrequency(r.freq);
    if (err) return `${r.name}: ${err}`;
    body[k] = { ...cleanFrequency(r.freq), enabled: true, name: r.name.trim() || null };
  }
  return body;
}

export function FixedHabitsSetup({
  devotionalMode,
  devotionalEnabled,
  onDone,
}: {
  devotionalMode: string;
  devotionalEnabled: boolean;
  onDone?: () => void;
}) {
  const [rows, setRows] = useState(() => defaultFixedRows(devotionalMode));
  const [err, setErr] = useState<string | null>(null);
  const { setupFixed } = useHabitMutations();
  const toast = useToast();
  const keys: Key[] = devotionalEnabled ? ["devotional", "exercise", "reading"] : ["exercise", "reading"];
  const any = keys.some((k) => rows[k].enabled);
  const set = (k: Key, r: Partial<Row>) => setRows((x) => ({ ...x, [k]: { ...x[k], ...r } }));

  return (
    <Card title="Hábitos fijos">
      <p className="mb-3 text-sm text-muted">
        Ejemplos del vault, editables. No se crea nada que no marques. Puedes cambiarlos después en Hábitos.
      </p>
      <div className="space-y-3">
        {keys.map((k) => (
          <div key={k} className="rounded-xl border border-line p-3">
            <CheckboxField
              label={rows[k].name || k}
              checked={rows[k].enabled}
              onChange={(e) => set(k, { enabled: e.target.checked })}
              hint={k === "exercise" ? "Ejemplo: 4 veces por semana (cualquier día)" : "Ejemplo: diario"}
            />
            {rows[k].enabled && (
              <div className="mt-3 space-y-3 pl-8">
                <TextField
                  label="Nombre"
                  value={rows[k].name}
                  onChange={(e) => set(k, { name: e.target.value })}
                />
                <FrequencyFields value={rows[k].freq} onChange={(freq) => set(k, { freq })} />
              </div>
            )}
          </div>
        ))}
      </div>
      {err && (
        <p role="alert" className="mt-2 text-sm text-danger">
          {err}
        </p>
      )}
      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          disabled={!any}
          loading={setupFixed.isPending}
          onClick={async () => {
            setErr(null);
            const body = fixedBody(rows);
            if (typeof body === "string") return setErr(body);
            try {
              const created = await setupFixed.mutateAsync(body);
              toast.show(`Hábitos configurados: ${created.map((h) => h.name).join(", ")}`, { tone: "ok" });
              onDone?.();
            } catch (e) {
              setErr(errorMessage(e));
            }
          }}
        >
          Crear los marcados
        </Button>
        {onDone && (
          <Button variant="ghost" onClick={onDone}>
            Omitir
          </Button>
        )}
      </div>
    </Card>
  );
}
