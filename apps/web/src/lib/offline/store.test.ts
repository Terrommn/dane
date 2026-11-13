import { afterEach, describe, expect, it } from "vitest";
import { closeAll, deleteUserDb, openUserDb } from "./db";
import {
  draftId,
  enqueue,
  flushQueue,
  getDraft,
  listQueue,
  pendingCount,
  resolveConflict,
  type Sender,
  saveDraft,
} from "./store";

const A = "user-a";
const B = "user-b";

afterEach(async () => {
  await deleteUserDb(A);
  await deleteUserDb(B);
  await closeAll();
});

const ok =
  (version = 2): Sender =>
  async (item) => ({ status: 200, body: { ...(item.body as object), version } });

describe("cola offline", () => {
  it("un borrador local no se marca como guardado hasta que el servidor confirma", async () => {
    await saveDraft(A, "checkin", "2026-11-12", { energy: 4 }, 1);
    expect((await getDraft(A, "checkin", "2026-11-12"))?.status).toBe("draft");
    await enqueue(A, {
      method: "PUT",
      path: "/checkins/2026-11-12",
      body: { energy: 4, version: 1 },
      baseVersion: 1,
      draftId: draftId("checkin", "2026-11-12"),
    });
    expect((await getDraft(A, "checkin", "2026-11-12"))?.status).toBe("pending");

    const offline: Sender = async () => ({ status: 0, body: null });
    const s1 = await flushQueue(A, offline);
    expect(s1).toMatchObject({ sent: 0, remaining: 1, offline: true });
    expect((await getDraft(A, "checkin", "2026-11-12"))?.status).toBe("pending");

    const s2 = await flushQueue(A, ok(2));
    expect(s2).toMatchObject({ sent: 1, remaining: 0 });
    const d = await getDraft(A, "checkin", "2026-11-12");
    expect(d?.status).toBe("saved");
    expect(d?.baseVersion).toBe(2);
  });

  it("reintenta con la misma Idempotency-Key tras un fallo de red", async () => {
    const item = await enqueue(A, { method: "POST", path: "/inbox", body: { text: "llamar" } });
    const keys: string[] = [];
    const flaky: Sender = async (i) => {
      keys.push(i.idempotencyKey);
      return keys.length === 1 ? { status: 503, body: null } : { status: 201, body: { id: "x" } };
    };
    await flushQueue(A, flaky);
    await flushQueue(A, flaky);
    expect(keys).toEqual([item.id, item.id]);
    expect(await pendingCount(A)).toBe(0);
  });

  it("mantiene el orden y se detiene en el primer error transitorio", async () => {
    await enqueue(A, { method: "POST", path: "/inbox", body: { n: 1 } });
    await enqueue(A, { method: "POST", path: "/inbox", body: { n: 2 } });
    const seen: number[] = [];
    await flushQueue(A, async (i) => {
      seen.push((i.body as { n: number }).n);
      return { status: 429, body: null };
    });
    expect(seen).toEqual([1]);
    expect(await pendingCount(A)).toBe(2);
  });

  it("un 409 queda como conflicto explícito con el estado del servidor y no se reintenta solo", async () => {
    await saveDraft(A, "checkin", "2026-11-11", { energy: 2 }, 3);
    await enqueue(A, {
      method: "PUT",
      path: "/checkins/2026-11-11",
      body: { energy: 2, version: 3 },
      baseVersion: 3,
      draftId: draftId("checkin", "2026-11-11"),
    });
    const conflict: Sender = async () => ({
      status: 409,
      body: { error: { code: "conflict", message: "cambió", current: { energy: 5, version: 4 } } },
    });
    const s = await flushQueue(A, conflict);
    expect(s.conflicts).toBe(1);
    const [q] = await listQueue(A);
    expect(q?.status).toBe("conflict");
    expect(q?.current).toEqual({ energy: 5, version: 4 });
    expect((await getDraft(A, "checkin", "2026-11-11"))?.status).toBe("conflict");

    // Un flush posterior no lo envía de nuevo.
    let calls = 0;
    await flushQueue(A, async () => {
      calls++;
      return { status: 200, body: {} };
    });
    expect(calls).toBe(0);

    // "Conservar la mía": se reenvía sobre la versión actual del servidor con una clave nueva.
    await resolveConflict(A, q?.id ?? "", { choice: "mine" });
    const [retry] = await listQueue(A);
    expect(retry?.status).toBe("pending");
    expect(retry?.id).not.toBe(q?.id);
    expect((retry?.body as { version: number }).version).toBe(4);
    await flushQueue(A, ok(5));
    expect((await getDraft(A, "checkin", "2026-11-11"))?.status).toBe("saved");
  });

  it("resolver con la versión del servidor descarta el cambio local", async () => {
    await saveDraft(A, "inbox", "c1", { text: "x" }, 1);
    await enqueue(A, {
      method: "PUT",
      path: "/inbox/1",
      body: { text: "x", version: 1 },
      baseVersion: 1,
      draftId: draftId("inbox", "c1"),
    });
    await flushQueue(A, async () => ({
      status: 409,
      body: { error: { code: "conflict", message: "", current: { text: "y", version: 2 } } },
    }));
    const [q] = await listQueue(A);
    await resolveConflict(A, q?.id ?? "", { choice: "theirs" });
    expect(await listQueue(A)).toEqual([]);
    expect(await getDraft(A, "inbox", "c1")).toBeUndefined();
  });

  it("errores de validación quedan como error y no bloquean el resto", async () => {
    await enqueue(A, { method: "POST", path: "/inbox", body: { text: "" } });
    await enqueue(A, { method: "POST", path: "/inbox", body: { text: "ok" } });
    const s = await flushQueue(A, async (i) =>
      (i.body as { text: string }).text
        ? { status: 201, body: {} }
        : { status: 422, body: { error: { code: "validation", message: "Texto obligatorio" } } },
    );
    expect(s).toMatchObject({ sent: 1, errors: 1, remaining: 1 });
    expect((await listQueue(A))[0]?.lastError).toBe("Texto obligatorio");
  });

  it("los datos locales están particionados por usuario y se borran al cerrar sesión", async () => {
    await saveDraft(A, "checkin", "2026-11-12", { energy: 1 }, null);
    await enqueue(A, { method: "POST", path: "/inbox", body: { text: "de A" } });
    expect(await getDraft(B, "checkin", "2026-11-12")).toBeUndefined();
    expect(await listQueue(B)).toEqual([]);
    await deleteUserDb(A);
    expect(await getDraft(A, "checkin", "2026-11-12")).toBeUndefined();
    expect(await pendingCount(A)).toBe(0);
  });

  it("requiere usuario para abrir datos locales", () => {
    expect(() => openUserDb("")).toThrow();
  });
});
