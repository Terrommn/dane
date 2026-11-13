// Tipos y reglas de la propuesta de captura (espejo de `domain::capture::Proposal`; el contrato OpenAPI la expone
// como objeto libre). El servidor vuelve a validar todo: aquí solo se normaliza y se dan avisos tempranos.

export interface ExpenseItem {
  date: string | null;
  concept: string;
  amount: string | null;
  category: string | null;
  payment: string | null;
}
export interface IncomeItem {
  date: string | null;
  concept: string;
  amount: string | null;
  category: string | null;
  account: string | null;
}
export interface TaskItem {
  title: string;
  area: string | null;
  due: string | null;
  people: string[];
  clickup_candidate: boolean;
}
export interface InteractionItem {
  person: string;
  topic: string;
}
export interface CheckinItem {
  weight_kg: string | null;
  energy: number | null;
  exercise: boolean | null;
  reading: boolean | null;
  planned_spend: string | null;
}
export interface GoalItem {
  title: string;
  kind: "month" | "week";
  area: string | null;
  motivation: string | null;
  success_criteria: string | null;
  habits: string[];
}
export interface DevlogItem {
  repo: string;
  branch: string | null;
  changes: string;
  bugs: string | null;
  decisions: string | null;
  next_steps: string | null;
}
export interface Proposal {
  expenses: ExpenseItem[];
  incomes: IncomeItem[];
  tasks: TaskItem[];
  inbox: string[];
  log: string[];
  reflections_good: string[];
  reflections_bad: string[];
  interactions: InteractionItem[];
  checkin: CheckinItem | null;
  goals: GoalItem[];
  devlog: DevlogItem | null;
  summary: string | null;
  questions: string[];
}

export interface CaptureQuestion {
  field: string;
  message: string;
}

export type ProposalField =
  | "expenses"
  | "incomes"
  | "tasks"
  | "inbox"
  | "log"
  | "reflections_good"
  | "reflections_bad"
  | "interactions"
  | "checkin"
  | "goals"
  | "devlog"
  | "summary";

/** Intenciones que el usuario puede elegir al capturar (equivalen a los comandos del vault/bot). */
export const INTENTS: { value: string; label: string; hint: string; command: string }[] = [
  {
    value: "daily",
    label: "Daily",
    hint: "Lo que pasó hoy: log, reflexión, peso, energía, gastos…",
    command: "/daily",
  },
  {
    value: "dump",
    label: "Dump de pendientes",
    hint: "Lista desordenada → pendientes por área",
    command: "/dump",
  },
  { value: "gasto", label: "Gastos", hint: "Uno o varios gastos con monto y categoría", command: "/gasto" },
  {
    value: "ingreso",
    label: "Ingresos",
    hint: "Uno o varios ingresos con fecha y cuenta",
    command: "/ingreso",
  },
  {
    value: "objetivos",
    label: "Objetivos",
    hint: "Objetivos del mes o semana con sus hábitos",
    command: "/objetivos",
  },
  {
    value: "weekly_review",
    label: "Revisión semanal",
    hint: "Resumen de la semana, pendientes e Inbox",
    command: "/weekly_review (weekly-review)",
  },
  {
    value: "log_session",
    label: "Sesión de código",
    hint: "Repo, rama, cambios, bugs, decisiones y siguiente paso",
    command: "/log_session (log-session)",
  },
];

export const INTENT_LABEL: Record<string, string> = {
  ...Object.fromEntries(INTENTS.map((i) => [i.value, i.label])),
  inbox: "Clasificar Inbox",
  free: "Captura libre",
};

export const STATUS_LABEL: Record<
  string,
  { label: string; tone: "muted" | "ok" | "warn" | "danger" | "accent" }
> = {
  pending: { label: "Por confirmar", tone: "accent" },
  needs_input: { label: "Faltan datos", tone: "warn" },
  applied: { label: "Aplicada", tone: "ok" },
  rejected: { label: "Descartada", tone: "muted" },
  failed: { label: "Sin interpretar", tone: "danger" },
};

export function emptyProposal(): Proposal {
  return {
    expenses: [],
    incomes: [],
    tasks: [],
    inbox: [],
    log: [],
    reflections_good: [],
    reflections_bad: [],
    interactions: [],
    checkin: null,
    goals: [],
    devlog: null,
    summary: null,
    questions: [],
  };
}

const str = (v: unknown): string => (typeof v === "string" ? v : "");
const optStr = (v: unknown): string | null => (typeof v === "string" && v.trim() !== "" ? v : null);
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);
const strList = (v: unknown): string[] => arr(v).filter((x): x is string => typeof x === "string");
const obj = (v: unknown): Record<string, unknown> =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
const optBool = (v: unknown): boolean | null => (typeof v === "boolean" ? v : null);

/** Convierte el JSON del servidor (campos posiblemente ausentes) en una propuesta completa. */
export function parseProposal(raw: unknown): Proposal {
  const p = obj(raw);
  const checkin = p.checkin ? obj(p.checkin) : null;
  const devlog = p.devlog ? obj(p.devlog) : null;
  return {
    expenses: arr(p.expenses).map((x) => {
      const e = obj(x);
      return {
        date: optStr(e.date),
        concept: str(e.concept),
        amount: optStr(e.amount),
        category: optStr(e.category),
        payment: optStr(e.payment),
      };
    }),
    incomes: arr(p.incomes).map((x) => {
      const e = obj(x);
      return {
        date: optStr(e.date),
        concept: str(e.concept),
        amount: optStr(e.amount),
        category: optStr(e.category),
        account: optStr(e.account),
      };
    }),
    tasks: arr(p.tasks).map((x) => {
      const t = obj(x);
      return {
        title: str(t.title),
        area: optStr(t.area),
        due: optStr(t.due),
        people: strList(t.people),
        clickup_candidate: t.clickup_candidate === true,
      };
    }),
    inbox: strList(p.inbox),
    log: strList(p.log),
    reflections_good: strList(p.reflections_good),
    reflections_bad: strList(p.reflections_bad),
    interactions: arr(p.interactions).map((x) => {
      const i = obj(x);
      return { person: str(i.person), topic: str(i.topic) };
    }),
    checkin: checkin
      ? {
          weight_kg: optStr(checkin.weight_kg),
          energy: typeof checkin.energy === "number" ? checkin.energy : null,
          exercise: optBool(checkin.exercise),
          reading: optBool(checkin.reading),
          planned_spend: optStr(checkin.planned_spend),
        }
      : null,
    goals: arr(p.goals).map((x) => {
      const g = obj(x);
      return {
        title: str(g.title),
        kind: g.kind === "week" ? "week" : "month",
        area: optStr(g.area),
        motivation: optStr(g.motivation),
        success_criteria: optStr(g.success_criteria),
        habits: strList(g.habits),
      };
    }),
    devlog: devlog
      ? {
          repo: str(devlog.repo),
          branch: optStr(devlog.branch),
          changes: str(devlog.changes),
          bugs: optStr(devlog.bugs),
          decisions: optStr(devlog.decisions),
          next_steps: optStr(devlog.next_steps),
        }
      : null,
    summary: optStr(p.summary),
    questions: strList(p.questions),
  };
}

const trimOrNull = (s: string | null): string | null => {
  const t = s?.trim() ?? "";
  return t === "" ? null : t;
};
const cleanList = (xs: string[]) => xs.map((x) => x.trim()).filter(Boolean);

/**
 * Limpia la propuesta antes de enviarla: recorta textos, quita filas vacías y deja vacío lo que el intent no
 * permite (el servidor lo descartaría igual). No inventa ni completa valores.
 */
export function cleanProposal(p: Proposal, allowed: readonly string[]): Proposal {
  const ok = (f: ProposalField) => allowed.includes(f);
  const checkin = p.checkin
    ? {
        weight_kg: trimOrNull(p.checkin.weight_kg),
        energy: p.checkin.energy,
        exercise: p.checkin.exercise,
        reading: p.checkin.reading,
        planned_spend: trimOrNull(p.checkin.planned_spend),
      }
    : null;
  const checkinEmpty =
    !checkin ||
    (checkin.weight_kg === null &&
      checkin.energy === null &&
      checkin.exercise === null &&
      checkin.reading === null &&
      checkin.planned_spend === null);
  const devlog = p.devlog
    ? {
        repo: p.devlog.repo.trim(),
        branch: trimOrNull(p.devlog.branch),
        changes: p.devlog.changes.trim(),
        bugs: trimOrNull(p.devlog.bugs),
        decisions: trimOrNull(p.devlog.decisions),
        next_steps: trimOrNull(p.devlog.next_steps),
      }
    : null;
  return {
    expenses: ok("expenses")
      ? p.expenses
          .map((e) => ({
            date: trimOrNull(e.date),
            concept: e.concept.trim(),
            amount: trimOrNull(e.amount),
            category: trimOrNull(e.category),
            payment: trimOrNull(e.payment),
          }))
          .filter((e) => e.concept || e.amount)
      : [],
    incomes: ok("incomes")
      ? p.incomes
          .map((e) => ({
            date: trimOrNull(e.date),
            concept: e.concept.trim(),
            amount: trimOrNull(e.amount),
            category: trimOrNull(e.category),
            account: trimOrNull(e.account),
          }))
          .filter((e) => e.concept || e.amount)
      : [],
    tasks: ok("tasks")
      ? p.tasks
          .map((t) => ({
            title: t.title.trim(),
            area: trimOrNull(t.area),
            due: trimOrNull(t.due),
            people: cleanList(t.people),
            clickup_candidate: t.clickup_candidate,
          }))
          .filter((t) => t.title)
      : [],
    inbox: ok("inbox") ? cleanList(p.inbox) : [],
    log: ok("log") ? cleanList(p.log) : [],
    reflections_good: ok("reflections_good") ? cleanList(p.reflections_good) : [],
    reflections_bad: ok("reflections_bad") ? cleanList(p.reflections_bad) : [],
    interactions: ok("interactions")
      ? p.interactions
          .map((i) => ({ person: i.person.trim(), topic: i.topic.trim() }))
          .filter((i) => i.person)
      : [],
    checkin: ok("checkin") && !checkinEmpty ? checkin : null,
    goals: ok("goals")
      ? p.goals
          .map((g) => ({
            title: g.title.trim(),
            kind: g.kind,
            area: trimOrNull(g.area),
            motivation: trimOrNull(g.motivation),
            success_criteria: trimOrNull(g.success_criteria),
            habits: cleanList(g.habits),
          }))
          .filter((g) => g.title)
      : [],
    devlog: ok("devlog") && devlog && (devlog.repo || devlog.changes) ? devlog : null,
    summary: ok("summary") ? trimOrNull(p.summary) : null,
    questions: p.questions,
  };
}

/** Monto escrito por el usuario: "$1,250", "180.50" o "180". Máximo dos decimales, sin negativos. */
export function isValidAmount(s: string | null): boolean {
  if (s === null) return false;
  const t = s.trim().replace(/^\$\s?/, "");
  if (!/^\d{1,3}(,\d{3})*(\.\d{1,2})?$|^\d+(\.\d{1,2})?$/.test(t)) return false;
  return Number(t.replaceAll(",", "")) > 0;
}

/** Avisos locales (antes de guardar) para montos con formato inválido o pesos fuera de rango. */
export function localWarnings(p: Proposal): string[] {
  const w: string[] = [];
  p.expenses.forEach((e, i) => {
    if (e.amount !== null && e.amount.trim() !== "" && !isValidAmount(e.amount))
      w.push(`Gasto ${i + 1}: el monto «${e.amount}» no es válido (usa por ejemplo 180.50).`);
  });
  p.incomes.forEach((e, i) => {
    if (e.amount !== null && e.amount.trim() !== "" && !isValidAmount(e.amount))
      w.push(`Ingreso ${i + 1}: el monto «${e.amount}» no es válido.`);
  });
  if (p.checkin?.weight_kg) {
    const n = Number(p.checkin.weight_kg.replace(",", "."));
    if (!Number.isFinite(n) || n <= 0 || n >= 500) w.push(`Peso «${p.checkin.weight_kg}» fuera de rango.`);
  }
  return w;
}

/** Resumen legible de lo que creará la propuesta, para el diálogo de confirmación. */
export function describeProposal(p: Proposal): string[] {
  const out: string[] = [];
  const n = (k: number, one: string, many: string) => (k === 1 ? `1 ${one}` : `${k} ${many}`);
  if (p.expenses.length) out.push(n(p.expenses.length, "gasto", "gastos"));
  if (p.incomes.length) out.push(n(p.incomes.length, "ingreso", "ingresos"));
  if (p.tasks.length) out.push(n(p.tasks.length, "pendiente", "pendientes"));
  if (p.inbox.length) out.push(n(p.inbox.length, "elemento al Inbox", "elementos al Inbox"));
  if (p.log.length) out.push(n(p.log.length, "línea de log", "líneas de log"));
  const refl = p.reflections_good.length + p.reflections_bad.length;
  if (refl) out.push(n(refl, "reflexión", "reflexiones"));
  if (p.interactions.length) out.push(n(p.interactions.length, "interacción", "interacciones"));
  if (p.checkin) out.push("datos del check-in");
  if (p.goals.length) out.push(n(p.goals.length, "objetivo", "objetivos"));
  if (p.devlog) out.push("1 sesión de devlog");
  if (p.summary) out.push("resumen semanal");
  return out;
}

export function isProposalEmpty(p: Proposal): boolean {
  return describeProposal(p).length === 0;
}

/** `expenses[1].amount` → { section: "expenses", index: 1, field: "amount" } para resaltar el campo. */
export function parseQuestionField(field: string): {
  section: string;
  index: number | null;
  field: string | null;
} {
  const m = /^([a-z_]+)(?:\[(\d+)\])?(?:\.([a-z_]+))?$/.exec(field);
  if (!m) return { section: field, index: null, field: null };
  return { section: m[1] ?? field, index: m[2] !== undefined ? Number(m[2]) : null, field: m[3] ?? null };
}
