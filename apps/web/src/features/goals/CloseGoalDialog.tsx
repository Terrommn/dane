// Cierre de objetivo: decisión del usuario (logrado / no logrado) con logro, aprendizaje, dificultades y ajustes.
// Es una acción distinta de editar y de archivar.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { RadioGroup, TextArea } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { type Goal, type GoalClosing, useGoalMutations } from "./goalsApi";

export function CloseGoalDialog({ goal, open, onClose }: { goal: Goal; open: boolean; onClose: () => void }) {
  const prev = (goal.closing ?? {}) as GoalClosing;
  const [status, setStatus] = useState<"achieved" | "not_achieved">(
    goal.status === "not_achieved" ? "not_achieved" : "achieved",
  );
  const [c, setC] = useState<Required<GoalClosing>>({
    achievement: prev.achievement ?? "",
    learning: prev.learning ?? "",
    difficulties: prev.difficulties ?? "",
    adjustments: prev.adjustments ?? "",
  });
  const [err, setErr] = useState<string | null>(null);
  const { close } = useGoalMutations();
  const toast = useToast();

  const submit = async () => {
    setErr(null);
    try {
      await close.mutateAsync({ id: goal.id, body: { status, ...c, version: goal.version } });
      toast.show("Objetivo cerrado", { tone: "ok" });
      onClose();
    } catch (e) {
      setErr(
        e instanceof ApiError && e.isConflict
          ? "El objetivo cambió en otro dispositivo; vuelve a abrir el cierre."
          : errorMessage(e),
      );
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Cerrar: ${goal.title}`}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={() => void submit()} loading={close.isPending}>
            Cerrar objetivo
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <RadioGroup
          legend="¿Lo lograste? (tu decisión)"
          name={`close-${goal.id}`}
          value={status}
          onChange={setStatus}
          options={[
            { value: "achieved", label: "Logrado" },
            { value: "not_achieved", label: "No logrado" },
          ]}
        />
        <TextArea
          label="Logro"
          rows={2}
          value={c.achievement}
          onChange={(e) => setC({ ...c, achievement: e.target.value })}
        />
        <TextArea
          label="Qué aprendí"
          rows={2}
          value={c.learning}
          onChange={(e) => setC({ ...c, learning: e.target.value })}
        />
        <TextArea
          label="Dificultades"
          rows={2}
          value={c.difficulties}
          onChange={(e) => setC({ ...c, difficulties: e.target.value })}
        />
        <TextArea
          label="Próximos ajustes"
          rows={2}
          value={c.adjustments}
          onChange={(e) => setC({ ...c, adjustments: e.target.value })}
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
