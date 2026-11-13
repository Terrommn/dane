// /captura/nueva — captura libre desde la app (equivalente a /daily, /dump, /gasto… del vault y del bot),
// historial de capturas y sugerencias asistidas. Todo queda como propuesta que se revisa antes de aplicar.
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { useCaptureMutations, useCaptures, useGoalsForSuggest } from "./captureApi";
import { INTENT_LABEL, INTENTS, STATUS_LABEL } from "./proposal";

function useLlmStatus() {
  return useQuery({
    queryKey: ["integrations"],
    queryFn: async () => unwrap(await api.GET("/integrations")),
  });
}

/** Semana ISO actual `GGGG-Www` (para el resumen semanal sugerido). */
export function isoWeekKey(d: Date): string {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const year = t.getUTCFullYear();
  const week = Math.ceil(((t.getTime() - Date.UTC(year, 0, 1)) / 86400000 + 1) / 7);
  return `${year}-W${String(week).padStart(2, "0")}`;
}

function NewCaptureForm({ llm }: { llm: { configured: boolean; model?: string | null } | undefined }) {
  const [intent, setIntent] = useState("daily");
  const [text, setText] = useState("");
  const [date, setDate] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const m = useCaptureMutations();
  const navigate = useNavigate();
  const selected = INTENTS.find((i) => i.value === intent);
  return (
    <form
      className="space-y-3"
      onSubmit={async (e) => {
        e.preventDefault();
        setErr(null);
        if (!text.trim()) {
          setErr("Escribe algo para capturar.");
          return;
        }
        try {
          const v = await m.create.mutateAsync({ intent, text, reference_date: date || null });
          navigate(`/captura/${v.id}`);
        } catch (ex) {
          setErr(errorMessage(ex));
        }
      }}
    >
      <SelectField
        label="¿Qué quieres capturar?"
        value={intent}
        onChange={(e) => setIntent(e.target.value)}
        hint={selected?.hint}
      >
        {INTENTS.map((i) => (
          <option key={i.value} value={i.value}>
            {i.label} — {i.command}
          </option>
        ))}
      </SelectField>
      <TextArea
        label="Texto libre"
        rows={5}
        value={text}
        maxLength={8000}
        placeholder="Ej. ayer 250 de gasolina con débito, hoy 180 cena con Ana"
        error={err ?? undefined}
        onChange={(e) => setText(e.target.value)}
      />
      <TextField
        label="Fecha de referencia (opcional)"
        type="date"
        value={date}
        hint="Si la dejas vacía se usa hoy en tu zona. Las fechas relativas («ayer») se resuelven contra esta."
        onChange={(e) => setDate(e.target.value)}
      />
      <p className="text-sm text-muted">
        {llm?.configured
          ? `Se interpretará con IA (${llm.model ?? "modelo configurado"}) solo con el contexto mínimo necesario. Revisarás la propuesta antes de aplicar nada.`
          : "La IA no está configurada: se abrirá el formulario manual con tu texto como referencia."}
      </p>
      <Button type="submit" loading={m.create.isPending}>
        Capturar
      </Button>
    </form>
  );
}

function SuggestionsCard({ llmConfigured }: { llmConfigured: boolean }) {
  const m = useCaptureMutations();
  const navigate = useNavigate();
  const toast = useToast();
  const [week, setWeek] = useState(() => isoWeekKey(new Date()));
  const [goalId, setGoalId] = useState("");
  const goals = useGoalsForSuggest(llmConfigured);
  const run = async (body: { kind: string; iso_week?: string | null; goal_id?: string | null }) => {
    try {
      const v = await m.suggest.mutateAsync(body);
      navigate(`/captura/${v.id}`);
    } catch (e) {
      toast.show(e instanceof ApiError && e.code === "llm_not_configured" ? e.message : errorMessage(e), {
        tone: "error",
      });
    }
  };
  if (!llmConfigured)
    return (
      <Card title="Sugerencias asistidas">
        <p className="text-sm text-muted">
          Requieren la integración de IA (ANTHROPIC_API_KEY en el servidor). Sin ella, la revisión semanal, el
          Inbox y los hábitos se completan a mano en sus pantallas.
        </p>
      </Card>
    );
  const activeGoals = (goals.data ?? []).filter((g) => g.goal.status === "active" && !g.goal.archived_at);
  return (
    <Card title="Sugerencias asistidas">
      <p className="mb-3 text-sm text-muted">
        La IA propone; tú revisas y confirmas. Solo se envían los datos mínimos de cada sugerencia.
      </p>
      <div className="space-y-4">
        <div className="flex flex-wrap items-end gap-2">
          <TextField
            className="min-w-0 flex-1"
            label="Resumen de la semana"
            value={week}
            pattern="\d{4}-W\d{2}"
            hint="Formato GGGG-Www"
            onChange={(e) => setWeek(e.target.value)}
          />
          <Button
            variant="secondary"
            loading={m.suggest.isPending}
            onClick={() => void run({ kind: "weekly_summary", iso_week: week })}
          >
            Proponer resumen
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <p className="min-w-0 flex-1 text-sm">
            Clasificar los elementos pendientes del Inbox como pendientes por área.
          </p>
          <Button
            variant="secondary"
            loading={m.suggest.isPending}
            onClick={() => void run({ kind: "inbox_classification" })}
          >
            Proponer clasificación
          </Button>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <SelectField
            className="min-w-0 flex-1"
            label="Hábitos para un objetivo"
            value={goalId}
            onChange={(e) => setGoalId(e.target.value)}
          >
            <option value="">Elige un objetivo activo</option>
            {activeGoals.map((g) => (
              <option key={g.goal.id} value={g.goal.id}>
                {g.goal.title} ({g.goal.kind === "month" ? "mes" : "semana"} {g.goal.period_key})
              </option>
            ))}
          </SelectField>
          <Button
            variant="secondary"
            disabled={!goalId}
            loading={m.suggest.isPending}
            onClick={() => void run({ kind: "goal_habits", goal_id: goalId })}
          >
            Proponer hábitos
          </Button>
        </div>
      </div>
    </Card>
  );
}

export function NewCapturePage() {
  const status = useLlmStatus();
  const captures = useCaptures();
  const llm = status.data?.llm;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Captura libre"
        subtitle="Escribe como en el bot; nada se guarda en tus registros hasta que confirmes la propuesta."
      />
      <div className="grid min-w-0 gap-4 lg:grid-cols-2">
        <Card title="Nueva captura">
          <NewCaptureForm llm={llm} />
        </Card>
        <SuggestionsCard llmConfigured={llm?.configured === true} />
      </div>
      <Card title="Capturas recientes">
        <QueryState
          query={captures}
          isEmpty={(d) => d.length === 0}
          empty={
            <EmptyState title="Sin capturas todavía">
              Lo que envíes desde el bot o desde aquí aparecerá en esta lista.
            </EmptyState>
          }
        >
          {(list) => (
            <ul className="divide-y divide-line">
              {list.map((c) => {
                const st = STATUS_LABEL[c.status] ?? { label: c.status, tone: "muted" as const };
                return (
                  <li key={c.id} className="flex min-w-0 flex-wrap items-center gap-2 py-2">
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-medium">{c.input_text}</p>
                      <p className="text-sm text-muted">
                        {INTENT_LABEL[c.intent] ?? c.intent} · {c.source === "telegram" ? "Telegram" : "App"}{" "}
                        · {formatShortDate(c.reference_date)}
                      </p>
                    </div>
                    <Badge tone={st.tone}>{st.label}</Badge>
                    <ButtonLink
                      size="sm"
                      to={`/captura/${c.id}`}
                      aria-label={`Abrir captura: ${c.input_text.slice(0, 40)}`}
                    >
                      Abrir
                    </ButtonLink>
                  </li>
                );
              })}
            </ul>
          )}
        </QueryState>
        {captures.dataUpdatedAt > 0 && (
          <p className="mt-2 text-xs text-muted">
            Actualizado {formatDateTime(new Date(captures.dataUpdatedAt).toISOString())}
          </p>
        )}
      </Card>
    </div>
  );
}
