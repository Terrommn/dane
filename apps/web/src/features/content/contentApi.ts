import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type ContentPiece = Schemas["ContentPiece"];
export type ContentView = Schemas["ContentView"];
export type ContentBody = Schemas["ContentBody"];
export type ShotRow = Schemas["ShotRow"];

export const STATUSES = ["idea", "script", "recorded", "edited", "published"] as const;
export const STATUS_LABEL: Record<string, string> = {
  idea: "Idea",
  script: "Guion",
  recorded: "Grabado",
  edited: "Editado",
  published: "Publicado",
};

/** "en 3 días", "hoy", "hace 2 días" (fórmula `dias_para_publicar` de Pipeline.base). */
export function daysToPublishLabel(d: number | null | undefined): string {
  if (d == null) return "—";
  if (d === 0) return "hoy";
  return d > 0 ? `en ${d} días` : `hace ${-d} días`;
}

/** Siempre 10 espacios para hooks (la plantilla pide escribir 10 y elegir 1). */
export function padHooks(h: string[]): string[] {
  const out = h.slice(0, 10);
  while (out.length < 10) out.push("");
  return out;
}

/** Hooks no vacíos con el índice elegido ajustado a la lista compacta que se envía al servidor. */
export function compactHooks(
  hooks: string[],
  chosen: number | null,
): { hooks: string[]; chosen_hook: number | null } {
  const kept: string[] = [];
  let newChosen: number | null = null;
  hooks.forEach((h, i) => {
    if (h.trim()) {
      if (i === chosen) newChosen = kept.length;
      kept.push(h.trim());
    }
  });
  return { hooks: kept, chosen_hook: newChosen };
}

export function contentBody(p: ContentPiece, patch: Partial<ContentBody> = {}): ContentBody {
  return {
    title: p.title,
    idea: p.idea,
    status: p.status,
    format: p.format,
    platforms: p.platforms,
    pillar: p.pillar ?? null,
    hooks: p.hooks,
    chosen_hook: p.chosen_hook ?? null,
    script_hook: p.script_hook,
    script_body: p.script_body,
    script_cta: p.script_cta,
    shot_list: p.shot_list as ShotRow[],
    edit_notes: p.edit_notes,
    result_worked: p.result_worked,
    result_change: p.result_change,
    publish_on: p.publish_on ?? null,
    published_on: p.published_on ?? null,
    raw_url: p.raw_url ?? null,
    render_url: p.render_url ?? null,
    views: p.views ?? null,
    retention_pct: p.retention_pct ?? null,
    area_id: p.area_id ?? null,
    project_id: p.project_id ?? null,
    archived: p.archived_at != null,
    ...patch,
    version: p.version,
  };
}

export function useContentList(status?: string, archived = false) {
  return useQuery({
    queryKey: ["content", "list", status ?? "", archived],
    queryFn: async () => unwrap(await api.GET("/content", { params: { query: { status, archived } } })),
  });
}
export function useBoard() {
  return useQuery({
    queryKey: ["content", "board"],
    queryFn: async () => unwrap(await api.GET("/content/board")),
  });
}
export function usePublished(sort: "views" | "retention") {
  return useQuery({
    queryKey: ["content", "published", sort],
    queryFn: async () =>
      unwrap(await api.GET("/content/published", { params: { query: { sort, limit: 50 } } })),
  });
}
export function useAiContext() {
  return useQuery({
    queryKey: ["content", "ai"],
    queryFn: async () => unwrap(await api.GET("/content/ai-context")),
  });
}
export function useContent(id: string) {
  return useQuery({
    queryKey: ["content", "one", id],
    queryFn: async () => unwrap(await api.GET("/content/{id}", { params: { path: { id } } })),
  });
}

export function useContentMutations() {
  const qc = useQueryClient();
  const refresh = () => qc.invalidateQueries({ queryKey: ["content"] });
  const create = useMutation({
    mutationFn: async (body: ContentBody) => unwrap(await api.POST("/content", { body })),
    onSuccess: refresh,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: ContentBody }) =>
      unwrap(await api.PUT("/content/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
  return { create, update };
}

export type SwipeRef = Schemas["SwipeRef"];
export type SwipeBody = Schemas["SwipeBody"];

export function useSwipe(q: string, tag: string, archived: boolean) {
  return useQuery({
    queryKey: ["swipe", q, tag, archived],
    queryFn: async () =>
      unwrap(
        await api.GET("/swipe", {
          params: { query: { q: q || undefined, tag: tag || undefined, archived } },
        }),
      ),
  });
}

export function useSwipeMutations() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id?: string; body: SwipeBody }) =>
      id
        ? unwrap(await api.PUT("/swipe/{id}", { params: { path: { id } }, body }))
        : unwrap(await api.POST("/swipe", { body })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["swipe"] }),
  });
}
