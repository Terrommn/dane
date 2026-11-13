import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type DevSession = Schemas["DevSession"];
export type SessionBody = Schemas["SessionBody"];
export type Adr = Schemas["Adr"];
export type AdrBody = Schemas["AdrBody"];

export const ADR_STATUS: Record<string, string> = {
  proposed: "Propuesta",
  accepted: "Aceptada",
  superseded: "Reemplazada",
};

/** "ADR-0007" */
export function adrCode(n: number) {
  return `ADR-${String(n).padStart(4, "0")}`;
}

export function sessionBody(s: DevSession, patch: Partial<SessionBody> = {}): SessionBody {
  return {
    repo: s.repo,
    branch: s.branch,
    at_time: s.at_time ?? null,
    changes_md: s.changes_md,
    bugs_md: s.bugs_md,
    decisions_md: s.decisions_md,
    next_steps_md: s.next_steps_md,
    project_ids: s.project_ids,
    ...patch,
    version: s.version,
  };
}

export function useDevlogDay(date: string) {
  return useQuery({
    queryKey: ["devlog", date],
    queryFn: async () => unwrap(await api.GET("/devlogs/{date}", { params: { path: { date } } })),
  });
}

export function useDevlogs(from?: string, to?: string) {
  return useQuery({
    queryKey: ["devlogs", from, to],
    queryFn: async () => unwrap(await api.GET("/devlogs", { params: { query: { from, to } } })),
  });
}

export function useDevMutations(date: string) {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["devlog"] });
    void qc.invalidateQueries({ queryKey: ["devlogs"] });
    void qc.invalidateQueries({ queryKey: ["activity"] });
  };
  const summary = useMutation({
    mutationFn: async ({ summary_md, version }: { summary_md: string; version?: number | null }) =>
      unwrap(await api.PUT("/devlogs/{date}", { params: { path: { date } }, body: { summary_md, version } })),
    onSuccess: refresh,
  });
  const addSession = useMutation({
    mutationFn: async (body: SessionBody) =>
      unwrap(await api.POST("/devlogs/{date}/sessions", { params: { path: { date } }, body })),
    onSuccess: refresh,
  });
  const updateSession = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: SessionBody }) =>
      unwrap(await api.PUT("/devlog-sessions/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
  const deleteSession = useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.DELETE("/devlog-sessions/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const preview = useMutation({
    mutationFn: async (text: string) =>
      unwrap(await api.POST("/devlogs/import/preview", { body: { text, date } })),
  });
  const importText = useMutation({
    mutationFn: async (text: string) => unwrap(await api.POST("/devlogs/import", { body: { text, date } })),
    onSuccess: refresh,
  });
  return { summary, addSession, updateSession, deleteSession, preview, importText };
}

export function useAdrs(projectId?: string, status?: string) {
  return useQuery({
    queryKey: ["adrs", projectId ?? "", status ?? ""],
    queryFn: async () =>
      unwrap(
        await api.GET("/adrs", {
          params: { query: { project_id: projectId || undefined, status: status || undefined } },
        }),
      ),
  });
}

export function useAdr(id: string) {
  return useQuery({
    queryKey: ["adr", id],
    queryFn: async () => unwrap(await api.GET("/adrs/{id}", { params: { path: { id } } })),
  });
}

export function useAdrMutations() {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["adrs"] });
    void qc.invalidateQueries({ queryKey: ["adr"] });
  };
  const create = useMutation({
    mutationFn: async (body: AdrBody) => unwrap(await api.POST("/adrs", { body })),
    onSuccess: refresh,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: AdrBody }) =>
      unwrap(await api.PUT("/adrs/{id}", { params: { path: { id } }, body })),
    onSuccess: refresh,
  });
  return { create, update };
}
