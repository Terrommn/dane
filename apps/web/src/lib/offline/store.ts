// Borradores y cola de sincronización persistentes. Sincroniza al abrir y al volver la conexión; no depende de
// Background Sync. Un borrador nunca se marca `saved` hasta que el servidor responde 2xx.
import { type DraftKind, type DraftRecord, type DraftStatus, openUserDb, type QueueItem } from "./db";

type Listener = () => void;
const listeners = new Set<Listener>();
export function subscribeOffline(fn: Listener) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
function emit() {
  for (const l of listeners) l();
}

/** Marca de orden estrictamente creciente: dos operaciones en el mismo milisegundo conservan su orden. */
let lastTs = 0;
export function nextTimestamp(): number {
  lastTs = Math.max(Date.now(), lastTs + 1);
  return lastTs;
}

export const draftId = (kind: DraftKind, key: string) => `${kind}:${key}`;

export async function getDraft<T>(
  userId: string,
  kind: DraftKind,
  key: string,
): Promise<DraftRecord<T> | undefined> {
  const db = await openUserDb(userId);
  return (await db.get("drafts", draftId(kind, key))) as DraftRecord<T> | undefined;
}

/** Guarda el borrador localmente (estado `draft`: solo en este dispositivo). */
export async function saveDraft<T>(
  userId: string,
  kind: DraftKind,
  key: string,
  data: T,
  baseVersion: number | null,
) {
  const db = await openUserDb(userId);
  const prev = await db.get("drafts", draftId(kind, key));
  const rec: DraftRecord<T> = {
    id: draftId(kind, key),
    kind,
    key,
    data,
    baseVersion: prev?.status === "conflict" ? prev.baseVersion : (baseVersion ?? prev?.baseVersion ?? null),
    status: prev?.status === "conflict" ? "conflict" : "draft",
    updatedAt: Date.now(),
    queueId: prev?.queueId ?? null,
    error: null,
  };
  await db.put("drafts", rec);
  emit();
  return rec;
}

export async function setDraftStatus(
  userId: string,
  id: string,
  status: DraftStatus,
  patch: Partial<DraftRecord> = {},
) {
  const db = await openUserDb(userId);
  const prev = await db.get("drafts", id);
  if (!prev) return;
  await db.put("drafts", { ...prev, ...patch, status });
  emit();
}

export async function deleteDraft(userId: string, kind: DraftKind, key: string) {
  const db = await openUserDb(userId);
  await db.delete("drafts", draftId(kind, key));
  emit();
}

export interface EnqueueInput {
  method: QueueItem["method"];
  path: string;
  body: unknown;
  baseVersion?: number | null;
  draftId?: string | null;
  /** Reusar un ID existente (p. ej. reintento) para conservar la Idempotency-Key. */
  id?: string;
}

/** Encola una mutación. Si el borrador ya tenía una operación pendiente se reemplaza (último estado gana localmente). */
export async function enqueue(userId: string, input: EnqueueInput): Promise<QueueItem> {
  const db = await openUserDb(userId);
  const tx = db.transaction(["queue", "drafts"], "readwrite");
  if (input.draftId) {
    const d = await tx.objectStore("drafts").get(input.draftId);
    if (d?.queueId && d.queueId !== input.id) await tx.objectStore("queue").delete(d.queueId);
  }
  const id = input.id ?? crypto.randomUUID();
  const item: QueueItem = {
    id,
    method: input.method,
    path: input.path,
    body: input.body,
    idempotencyKey: id,
    baseVersion: input.baseVersion ?? null,
    draftId: input.draftId ?? null,
    createdAt: nextTimestamp(),
    attempts: 0,
    status: "pending",
    lastError: null,
    current: null,
  };
  await tx.objectStore("queue").put(item);
  if (input.draftId) {
    const d = await tx.objectStore("drafts").get(input.draftId);
    if (d) await tx.objectStore("drafts").put({ ...d, status: "pending", queueId: id, error: null });
  }
  await tx.done;
  emit();
  return item;
}

export async function listQueue(userId: string): Promise<QueueItem[]> {
  const db = await openUserDb(userId);
  return db.getAllFromIndex("queue", "byCreated");
}

export async function pendingCount(userId: string): Promise<number> {
  return (await listQueue(userId)).length;
}

export interface SendResult {
  status: number;
  body: unknown;
}
/** Transporte inyectable: en la app usa `apiFetch`; en pruebas, un doble. Status 0 = sin red. */
export type Sender = (item: QueueItem) => Promise<SendResult>;

export interface FlushSummary {
  sent: number;
  conflicts: number;
  errors: number;
  remaining: number;
  offline: boolean;
}

let flushing: Promise<FlushSummary> | null = null;

/**
 * Envía la cola en orden. 2xx → elimina y marca el borrador `saved` con la versión nueva; 409 conflict → `conflict`
 * con el estado del servidor (no reintenta); 4xx de validación → `error` (requiere acción); red/5xx/429/401 →
 * se detiene y conserva el resto para el siguiente intento.
 */
export function flushQueue(userId: string, send: Sender): Promise<FlushSummary> {
  if (flushing) return flushing;
  flushing = doFlush(userId, send).finally(() => {
    flushing = null;
  });
  return flushing;
}

async function doFlush(userId: string, send: Sender): Promise<FlushSummary> {
  const summary: FlushSummary = { sent: 0, conflicts: 0, errors: 0, remaining: 0, offline: false };
  const db = await openUserDb(userId);
  const items = await db.getAllFromIndex("queue", "byCreated");
  for (const item of items) {
    if (item.status !== "pending") continue;
    let res: SendResult;
    try {
      res = await send(item);
    } catch {
      res = { status: 0, body: null };
    }
    const attempts = item.attempts + 1;
    if (res.status >= 200 && res.status < 300) {
      await db.delete("queue", item.id);
      if (item.draftId) {
        const d = await db.get("drafts", item.draftId);
        // Solo si el borrador no cambió a otra operación mientras se enviaba.
        if (d && d.queueId === item.id) {
          const version = (res.body as { version?: number } | null)?.version ?? d.baseVersion;
          await db.put("drafts", {
            ...d,
            status: "saved",
            queueId: null,
            baseVersion: version ?? null,
            error: null,
          });
        }
      }
      summary.sent++;
      emit();
      continue;
    }
    const code = (res.body as { error?: { code?: string; message?: string; current?: unknown } } | null)
      ?.error;
    if (res.status === 409 && code?.code === "conflict") {
      await db.put("queue", {
        ...item,
        attempts,
        status: "conflict",
        current: code.current ?? null,
        lastError: code.message ?? null,
      });
      if (item.draftId) await markDraft(db, item.draftId, "conflict", code.message ?? "Conflicto");
      summary.conflicts++;
      emit();
      continue;
    }
    if (res.status >= 400 && res.status < 500 && ![401, 408, 409, 429].includes(res.status)) {
      await db.put("queue", {
        ...item,
        attempts,
        status: "error",
        lastError: code?.message ?? `Error ${res.status}`,
      });
      if (item.draftId) await markDraft(db, item.draftId, "error", code?.message ?? `Error ${res.status}`);
      summary.errors++;
      emit();
      continue;
    }
    // Sin red, servidor caído, rate limit, sesión vencida u operación en curso: reintentar más tarde, en orden.
    await db.put("queue", {
      ...item,
      attempts,
      lastError: res.status === 0 ? "Sin conexión" : `Error ${res.status}`,
    });
    summary.offline = res.status === 0;
    break;
  }
  summary.remaining = (await db.getAll("queue")).length;
  emit();
  return summary;
}

async function markDraft(
  db: Awaited<ReturnType<typeof openUserDb>>,
  id: string,
  status: DraftStatus,
  error: string,
) {
  const d = await db.get("drafts", id);
  if (d) await db.put("drafts", { ...d, status, error });
}

export type Resolution = { choice: "mine" } | { choice: "theirs" } | { choice: "merged"; body: unknown };

/**
 * Resolución explícita de un conflicto:
 * - `mine`: reintenta con mis datos sobre la versión actual del servidor;
 * - `theirs`: descarta mis cambios locales y adopta el servidor;
 * - `merged`: envía la mezcla elegida sobre la versión actual.
 */
export async function resolveConflict(userId: string, queueId: string, r: Resolution) {
  const db = await openUserDb(userId);
  const item = await db.get("queue", queueId);
  if (item?.status !== "conflict") return;
  const serverVersion = (item.current as { version?: number } | null)?.version ?? null;
  if (r.choice === "theirs") {
    await db.delete("queue", queueId);
    if (item.draftId) await db.delete("drafts", item.draftId);
    emit();
    return;
  }
  const baseBody = r.choice === "merged" ? r.body : item.body;
  const body =
    baseBody && typeof baseBody === "object" && serverVersion !== null
      ? { ...(baseBody as object), version: serverVersion }
      : baseBody;
  // Nueva operación (nueva Idempotency-Key): el cuerpo cambió.
  await db.delete("queue", queueId);
  const newId = crypto.randomUUID();
  await db.put("queue", {
    ...item,
    id: newId,
    idempotencyKey: newId,
    body,
    baseVersion: serverVersion,
    status: "pending",
    attempts: 0,
    current: null,
    lastError: null,
    createdAt: nextTimestamp(),
  });
  if (item.draftId) {
    const d = await db.get("drafts", item.draftId);
    if (d)
      await db.put("drafts", {
        ...d,
        data: r.choice === "merged" ? r.body : d.data,
        status: "pending",
        queueId: newId,
        baseVersion: serverVersion,
        error: null,
      });
  }
  emit();
}

/** Descarta una operación con error de validación (el borrador queda para corregirlo). */
export async function dropQueueItem(userId: string, queueId: string) {
  const db = await openUserDb(userId);
  const item = await db.get("queue", queueId);
  await db.delete("queue", queueId);
  if (item?.draftId) await markDraft(db, item.draftId, "draft", "");
  emit();
}
