import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import type { Proposal } from "./proposal";

export type CaptureView = Schemas["CaptureView"];

export const captureKey = (id: string) => ["capture", id] as const;

/** GET extrae la propuesta (si hay IA) la primera vez que se abre una captura del bot. */
export function useCapture(id: string) {
  return useQuery({
    queryKey: captureKey(id),
    queryFn: async () => unwrap(await api.GET("/captures/{id}", { params: { path: { id } } })),
  });
}

export function useCaptures(status?: string) {
  return useQuery({
    queryKey: ["captures", status ?? "all"],
    queryFn: async () => unwrap(await api.GET("/captures", { params: { query: status ? { status } : {} } })),
  });
}

export function useCaptureMutations(id?: string) {
  const qc = useQueryClient();
  const set = (v: CaptureView) => {
    qc.setQueryData(captureKey(v.id), v);
    void qc.invalidateQueries({ queryKey: ["captures"] });
  };
  const create = useMutation({
    mutationFn: async (body: Schemas["NewCapture"]) => unwrap(await api.POST("/captures", { body })),
    onSuccess: set,
  });
  const update = useMutation({
    mutationFn: async ({ proposal, version }: { proposal: Proposal; version: number }) =>
      unwrap(
        await api.PUT("/captures/{id}", {
          params: { path: { id: id ?? "" } },
          body: { version, proposal: proposal as unknown as Record<string, never> },
        }),
      ),
    onSuccess: set,
  });
  const apply = useMutation({
    mutationFn: async (version: number) =>
      unwrap(
        await api.POST("/captures/{id}/apply", {
          params: { path: { id: id ?? "" } },
          // El servidor espera `{ version }`. El OpenAPI generado lo tipa mal por un choque de nombres de esquema
          // (`ApplyBody` de capturas e Inbox); se fuerza el tipo hasta que se renombre en el server.
          body: { version } as unknown as Schemas["CaptureApplyBody"],
        }),
      ),
    onSuccess: (v) => {
      set(v);
      // Lo aplicado aparece en otros módulos (gastos, tareas, check-in…).
      void qc.invalidateQueries();
    },
  });
  const reject = useMutation({
    mutationFn: async () =>
      unwrap(await api.POST("/captures/{id}/reject", { params: { path: { id: id ?? "" } } })),
    onSuccess: set,
  });
  const suggest = useMutation({
    mutationFn: async (body: Schemas["SuggestBody"]) => unwrap(await api.POST("/captures/suggest", { body })),
    onSuccess: set,
  });
  return { create, update, apply, reject, suggest };
}

/** Catálogos para autocompletar la edición manual (nombres, como los usa el validador del servidor). */
export function useCaptureCatalogs(enabled: boolean) {
  const categories = useQuery({
    queryKey: ["finance", "categories"],
    enabled,
    queryFn: async () => unwrap(await api.GET("/finance/categories")),
  });
  const accounts = useQuery({
    queryKey: ["finance", "accounts"],
    enabled,
    queryFn: async () => unwrap(await api.GET("/finance/accounts")),
  });
  const areas = useQuery({
    queryKey: ["areas", { includeArchived: false }],
    enabled,
    queryFn: async () => unwrap(await api.GET("/areas", { params: { query: { archived: false } } })),
  });
  const people = useQuery({
    queryKey: ["people", "names"],
    enabled,
    queryFn: async () => unwrap(await api.GET("/people")),
  });
  return {
    expenseCategories: (categories.data ?? [])
      .filter((c) => c.kind === "expense" && !c.archived_at)
      .map((c) => c.name),
    incomeCategories: (categories.data ?? [])
      .filter((c) => c.kind === "income" && !c.archived_at)
      .map((c) => c.name),
    accounts: (accounts.data ?? []).map((a) => a.account.name),
    areas: (areas.data ?? []).map((a) => a.name),
    people: (people.data ?? []).map((p) => p.name),
  };
}

export function useGoalsForSuggest(enabled: boolean) {
  return useQuery({
    queryKey: ["goals", "for-suggest"],
    enabled,
    queryFn: async () => unwrap(await api.GET("/goals")),
  });
}
