// Preparación de una carpeta del vault elegida en el navegador para `POST /imports/folder`.
// Se envía el texto de Markdown/Bases/Canvas; los binarios van solo como ruta (el servidor los registra como
// referencia). Nunca se lee ni se envía `.obsidian/` (plugins con código), `.git/` ni `.trash/`.

export interface FolderFile {
  path: string;
  content: string | null;
}

/** Extensiones cuyo texto se envía. El resto se registra como referencia. */
const TEXT_EXT = new Set(["md", "base", "canvas", "txt", "csv"]);
const SKIP_DIR = /(^|\/)(\.obsidian|\.git|\.trash|node_modules)(\/|$)/;
const SKIP_FILE = /(^|\/)(\.DS_Store|Thumbs\.db)$/;

export const MAX_FILES = 5000;
export const MAX_TEXT_BYTES = 40 * 1024 * 1024;

export function extension(path: string): string {
  const name = path.split("/").pop() ?? "";
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

export function shouldSkip(path: string): boolean {
  return SKIP_DIR.test(path) || SKIP_FILE.test(path);
}

export function isText(path: string): boolean {
  return TEXT_EXT.has(extension(path));
}

export interface FolderSelection {
  name: string;
  files: FolderFile[];
  skipped: number;
  bytes: number;
}

/** Lee los archivos seleccionados (`<input webkitdirectory>`). `relPath` permite probar sin `File` reales. */
export async function readFolder(
  files: { name: string; webkitRelativePath?: string; size: number; text: () => Promise<string> }[],
): Promise<FolderSelection> {
  const out: FolderFile[] = [];
  let skipped = 0;
  let bytes = 0;
  let root = "";
  for (const f of files) {
    const path = (f.webkitRelativePath || f.name).replaceAll("\\", "/");
    if (!root) root = path.split("/")[0] ?? "";
    if (shouldSkip(path)) {
      skipped++;
      continue;
    }
    if (out.length >= MAX_FILES)
      throw new Error(`La carpeta tiene más de ${MAX_FILES} archivos importables.`);
    if (isText(path)) {
      bytes += f.size;
      if (bytes > MAX_TEXT_BYTES)
        throw new Error("La carpeta excede 40 MB de texto. Importa un ZIP más pequeño.");
      out.push({ path, content: await f.text() });
    } else {
      out.push({ path, content: null });
    }
  }
  return { name: root || "carpeta", files: out, skipped, bytes };
}

/** Resumen numérico del dry-run (`run.summary`), tolerante a campos ausentes. */
export interface PlanSummary {
  total: number;
  create: number;
  link: number;
  unchanged: number;
  conflicts: number;
  references: number;
  ignored: number;
  warnings: number;
  byEntity: { entity: string; actions: Record<string, number> }[];
}

export function parseSummary(raw: unknown): PlanSummary {
  const s = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  // Tras aplicar, el servidor guarda { plan, result }.
  const plan = s.plan && typeof s.plan === "object" ? (s.plan as Record<string, unknown>) : s;
  const n = (k: string) => (typeof plan[k] === "number" ? (plan[k] as number) : 0);
  const by =
    plan.by_entity && typeof plan.by_entity === "object" ? (plan.by_entity as Record<string, unknown>) : {};
  return {
    total: n("total"),
    create: n("create"),
    link: n("link"),
    unchanged: n("unchanged"),
    conflicts: n("conflicts"),
    references: n("references"),
    ignored: n("ignored"),
    warnings: n("warnings"),
    byEntity: Object.entries(by)
      .map(([entity, v]) => ({
        entity,
        actions: Object.fromEntries(
          Object.entries(v && typeof v === "object" ? (v as Record<string, unknown>) : {}).filter(
            (e): e is [string, number] => typeof e[1] === "number",
          ),
        ),
      }))
      .sort((a, b) => a.entity.localeCompare(b.entity)),
  };
}

/** Resultado tras aplicar (`summary.result`). */
export function parseResult(
  raw: unknown,
): { created: number; linked: number; skipped: number; conflicts: number } | null {
  const r = (raw as { result?: Record<string, unknown> } | null)?.result;
  if (!r) return null;
  const n = (k: string) => (typeof r[k] === "number" ? (r[k] as number) : 0);
  return { created: n("created"), linked: n("linked"), skipped: n("skipped"), conflicts: n("conflicts") };
}

export const ENTITY_LABEL: Record<string, string> = {
  checkin: "Dailies",
  daily: "Dailies",
  goal: "Objetivos",
  weekly_review: "Revisiones semanales",
  person: "Personas",
  reading: "Lecturas",
  content: "Contenido",
  swipe: "Swipe",
  class_note: "Clases",
  devlog: "Devlog",
  devlog_session: "Sesiones de devlog",
  adr: "ADRs",
  transaction: "Movimientos",
  account: "Cuentas y tarjetas",
  recurring: "Recurrentes",
  recurring_rule: "Recurrentes",
  task: "Pendientes",
  recurring_task: "Pendientes recurrentes",
  inbox: "Inbox",
  project: "Proyectos",
  note: "Notas",
  budget: "Presupuestos",
  debt: "Deudas",
};

export const ACTION_LABEL: Record<string, string> = {
  create: "Crear",
  link: "Vincular/actualizar",
  unchanged: "Sin cambios",
  conflict: "Conflicto",
};
