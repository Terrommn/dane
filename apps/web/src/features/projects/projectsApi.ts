import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type Area = Schemas["Area"];
export type Project = Schemas["Project"];
export type ActivityItem = Schemas["ActivityItem"];
export interface ProjectLink {
  label: string;
  url: string;
}

export const PROJECT_STATUS: Record<string, string> = {
  active: "Activo",
  paused: "En pausa",
  done: "Terminado",
  archived: "Archivado",
};

export function useAreas(includeArchived = false) {
  return useQuery({
    queryKey: ["areas", { includeArchived }],
    queryFn: async () =>
      unwrap(await api.GET("/areas", { params: { query: { archived: includeArchived } } })),
  });
}

export function useProjects(includeArchived = false) {
  return useQuery({
    queryKey: ["projects", { includeArchived }],
    queryFn: async () =>
      unwrap(await api.GET("/projects", { params: { query: { archived: includeArchived } } })),
  });
}

export function useProject(id: string) {
  return useQuery({
    queryKey: ["project", id],
    queryFn: async () => unwrap(await api.GET("/projects/{id}", { params: { path: { id } } })),
  });
}

export function useProjectActivity(id: string | null) {
  return useQuery({
    queryKey: ["activity", id ?? "all"],
    queryFn: async () =>
      id
        ? unwrap(await api.GET("/projects/{id}/activity", { params: { path: { id } } }))
        : unwrap(await api.GET("/activity")),
  });
}

export function useAreaMutations() {
  const qc = useQueryClient();
  const invalidate = () => qc.invalidateQueries({ queryKey: ["areas"] });
  const create = useMutation({
    mutationFn: async (body: Schemas["AreaBody"]) => unwrap(await api.POST("/areas", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ area, patch }: { area: Area; patch: Partial<Schemas["AreaBody"]> }) =>
      unwrap(
        await api.PUT("/areas/{id}", {
          params: { path: { id: area.id } },
          body: {
            name: area.name,
            slug: area.slug,
            description: area.description,
            color: area.color ?? null,
            position: area.position,
            archived: area.archived_at != null,
            ...patch,
            version: area.version,
          },
        }),
      ),
    onSettled: invalidate,
  });
  return { create, update };
}

export function projectBody(p: Project, patch: Partial<Schemas["ProjectBody"]> = {}): Schemas["ProjectBody"] {
  return {
    area_id: p.area_id ?? null,
    name: p.name,
    slug: p.slug,
    description: p.description,
    status: p.status,
    repo_url: p.repo_url ?? null,
    links: p.links,
    notes_md: p.notes_md,
    archived: p.archived_at != null,
    ...patch,
    version: p.version,
  };
}

export function useProjectMutations() {
  const qc = useQueryClient();
  const invalidate = (id?: string) => {
    void qc.invalidateQueries({ queryKey: ["projects"] });
    void qc.invalidateQueries({ queryKey: ["activity"] });
    if (id) void qc.invalidateQueries({ queryKey: ["project", id] });
  };
  const create = useMutation({
    mutationFn: async (body: Schemas["ProjectBody"]) => unwrap(await api.POST("/projects", { body })),
    onSuccess: (p) => invalidate(p.id),
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["ProjectBody"] }) =>
      unwrap(await api.PUT("/projects/{id}", { params: { path: { id } }, body })),
    onSuccess: (p) => {
      qc.setQueryData(["project", p.id], p);
      invalidate(p.id);
    },
  });
  return { create, update };
}
