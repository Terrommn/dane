import { describe, expect, it } from "vitest";
import { applyAuthHeaders } from "./api";
import { toApiError } from "./errors";
import { readCookie } from "./session";

const cookie = { mode: "cookie" as const, token: null, userId: "u" };
const bearer = { mode: "bearer" as const, token: "tok-123", userId: "u" };

describe("cabeceras de autenticación", () => {
  it("Mini App: Bearer en memoria, sin CSRF", () => {
    const h = applyAuthHeaders("POST", new Headers(), bearer, "vida_csrf=abc");
    expect(h.get("Authorization")).toBe("Bearer tok-123");
    expect(h.get("X-CSRF-Token")).toBeNull();
  });

  it("web: CSRF desde la cookie solo en mutaciones y nunca Authorization", () => {
    const post = applyAuthHeaders("POST", new Headers(), cookie, "other=1; vida_csrf=abc%3D");
    expect(post.get("X-CSRF-Token")).toBe("abc=");
    expect(post.get("Authorization")).toBeNull();
    const get = applyAuthHeaders("GET", new Headers(), cookie, "vida_csrf=abc");
    expect(get.get("X-CSRF-Token")).toBeNull();
  });

  it("agrega Idempotency-Key en mutaciones y respeta la del llamador", () => {
    const put = applyAuthHeaders("PUT", new Headers(), cookie, "");
    expect(put.get("Idempotency-Key")).toMatch(/^[0-9a-f-]{36}$/);
    const own = applyAuthHeaders("POST", new Headers({ "Idempotency-Key": "mi-clave-123" }), cookie, "");
    expect(own.get("Idempotency-Key")).toBe("mi-clave-123");
    expect(applyAuthHeaders("GET", new Headers(), cookie, "").get("Idempotency-Key")).toBeNull();
    const a = applyAuthHeaders("DELETE", new Headers(), cookie, "").get("Idempotency-Key");
    const b = applyAuthHeaders("DELETE", new Headers(), cookie, "").get("Idempotency-Key");
    expect(a).not.toBe(b);
  });

  it("sin sesión no envía credenciales", () => {
    const h = applyAuthHeaders("POST", new Headers(), { mode: null, token: null, userId: null }, "");
    expect(h.get("Authorization")).toBeNull();
  });
});

describe("errores de la API", () => {
  it("mapea ErrorBody y conserva current en conflictos", () => {
    const e = toApiError(409, { error: { code: "conflict", message: "Cambió", current: { version: 4 } } });
    expect(e.isConflict).toBe(true);
    expect(e.current).toEqual({ version: 4 });
    expect(e.message).toBe("Cambió");
    expect(toApiError(500, null).message).toMatch(/servidor/);
    expect(toApiError(0, null).isNetwork).toBe(true);
    expect(toApiError(422, { error: { code: "validation", message: "x", field: "name" } }).field).toBe(
      "name",
    );
  });

  it("lee cookies con valores codificados", () => {
    expect(readCookie("a", "a=1; b=2")).toBe("1");
    expect(readCookie("z", "a=1")).toBeNull();
  });
});
