// Errores uniformes a partir de `ErrorBody` del servidor: { error: { code, message, field?, current? } }.

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly field: string | null;
  /** En 409 `conflict`: estado actual del recurso en el servidor. */
  readonly current: unknown;

  constructor(
    status: number,
    code: string,
    message: string,
    field: string | null = null,
    current: unknown = null,
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.field = field;
    this.current = current;
  }

  get isConflict() {
    return this.status === 409 && this.code === "conflict";
  }
  get isNetwork() {
    return this.status === 0;
  }
}

interface ErrorBodyLike {
  error?: { code?: string; message?: string; field?: string | null; current?: unknown };
}

export function toApiError(status: number, body: unknown): ApiError {
  const e = (body as ErrorBodyLike | null)?.error;
  if (e?.code) return new ApiError(status, e.code, e.message ?? "Error", e.field ?? null, e.current ?? null);
  if (status === 0) return new ApiError(0, "network", "Sin conexión con el servidor");
  if (status >= 500) return new ApiError(status, "internal", "Error del servidor; intenta de nuevo");
  return new ApiError(status, "unknown", `Error ${status}`);
}

/** Mensaje legible para mostrar en la UI. */
export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) return e.message;
  if (e instanceof TypeError) return "Sin conexión con el servidor";
  if (e instanceof Error) return e.message;
  return "Ocurrió un error inesperado";
}

/** Desenvuelve un resultado de openapi-fetch y lanza `ApiError` si falló. */
export function unwrap<T>(r: { data?: T; error?: unknown; response: Response }): Exclude<T, undefined> {
  if (r.error !== undefined || !r.response.ok) throw toApiError(r.response.status, r.error);
  return r.data as Exclude<T, undefined>;
}

/** Falla de red (sin conexión o server inalcanzable), sea un TypeError de fetch o un ApiError de estado 0. */
export function isNetworkError(e: unknown): boolean {
  return e instanceof TypeError || (e instanceof ApiError && e.isNetwork);
}
