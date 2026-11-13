// /captura/:id — revisar lo que se extrajo de una captura (bot o app), editar, y aplicar o descartar con
// confirmación explícita. Sin IA configurada, la misma pantalla es el formulario manual completo.
import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatLongDate } from "../../lib/format";
import { appliedLinks } from "./applied";
import { type CaptureView, useCapture, useCaptureCatalogs, useCaptureMutations } from "./captureApi";
import { ProposalEditor } from "./ProposalEditor";
import {
  type CaptureQuestion,
  cleanProposal,
  describeProposal,
  INTENT_LABEL,
  isProposalEmpty,
  localWarnings,
  type Proposal,
  parseProposal,
  STATUS_LABEL,
} from "./proposal";

function asQuestions(v: unknown): CaptureQuestion[] {
  if (!Array.isArray(v)) return [];
  return v
    .map((q) => q as Partial<CaptureQuestion>)
    .filter((q): q is CaptureQuestion => typeof q.field === "string" && typeof q.message === "string");
}

/** Estado de la IA para esta captura, siempre visible: nunca se finge una interpretación. */
export function LlmModeNotice({
  capture,
}: {
  capture: Pick<CaptureView, "mode" | "llm_configured" | "llm_model" | "status">;
}) {
  if (capture.mode === "llm")
    return (
      <p className="rounded-lg border border-line bg-surface-2 p-3 text-sm">
        Propuesta interpretada por IA{capture.llm_model ? ` (${capture.llm_model})` : ""}. Revísala: solo se
        aplica lo que confirmes.
      </p>
    );
  return (
    <div role="note" className="rounded-lg border border-warn bg-surface p-3 text-sm">
      <p className="font-medium">Modo manual</p>
      <p className="mt-1 text-muted">
        {capture.llm_configured
          ? "La IA no pudo interpretar el texto. Completa la propuesta con el formulario."
          : "La IA no está configurada en el servidor. Llena la propuesta con el formulario; nada se completa automáticamente."}{" "}
        <Link to="/ajustes/integraciones">Ver estado de integraciones</Link>.
      </p>
    </div>
  );
}

export function CapturePage() {
  const { id = "" } = useParams();
  const q = useCapture(id);
  if (q.isPending)
    return (
      <div>
        <PageHeader title="Revisar captura" />
        <LoadingState label="Interpretando la captura…" rows={4} />
      </div>
    );
  if (q.isError)
    return (
      <div>
        <PageHeader title="Revisar captura" />
        <ErrorState error={q.error} onRetry={() => q.refetch()} />
      </div>
    );
  return <CaptureReview key={q.data.id} capture={q.data} />;
}

function CaptureReview({ capture }: { capture: CaptureView }) {
  const toast = useToast();
  const final = capture.status === "applied" || capture.status === "rejected";
  const [draft, setDraft] = useState<Proposal>(() => parseProposal(capture.proposal));
  const [dirty, setDirty] = useState(false);
  const [confirmApply, setConfirmApply] = useState(false);
  const [confirmReject, setConfirmReject] = useState(false);
  const [conflict, setConflict] = useState<CaptureView | null>(null);
  const [blocking, setBlocking] = useState<CaptureQuestion[]>([]);
  const m = useCaptureMutations(capture.id);
  const catalogs = useCaptureCatalogs(!final);
  const questions = useMemo(
    () => [...asQuestions(capture.questions), ...blocking],
    [capture.questions, blocking],
  );
  const cleaned = cleanProposal(draft, capture.allowed_fields);
  const warnings = localWarnings(cleaned);
  const willCreate = describeProposal(cleaned);
  const statusInfo = STATUS_LABEL[capture.status] ?? { label: capture.status, tone: "muted" as const };

  // Una versión nueva del servidor reemplaza el borrador solo si no hay ediciones locales; si las hay, se
  // conservan y el siguiente guardado mostrará el conflicto (nunca se descartan en silencio).
  // biome-ignore lint/correctness/useExhaustiveDependencies: solo reacciona a cambios de versión/estado del servidor
  useEffect(() => {
    if (!dirty) setDraft(parseProposal(capture.proposal));
  }, [capture.version, capture.status]);

  // Aviso al salir con cambios sin guardar en el servidor.
  useEffect(() => {
    if (!dirty) return;
    const h = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", h);
    return () => window.removeEventListener("beforeunload", h);
  }, [dirty]);

  const edit = (p: Proposal) => {
    setDraft(p);
    setDirty(true);
  };

  const save = async (version = capture.version): Promise<CaptureView | null> => {
    try {
      const v = await m.update.mutateAsync({ proposal: cleaned, version });
      setDraft(parseProposal(v.proposal));
      setDirty(false);
      setBlocking([]);
      return v;
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as CaptureView);
      else toast.show(errorMessage(e), { tone: "error" });
      return null;
    }
  };

  const apply = async () => {
    let version = capture.version;
    if (dirty) {
      const saved = await save();
      if (!saved) {
        setConfirmApply(false);
        return;
      }
      if (saved.status === "needs_input") {
        setConfirmApply(false);
        toast.show("Faltan datos por confirmar antes de aplicar.", { tone: "error" });
        return;
      }
      version = saved.version;
    }
    try {
      await m.apply.mutateAsync(version);
      setConfirmApply(false);
      toast.show("Captura aplicada", { tone: "ok" });
    } catch (e) {
      setConfirmApply(false);
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as CaptureView);
      else if (e instanceof ApiError && e.code === "validation" && Array.isArray(e.current)) {
        setBlocking(asQuestions(e.current));
        toast.show(e.message, { tone: "error" });
      } else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const reject = async () => {
    try {
      await m.reject.mutateAsync();
      setConfirmReject(false);
      toast.show("Captura descartada; no se aplicó nada");
    } catch (e) {
      setConfirmReject(false);
      toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const links = appliedLinks(capture.applied);

  return (
    <div className="space-y-4">
      <PageHeader
        title={INTENT_LABEL[capture.intent] ?? "Captura"}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Badge tone={statusInfo.tone}>{statusInfo.label}</Badge>
            <span>{capture.source === "telegram" ? "Desde Telegram" : "Desde la app"}</span>
            <span>· Fecha de referencia: {formatLongDate(capture.reference_date)}</span>
          </span>
        }
        actions={<ButtonLink to="/captura/nueva">Nueva captura</ButtonLink>}
      />

      <Card title="Lo que escribiste">
        <blockquote className="whitespace-pre-wrap break-words border-l-4 border-line pl-3 text-base">
          {capture.input_text}
        </blockquote>
      </Card>

      {!final && <LlmModeNotice capture={capture} />}

      {capture.errors.length > 0 && (
        <div role="note" className="rounded-lg border border-warn bg-surface p-3 text-sm">
          <p className="font-medium">Avisos de la validación</p>
          <ul className="mt-1 list-disc pl-5">
            {capture.errors.map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        </div>
      )}

      {!final && questions.length > 0 && (
        <div role="alert" className="rounded-lg border border-warn bg-surface p-3 text-sm">
          <p className="font-medium">Necesito que confirmes estos datos (no se suponen):</p>
          <ul className="mt-1 list-disc pl-5">
            {questions.map((qq) => (
              <li key={`${qq.field}:${qq.message}`}>{qq.message}</li>
            ))}
          </ul>
        </div>
      )}

      {capture.status === "applied" && (
        <Card title="Aplicada">
          <p className="text-sm text-muted">Los registros se crearon una sola vez. Repetir no duplica.</p>
          {links.length > 0 ? (
            <ul className="mt-2 flex flex-wrap gap-2">
              {links.map((l) => (
                <li key={l.label + l.to}>
                  <ButtonLink size="sm" to={l.to}>
                    {l.label}
                  </ButtonLink>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-2 text-sm">Se registró la sugerencia.</p>
          )}
        </Card>
      )}
      {capture.status === "rejected" && (
        <p className="rounded-lg border border-line bg-surface p-3 text-sm">Descartada: no se aplicó nada.</p>
      )}

      {suggestedHabits(capture.meta).length > 0 && (
        <Card title="Hábitos sugeridos para el objetivo">
          <ul className="list-disc space-y-1 pl-5 text-sm">
            {suggestedHabits(capture.meta).map((h) => (
              <li key={h}>{h}</li>
            ))}
          </ul>
          <p className="mt-2 text-xs text-muted">
            {final
              ? "Estos hábitos se aplicaron al objetivo."
              : "Al aplicar se crean como hábitos diarios del objetivo. Si no te convencen, descarta la propuesta."}
          </p>
        </Card>
      )}

      <Card title={final ? "Propuesta" : "Revisa y edita la propuesta"}>
        {!final && isProposalEmpty(cleaned) && (
          <p className="mb-3 text-sm text-muted">
            La propuesta está vacía. Agrega lo que corresponda con los botones de cada sección.
          </p>
        )}
        <ProposalEditor
          value={draft}
          onChange={edit}
          allowed={capture.allowed_fields}
          catalogs={catalogs}
          questions={questions}
          readOnly={final}
        />
        {warnings.length > 0 && (
          <ul role="alert" className="mt-3 list-disc pl-5 text-sm text-danger">
            {warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        )}
      </Card>

      {!final && (
        <div className="sticky bottom-20 z-10 flex flex-wrap justify-end gap-2 rounded-xl border border-line bg-surface p-3 shadow md:bottom-4">
          <span className="mr-auto self-center text-sm text-muted" aria-live="polite">
            {dirty ? "Cambios sin guardar" : "Sin cambios pendientes"}
          </span>
          <Button variant="ghost" onClick={() => setConfirmReject(true)}>
            Descartar
          </Button>
          <Button
            variant="secondary"
            disabled={!dirty}
            loading={m.update.isPending}
            onClick={() => void save()}
          >
            Guardar cambios
          </Button>
          <Button
            disabled={isProposalEmpty(cleaned) || warnings.length > 0}
            onClick={() => setConfirmApply(true)}
          >
            Aplicar…
          </Button>
        </div>
      )}

      <ConfirmDialog
        open={confirmApply}
        title="¿Aplicar esta captura?"
        confirmLabel="Sí, aplicar"
        loading={m.apply.isPending || m.update.isPending}
        onClose={() => setConfirmApply(false)}
        onConfirm={() => void apply()}
        message={
          <div className="space-y-2">
            <p>Se registrará:</p>
            <ul className="list-disc pl-5">
              {willCreate.map((x) => (
                <li key={x}>{x}</li>
              ))}
            </ul>
            <p className="text-sm text-muted">
              Fecha de referencia: {formatLongDate(capture.reference_date)}.
            </p>
          </div>
        }
      />
      <ConfirmDialog
        open={confirmReject}
        title="¿Descartar la captura?"
        confirmLabel="Descartar"
        danger
        loading={m.reject.isPending}
        onClose={() => setConfirmReject(false)}
        onConfirm={() => void reject()}
        message="No se aplicará nada. El texto original queda en el historial de capturas."
      />
      <Dialog
        open={conflict !== null}
        onClose={() => setConflict(null)}
        title="La captura cambió en otro dispositivo"
        footer={
          <>
            <Button
              variant="secondary"
              onClick={() => {
                if (conflict) {
                  setDraft(parseProposal(conflict.proposal));
                  setDirty(false);
                }
                setConflict(null);
              }}
            >
              Usar la versión guardada
            </Button>
            <Button
              loading={m.update.isPending}
              onClick={async () => {
                const v = conflict?.version;
                setConflict(null);
                if (v !== undefined && conflict?.status !== "applied" && conflict?.status !== "rejected")
                  await save(v);
              }}
              disabled={conflict?.status === "applied" || conflict?.status === "rejected"}
            >
              Guardar mis cambios encima
            </Button>
          </>
        }
      >
        <p>
          Otra sesión guardó una versión distinta
          {conflict?.status === "applied"
            ? " y ya la aplicó"
            : conflict?.status === "rejected"
              ? " y la descartó"
              : ""}
          . Elige cuál conservar; nada se sobrescribe sin tu decisión.
        </p>
        {conflict && (
          <p className="mt-2 text-sm text-muted">
            Versión guardada: {describeProposal(parseProposal(conflict.proposal)).join(", ") || "vacía"}.
          </p>
        )}
      </Dialog>
    </div>
  );
}

/** Hábitos que la IA propuso para un objetivo existente (van en `meta`, no en la propuesta). */
function suggestedHabits(meta: unknown): string[] {
  const h = (meta as { habits?: unknown } | null)?.habits;
  return Array.isArray(h) ? h.filter((x): x is string => typeof x === "string" && x.trim() !== "") : [];
}
