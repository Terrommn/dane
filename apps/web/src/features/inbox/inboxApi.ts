import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type InboxItem = Schemas["InboxItem"];
export type InboxAction = Schemas["InboxAction"];

export const INBOX_STATUS: Record<string, string> = {
  pending: "Pendientes",
  processed: "Procesados",
  discarded: "Descartados",
  archived: "Archivados",
  all: "Todos",
};

/** Propuesta determinista del servidor (`POST /inbox/{id}/propose`) o editada por el usuario. */
export interface Proposal {
  destination?: "task" | "note" | "person" | "discard" | string;
  title?: string;
  area_slug?: string | null;
  area_id?: string | null;
  reason?: string;
}

/** Convierte una propuesta (o nada) en la acción inicial que el usuario revisa antes de aplicar. */
export function actionFromProposal(text: string, p: Proposal | null | undefined): InboxAction {
  const title = (p?.title ?? text).trim();
  switch (p?.destination) {
    case "note":
      return { type: "note", title, body_md: text, tags: [], area_id: p.area_id ?? null, project_id: null };
    case "person":
      return { type: "person", name: title, aliases: [], area_id: p.area_id ?? null, context_md: text };
    case "discard":
      return { type: "discard" };
    default:
      return {
        type: "task",
        title,
        area_id: p?.area_id ?? null,
        project_id: null,
        planned_on: null,
        due_on: null,
        priority: null,
        notes_md: "",
      };
  }
}

export const ACTION_LABEL: Record<InboxAction["type"], string> = {
  task: "Convertir en tarea",
  note: "Convertir en nota",
  person: "Crear persona",
  discard: "Descartar",
  archive: "Archivar",
};

export function useInbox(status: string) {
  return useQuery({
    queryKey: ["inbox", status],
    queryFn: async () => unwrap(await api.GET("/inbox", { params: { query: { status } } })),
  });
}

export function useInboxMutations() {
  const qc = useQueryClient();
  const refresh = () => qc.invalidateQueries({ queryKey: ["inbox"] });
  const propose = useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.POST("/inbox/{id}/propose", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const edit = useMutation({
    mutationFn: async ({ item, text }: { item: InboxItem; text: string }) =>
      unwrap(
        await api.PUT("/inbox/{id}", {
          params: { path: { id: item.id } },
          body: { text, version: item.version },
        }),
      ),
    onSuccess: refresh,
  });
  const apply = useMutation({
    mutationFn: async ({ item, action }: { item: InboxItem; action: InboxAction }) =>
      unwrap(
        await api.POST("/inbox/{id}/apply", {
          params: { path: { id: item.id } },
          body: { version: item.version, action },
        }),
      ),
    onSuccess: () => {
      void refresh();
      void qc.invalidateQueries({ queryKey: ["tasks"] });
      void qc.invalidateQueries({ queryKey: ["notes"] });
      void qc.invalidateQueries({ queryKey: ["people"] });
    },
  });
  const reopen = useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.POST("/inbox/{id}/reopen", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  return { propose, edit, apply, reopen };
}
