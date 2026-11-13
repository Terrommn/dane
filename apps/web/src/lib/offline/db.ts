// IndexedDB particionado por usuario: una base por user_id (`vida-offline-<userId>`), así los borradores y la cola de
// una cuenta nunca son visibles para otra y cerrar sesión borra la base completa.
import { type DBSchema, deleteDB, type IDBPDatabase, openDB } from "idb";

export type DraftKind = "checkin" | "inbox";

/** Estado visible del guardado. `saved` solo cuando el servidor confirmó. */
export type DraftStatus = "draft" | "pending" | "saved" | "conflict" | "error";

export interface DraftRecord<T = unknown> {
  /** `${kind}:${key}` */
  id: string;
  kind: DraftKind;
  key: string;
  data: T;
  /** Versión del servidor sobre la que se editó (control optimista). null = recurso nuevo. */
  baseVersion: number | null;
  status: DraftStatus;
  updatedAt: number;
  /** Cola asociada mientras está pendiente. */
  queueId: string | null;
  error: string | null;
}

export type QueueStatus = "pending" | "conflict" | "error";

export interface QueueItem {
  /** ID de cliente estable (también se usa como Idempotency-Key). */
  id: string;
  method: "POST" | "PUT" | "PATCH" | "DELETE";
  path: string;
  body: unknown;
  idempotencyKey: string;
  baseVersion: number | null;
  draftId: string | null;
  createdAt: number;
  attempts: number;
  status: QueueStatus;
  lastError: string | null;
  /** En conflicto: estado actual del servidor. */
  current: unknown;
}

/** Última respuesta conocida de una vista privada (p. ej. el día) para abrirla sin red. Solo en la base del usuario. */
export interface SnapshotRecord {
  id: string;
  data: unknown;
  savedAt: number;
}

interface OfflineSchema extends DBSchema {
  drafts: { key: string; value: DraftRecord };
  queue: { key: string; value: QueueItem; indexes: { byCreated: number } };
  snapshots: { key: string; value: SnapshotRecord };
}

const DB_PREFIX = "vida-offline-";
const cache = new Map<string, Promise<IDBPDatabase<OfflineSchema>>>();

export function dbName(userId: string) {
  return `${DB_PREFIX}${userId}`;
}

export function openUserDb(userId: string): Promise<IDBPDatabase<OfflineSchema>> {
  if (!userId) throw new Error("Se requiere usuario para los datos locales");
  let p = cache.get(userId);
  if (!p) {
    p = openDB<OfflineSchema>(dbName(userId), 2, {
      upgrade(db, oldVersion) {
        if (oldVersion < 1) {
          db.createObjectStore("drafts", { keyPath: "id" });
          const q = db.createObjectStore("queue", { keyPath: "id" });
          q.createIndex("byCreated", "createdAt");
        }
        if (oldVersion < 2) db.createObjectStore("snapshots", { keyPath: "id" });
      },
    });
    cache.set(userId, p);
  }
  return p;
}

/** Borra todos los datos locales de un usuario (cerrar sesión / cambio de cuenta). */
export async function deleteUserDb(userId: string) {
  const p = cache.get(userId);
  if (p) {
    (await p).close();
    cache.delete(userId);
  }
  await deleteDB(dbName(userId));
}

/** Cierra conexiones (pruebas). */
export async function closeAll() {
  for (const p of cache.values()) (await p).close();
  cache.clear();
}

export async function saveSnapshot(userId: string, id: string, data: unknown) {
  await (await openUserDb(userId)).put("snapshots", { id, data, savedAt: Date.now() });
}

export async function loadSnapshot<T>(
  userId: string,
  id: string,
): Promise<{ data: T; savedAt: number } | null> {
  const r = await (await openUserDb(userId)).get("snapshots", id);
  return r ? { data: r.data as T, savedAt: r.savedAt } : null;
}
