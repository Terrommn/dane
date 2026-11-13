// Cliente de la API sobre @vida/api-client (tipos generados del OpenAPI del servidor).
import { createApiClient, type Schemas } from "@vida/api-client";
import { getSession, notifyUnauthorized, readCookie, type SessionState } from "./session";

export type { Schemas };

const MUTATIONS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

export function newIdempotencyKey(): string {
  return crypto.randomUUID();
}

/**
 * Agrega credenciales y cabeceras de seguridad a una petición:
 * - Bearer en memoria (Mini App) o cookie + `X-CSRF-Token` (web/PWA) en mutaciones;
 * - `Idempotency-Key` en mutaciones si el llamador no puso una (para reintentos seguros).
 * Función pura para poder probarla.
 */
export function applyAuthHeaders(
  method: string,
  headers: Headers,
  session: SessionState,
  cookieString?: string,
) {
  const m = method.toUpperCase();
  if (session.mode === "bearer" && session.token) {
    headers.set("Authorization", `Bearer ${session.token}`);
  }
  if (MUTATIONS.has(m)) {
    if (session.mode !== "bearer") {
      const csrf = readCookie("vida_csrf", cookieString);
      if (csrf) headers.set("X-CSRF-Token", csrf);
    }
    if (!headers.has("Idempotency-Key")) headers.set("Idempotency-Key", newIdempotencyKey());
  }
  return headers;
}

export const api = createApiClient({
  middleware: [
    {
      onRequest({ request }) {
        applyAuthHeaders(request.method, request.headers, getSession());
        return request;
      },
      onResponse({ response, request }) {
        if (response.status === 401 && !new URL(request.url, location.origin).pathname.includes("/auth/")) {
          notifyUnauthorized();
        }
        return response;
      },
    },
  ],
});

/** fetch con las mismas reglas (para la cola offline y rutas sin tipar). */
export async function apiFetch(
  path: string,
  init: RequestInit & { idempotencyKey?: string } = {},
): Promise<Response> {
  const headers = new Headers(init.headers);
  if (init.idempotencyKey) headers.set("Idempotency-Key", init.idempotencyKey);
  if (init.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  applyAuthHeaders(init.method ?? "GET", headers, getSession());
  return fetch(`/api/v1${path}`, { ...init, headers, credentials: "include" });
}
