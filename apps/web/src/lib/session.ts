// Estado de sesión en memoria. NUNCA se persiste un token: la Mini App guarda el Bearer solo aquí (D-005) y
// la web/PWA usa cookie HttpOnly que JS no puede leer (D-006).

export type SessionMode = "bearer" | "cookie";

export interface SessionState {
  mode: SessionMode | null;
  /** Solo en modo bearer (Mini App). */
  token: string | null;
  userId: string | null;
}

let state: SessionState = { mode: null, token: null, userId: null };
const listeners = new Set<() => void>();

export function getSession(): SessionState {
  return state;
}

export function setSession(next: SessionState) {
  state = next;
  for (const l of listeners) l();
}

export function clearSession() {
  setSession({ mode: null, token: null, userId: null });
}

export function subscribeSession(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Lee una cookie no HttpOnly (solo `dane_csrf`). */
export function readCookie(
  name: string,
  cookieString: string = typeof document === "undefined" ? "" : document.cookie,
) {
  for (const part of cookieString.split(";")) {
    const [k, ...v] = part.trim().split("=");
    if (k === name) return decodeURIComponent(v.join("="));
  }
  return null;
}

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: (() => void) | null) {
  onUnauthorized = fn;
}
export function notifyUnauthorized() {
  onUnauthorized?.();
}
