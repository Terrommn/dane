// Alta de hábito personalizado o de objetivo. Los fijos (ejercicio, lectura) se configuran en onboarding.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { useGoals } from "../goals/goalsApi";
import { cleanFrequency, FrequencyFields, validateFrequency } from "./FrequencyFields";
import { type FrequencyBody, useHabitMutations } from "./habitsApi";

export function HabitFormDialog({
  open,
  onClose,
  goalId,
  defaultStartsOn,
}: {
  open: boolean;
  onClose: () => void;
  /** Si viene, el hábito pertenece a ese objetivo. */
  goalId?: string;
  defaultStartsOn?: string;
}) {
  const { create } = useHabitMutations();
  const toast = useToast();
  const goals = useGoals({ status: "active" });
  const [name, setName] = useState("");
  const [kind, setKind] = useState(goalId ? "goal" : "custom");
  const [goal, setGoal] = useState(goalId ?? "");
  const [unit, setUnit] = useState("");
  const [description, setDescription] = useState("");
  const [startsOn, setStartsOn] = useState(defaultStartsOn ?? "");
  const [freq, setFreq] = useState<FrequencyBody>({ frequency: "daily" });
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setErr(null);
    if (!name.trim()) return setErr("Escribe el nombre del hábito");
    if (kind === "goal" && !goal) return setErr("Elige el objetivo al que apoya");
    const fe = validateFrequency(freq);
    if (fe) return setErr(fe);
    try {
      await create.mutateAsync({
        ...cleanFrequency(freq),
        name: name.trim(),
        kind,
        goal_id: kind === "goal" ? goal : null,
        unit: unit.trim() || null,
        description,
        starts_on: startsOn || null,
      });
      toast.show("Hábito creado", { tone: "ok" });
      setName("");
      setUnit("");
      setDescription("");
      onClose();
    } catch (e) {
      setErr(errorMessage(e));
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Nuevo hábito"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={() => void submit()} loading={create.isPending}>
            Crear
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <TextField
          label="Nombre"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Ej. Escribir 30 min en el proyecto X"
          hint="Una acción concreta que se marque como hecha o no en una línea."
        />
        {!goalId && (
          <SelectField label="Tipo" value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="custom">Personal (sin objetivo)</option>
            <option value="goal">De un objetivo</option>
          </SelectField>
        )}
        {kind === "goal" && !goalId && (
          <SelectField label="Objetivo" value={goal} onChange={(e) => setGoal(e.target.value)}>
            <option value="">Elige…</option>
            {(goals.data ?? []).map((g) => (
              <option key={g.goal.id} value={g.goal.id}>
                {g.goal.kind === "month" ? "Mes" : "Semana"} {g.goal.period_key} · {g.goal.title}
              </option>
            ))}
          </SelectField>
        )}
        <TextField
          label="Unidad de cantidad (opcional)"
          value={unit}
          onChange={(e) => setUnit(e.target.value)}
          placeholder="min, páginas"
          hint="Independiente de la unidad de cualquier objetivo (leer minutos ≠ terminar libros)."
        />
        <FrequencyFields value={freq} onChange={setFreq} unit={unit || null} />
        <TextField
          label="Desde (opcional)"
          type="date"
          value={startsOn}
          onChange={(e) => setStartsOn(e.target.value)}
          hint="Por defecto hoy o el inicio del objetivo."
        />
        <TextArea
          label="Descripción (opcional)"
          rows={2}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
        {err && (
          <p role="alert" className="text-sm text-danger">
            {err}
          </p>
        )}
      </div>
    </Dialog>
  );
}
