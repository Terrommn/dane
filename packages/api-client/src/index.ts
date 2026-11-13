// Cliente tipado generado desde el contrato OpenAPI del servidor (apps/server → openapi.json).
// No editar schema.d.ts a mano: `pnpm gen:api`.
import createClient, { type Middleware } from "openapi-fetch";
import type { components, paths } from "./schema";

export type { components, paths };
export type Schemas = components["schemas"];

export function createApiClient(opts: { baseUrl?: string; middleware?: Middleware[] } = {}) {
  const client = createClient<paths>({ baseUrl: opts.baseUrl ?? "/api/v1", credentials: "include" });
  for (const m of opts.middleware ?? []) client.use(m);
  return client;
}

export type ApiClient = ReturnType<typeof createApiClient>;
