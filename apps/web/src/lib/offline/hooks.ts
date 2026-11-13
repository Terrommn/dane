// API de React para borradores y cola offline (la usan Hoy e Inbox).
//
//   const { draft, status, save, submit, clear } = useDraft<CheckinForm>("checkin", date);
//   save(data, version)            → guarda en el dispositivo (status "draft")
//   submit("PUT", `/checkins/${date}`, body, version) → encola y sincroniza (status "pending" → "saved" | "conflict" | "error")
//   const q = useSyncQueue();      → { items, pending, conflicts, errors, flush, resolve, drop, online }
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useAuth } from "../../features/auth/AuthProvider";
import { apiFetch } from "../api";
import type { DraftKind, DraftRecord, DraftStatus, QueueItem } from "./db";
import {
  deleteDraft,
  draftId,
  dropQueueItem,
  enqueue,
  flushQueue,
  getDraft,
  listQueue,
  type Resolution,
  resolveConflict,
  type Sender,
  saveDraft,
  subscribeOffline,
} from "./store";

/** Transporte real: misma autenticación e Idempotency-Key estable por operación. */
export const defaultSender: Sender = async (item: QueueItem) => {
  try {
    const res = await apiFetch(item.path, {
      method: item.method,
      body: item.body === undefined ? undefined : JSON.stringify(item.body),
      idempotencyKey: item.idempotencyKey,
    });
    const text = await res.text();
    let body: unknown = null;
    try {
      body = text ? JSON.parse(text) : null;
    } catch {
      body = null;
    }
    return { status: res.status, body };
  } catch {
    return { status: 0, body: null };
  }
};

function useOfflineVersion() {
  return useSyncExternalStore(
    (cb) => subscribeOffline(cb),
    () => offlineTick,
    () => 0,
  );
}
let offlineTick = 0;
subscribeOffline(() => {
  offlineTick++;
});

export function useOnline() {
  return useSyncExternalStore(
    (cb) => {
      window.addEventListener("online", cb);
      window.addEventListener("offline", cb);
      return () => {
        window.removeEventListener("online", cb);
        window.removeEventListener("offline", cb);
      };
    },
    () => navigator.onLine,
    () => true,
  );
}

export function useSyncQueue(sender: Sender = defaultSender) {
  const { userId } = useAuth();
  const tick = useOfflineVersion();
  const online = useOnline();
  const [items, setItems] = useState<QueueItem[]>([]);
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    let alive = true;
    void tick;
    if (userId)
      listQueue(userId).then((q) => {
        if (alive) setItems(q);
      });
    return () => {
      alive = false;
    };
  }, [userId, tick]);

  const flush = useCallback(async () => {
    if (!userId) return null;
    setSyncing(true);
    try {
      return await flushQueue(userId, sender);
    } finally {
      setSyncing(false);
    }
  }, [userId, sender]);

  // Sincroniza al abrir, al reconectar y al volver a primer plano (sin depender de Background Sync).
  useEffect(() => {
    if (!userId) return;
    void flush();
    const onVisible = () => document.visibilityState === "visible" && void flush();
    window.addEventListener("online", flush);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("online", flush);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [userId, flush]);

  return {
    items,
    pending: items.filter((i) => i.status === "pending").length,
    conflicts: items.filter((i) => i.status === "conflict"),
    errors: items.filter((i) => i.status === "error"),
    syncing,
    online,
    flush,
    resolve: (id: string, r: Resolution) =>
      userId ? resolveConflict(userId, id, r).then(flush) : Promise.resolve(null),
    drop: (id: string) => (userId ? dropQueueItem(userId, id) : Promise.resolve()),
  };
}

export function useDraft<T>(kind: DraftKind, key: string, sender: Sender = defaultSender) {
  const { userId } = useAuth();
  const tick = useOfflineVersion();
  const [draft, setDraft] = useState<DraftRecord<T> | undefined>(undefined);
  const [loaded, setLoaded] = useState(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let alive = true;
    void tick;
    if (!userId) return;
    getDraft<T>(userId, kind, key).then((d) => {
      if (alive) {
        setDraft(d);
        setLoaded(true);
      }
    });
    return () => {
      alive = false;
    };
  }, [userId, kind, key, tick]);

  const save = useCallback(
    (data: T, baseVersion: number | null) => {
      if (!userId) return Promise.resolve();
      return saveDraft(userId, kind, key, data, baseVersion).then(() => undefined);
    },
    [userId, kind, key],
  );

  /** Guardado con retardo para formularios (cada pulsación no escribe en IndexedDB). */
  const saveDebounced = useCallback(
    (data: T, baseVersion: number | null, ms = 400) => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => void save(data, baseVersion), ms);
    },
    [save],
  );

  const submit = useCallback(
    async (method: QueueItem["method"], path: string, body: unknown, baseVersion: number | null) => {
      if (!userId) return null;
      if (saveTimer.current) clearTimeout(saveTimer.current);
      await saveDraft(userId, kind, key, body as T, baseVersion);
      await enqueue(userId, { method, path, body, baseVersion, draftId: draftId(kind, key) });
      return flushQueue(userId, sender);
    },
    [userId, kind, key, sender],
  );

  const clear = useCallback(
    () => (userId ? deleteDraft(userId, kind, key) : Promise.resolve()),
    [userId, kind, key],
  );

  const status: DraftStatus | null = draft?.status ?? null;
  return { draft, loaded, status, save, saveDebounced, submit, clear };
}
