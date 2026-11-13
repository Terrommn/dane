// Resultado de aplicar una captura (`applied.created[]`) → enlaces navegables a los registros creados.

export interface AppliedLink {
  label: string;
  to: string;
}

const LABEL: Record<string, string> = {
  transaction: "Movimiento",
  task: "Pendiente",
  inbox: "Inbox",
  checkin: "Check-in",
  goal: "Objetivo",
  devlog_session: "Sesión de devlog",
  weekly_review: "Revisión semanal",
  interaction: "Interacción",
  person_suggestion: "Sugerencia de persona",
  reflection: "Reflexión",
  log: "Log",
};

export function appliedLinks(applied: unknown): AppliedLink[] {
  const created = (applied as { created?: unknown } | null)?.created;
  if (!Array.isArray(created)) return [];
  const out: AppliedLink[] = [];
  for (const c of created) {
    if (!c || typeof c !== "object") continue;
    const x = c as Record<string, unknown>;
    const type = typeof x.type === "string" ? x.type : "otro";
    const label = LABEL[type] ?? type;
    const date = typeof x.date === "string" ? x.date : null;
    const week = typeof x.iso_week === "string" ? x.iso_week : null;
    const to =
      type === "transaction"
        ? "/finanzas/movimientos"
        : type === "task"
          ? "/tareas"
          : type === "inbox"
            ? "/inbox"
            : type === "checkin"
              ? `/hoy${date ? `?date=${date}` : ""}`
              : type === "goal"
                ? "/objetivos"
                : type === "devlog_session"
                  ? "/dev"
                  : type === "weekly_review"
                    ? `/semanal${week ? `?week=${week}` : ""}`
                    : type === "interaction" || type === "person_suggestion"
                      ? "/personas"
                      : "/hoy";
    out.push({ label, to });
  }
  // Agrupa por destino: "3 × Movimiento".
  const grouped = new Map<string, { label: string; to: string; n: number }>();
  for (const l of out) {
    const k = `${l.label}|${l.to}`;
    const g = grouped.get(k);
    if (g) g.n++;
    else grouped.set(k, { ...l, n: 1 });
  }
  return [...grouped.values()].map((g) => ({ label: g.n > 1 ? `${g.n} × ${g.label}` : g.label, to: g.to }));
}
