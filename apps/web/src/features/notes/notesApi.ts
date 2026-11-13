import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type Note = Schemas["Note"];
export type NoteKind = "note" | "runbook" | "til" | "snippet";

export const NOTE_KIND: Record<NoteKind, string> = {
  note: "Nota",
  runbook: "Runbook",
  til: "TIL",
  snippet: "Snippet",
};

/**
 * Cuerpo de POST/PUT /notes. Se declara aquí porque en el OpenAPI actual el nombre `NoteBody` choca con el cuerpo de
 * notas de personas (`{ on_date, text }`) y el tipo generado no corresponde a esta ruta.
 */
export interface NoteInput {
  kind: NoteKind;
  title: string;
  body_md: string;
  language: string | null;
  aliases: string[];
  tags: string[];
  area_id: string | null;
  project_id: string | null;
  metadata: Record<string, unknown>;
  archived: boolean;
  version?: number | null;
}

export function noteInput(n: Note, patch: Partial<NoteInput> = {}): NoteInput {
  return {
    kind: n.kind as NoteKind,
    title: n.title,
    body_md: n.body_md,
    language: n.language ?? null,
    aliases: n.aliases,
    tags: n.tags,
    area_id: n.area_id ?? null,
    project_id: n.project_id ?? null,
    metadata: (n.metadata as Record<string, unknown>) ?? {},
    archived: n.archived_at != null,
    ...patch,
    version: n.version,
  };
}

export interface NoteFilter {
  kind?: string;
  tag?: string;
  q?: string;
  archived?: boolean;
  area_id?: string;
  project_id?: string;
  limit?: number;
  offset?: number;
}

export function useNotes(f: NoteFilter) {
  return useQuery({
    queryKey: ["notes", f],
    queryFn: async () => {
      const page = unwrap(await api.GET("/notes", { params: { query: { ...f, limit: f.limit ?? 50 } } }));
      return { ...page, items: page.items as unknown as Note[] };
    },
  });
}

export function useNote(id: string) {
  return useQuery({
    queryKey: ["note", id],
    queryFn: async () => unwrap(await api.GET("/notes/{id}", { params: { path: { id } } })),
  });
}

export function useTags() {
  return useQuery({ queryKey: ["tags"], queryFn: async () => unwrap(await api.GET("/tags")) });
}

export function useNoteMutations() {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["notes"] });
    void qc.invalidateQueries({ queryKey: ["tags"] });
  };
  const create = useMutation({
    mutationFn: async (body: NoteInput) => unwrap(await api.POST("/notes", { body: body as never })) as Note,
    onSuccess: refresh,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: NoteInput }) =>
      unwrap(await api.PUT("/notes/{id}", { params: { path: { id } }, body: body as never })) as Note,
    onSuccess: (n) => {
      refresh();
      void qc.invalidateQueries({ queryKey: ["note", n.id] });
    },
  });
  return { create, update };
}
