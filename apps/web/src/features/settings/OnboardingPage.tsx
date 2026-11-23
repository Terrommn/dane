// Onboarding por pasos. Cada paso guarda solo lo que el usuario confirma; se puede saltar y completar después.
// Los ejemplos del vault (70 kg, 5 fallas, 4 ejercicios/semana) se proponen desmarcados: no son datos del usuario.
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Card, PageHeader } from "../../components/ui/Display";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { FixedHabitsSetup } from "../habits/FixedHabitsSetup";
import { AreasEditor } from "../projects/AreasEditor";
import {
  GoalsSection,
  ModulesSection,
  RegionSection,
  RemindersSection,
  TelegramStatusCard,
} from "./sections";
import { useSaveSettings, useSettings } from "./settingsApi";

const STEPS = [
  "Bienvenida",
  "Región",
  "Áreas",
  "Módulos",
  "Recordatorios",
  "Hábitos",
  "Metas",
  "Telegram",
] as const;

export function OnboardingPage() {
  const settings = useSettings();
  const { save, saving, conflict, resolve, dismissConflict } = useSaveSettings();
  const [step, setStep] = useState(0);
  const navigate = useNavigate();
  const toast = useToast();
  const next = () => setStep((s) => Math.min(s + 1, STEPS.length - 1));
  const finish = async () => {
    try {
      await save({ onboarding_completed: true });
      toast.show("¡Listo! Puedes cambiar todo en Ajustes.", { tone: "ok" });
      navigate("/");
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    }
  };

  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <PageHeader
        title="Configura tu app"
        subtitle={`Paso ${step + 1} de ${STEPS.length}: ${STEPS[step]}`}
        actions={
          <Button variant="ghost" onClick={() => navigate("/")}>
            Saltar por ahora
          </Button>
        }
      />
      <nav aria-label="Pasos">
        <ol className="flex flex-wrap gap-1">
          {STEPS.map((s, i) => (
            <li key={s}>
              <button
                type="button"
                onClick={() => setStep(i)}
                aria-current={i === step ? "step" : undefined}
                className={`rounded-full px-3 py-1 text-xs ${i === step ? "bg-accent text-accent-fg" : "bg-surface-2 text-muted"}`}
              >
                {i + 1}. {s}
              </button>
            </li>
          ))}
        </ol>
      </nav>
      <QueryState query={settings}>
        {(s) => (
          <>
            {step === 0 && (
              <Card>
                <p>Empieza con pocos datos: todo es opcional y editable después.</p>
                <p className="mt-2 text-sm text-muted">
                  Configuraremos zona horaria, tus áreas, módulos, recordatorios del bot y metas opcionales.
                </p>
                <Button className="mt-4" onClick={next}>
                  Empezar
                </Button>
              </Card>
            )}
            {step === 1 && (
              <RegionSection
                key={s.version}
                settings={s}
                save={save}
                saving={saving}
                submitLabel="Guardar y continuar"
                onSaved={next}
              />
            )}
            {step === 2 && (
              <Card title="Tus áreas">
                <p className="mb-3 text-sm text-muted">
                  Empiezan Personal, Trabajo, Negocio y Escuela. Renómbralas o agrega las tuyas.
                </p>
                <AreasEditor />
                <Button className="mt-4" onClick={next}>
                  Continuar
                </Button>
              </Card>
            )}
            {step === 3 && (
              <ModulesSection
                key={s.version}
                settings={s}
                save={save}
                saving={saving}
                submitLabel="Guardar y continuar"
                onSaved={next}
              />
            )}
            {step === 4 && (
              <RemindersSection
                key={s.version}
                settings={s}
                save={save}
                saving={saving}
                submitLabel="Guardar y continuar"
                onSaved={next}
              />
            )}
            {step === 5 && (
              <>
                <FixedHabitsSetup
                  devotionalMode={s.devotional_mode}
                  devotionalEnabled={(s.modules as Record<string, unknown>).devotional !== false}
                  onDone={next}
                />
                <p className="text-sm text-muted">
                  Los hábitos de tus objetivos se proponen al{" "}
                  <Link to="/objetivos/planificar">planificar</Link>; los vínculos automáticos (peso, gastos,
                  algo bueno, devocional) se configuran en <Link to="/habitos">Hábitos</Link>.
                </p>
              </>
            )}
            {step === 6 && (
              <>
                <GoalsSection
                  key={s.version}
                  settings={s}
                  save={save}
                  saving={saving}
                  submitLabel="Guardar y continuar"
                  onSaved={next}
                  asExamples
                />
                <Button variant="ghost" onClick={next}>
                  Omitir metas
                </Button>
              </>
            )}
            {step === 7 && (
              <>
                <TelegramStatusCard />
                <Button onClick={() => void finish()} loading={saving}>
                  Terminar
                </Button>
              </>
            )}
          </>
        )}
      </QueryState>
      {conflict && (
        <ConflictDialog
          open
          mine={conflict.patch as Record<string, unknown>}
          theirs={conflict.current as unknown as Record<string, unknown>}
          onClose={dismissConflict}
          onResolve={(r) => void resolve(r)}
        />
      )}
    </div>
  );
}
