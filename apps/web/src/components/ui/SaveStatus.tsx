// Indicador del estado de guardado. "Guardado" significa confirmado por el servidor; lo que solo está en el
// dispositivo se muestra como borrador o pendiente de sincronizar.
import type { DraftStatus } from "../../lib/offline/db";

export const SAVE_STATUS_LABEL: Record<DraftStatus, string> = {
  draft: "Borrador en este dispositivo",
  pending: "Pendiente de sincronizar",
  saved: "Guardado",
  conflict: "Conflicto: revisa los cambios",
  error: "Error al guardar",
};

const TONE: Record<DraftStatus, string> = {
  draft: "border-line text-muted",
  pending: "border-warn text-warn",
  saved: "border-ok text-ok",
  conflict: "border-danger text-danger",
  error: "border-danger text-danger",
};

const ICON: Record<DraftStatus, string> = { draft: "✎", pending: "⟳", saved: "✓", conflict: "⚠", error: "✕" };

export function SaveStatus({ status, detail }: { status: DraftStatus | null; detail?: string | null }) {
  if (!status) return null;
  return (
    <span
      role="status"
      aria-live="polite"
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium ${TONE[status]}`}
    >
      <span aria-hidden>{ICON[status]}</span>
      {SAVE_STATUS_LABEL[status]}
      {detail && <span className="font-normal">· {detail}</span>}
    </span>
  );
}
