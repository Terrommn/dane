import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, apiFetch, type Schemas } from "../../lib/api";
import { toApiError, unwrap } from "../../lib/errors";
import type { FolderFile } from "./folder";

export type ImportRunView = Schemas["ImportRunView"];
export type ImportRun = Schemas["ImportRun"];

export function useImportRuns() {
  return useQuery({ queryKey: ["imports"], queryFn: async () => unwrap(await api.GET("/imports")) });
}

export function useImportRun(id: string | null) {
  return useQuery({
    queryKey: ["imports", id],
    enabled: id !== null,
    queryFn: async () => unwrap(await api.GET("/imports/{id}", { params: { path: { id: id ?? "" } } })),
  });
}

async function json<T>(r: Response): Promise<T> {
  const body = await r.json().catch(() => null);
  if (!r.ok) throw toApiError(r.status, body);
  return body as T;
}

export function useImportMutations() {
  const qc = useQueryClient();
  const set = (v: ImportRunView) => {
    qc.setQueryData(["imports", v.run.id], v);
    void qc.invalidateQueries({ queryKey: ["imports"], exact: true });
  };
  /** ZIP binario (`application/zip`); el dry-run no escribe datos. */
  const uploadZip = useMutation({
    mutationFn: async (file: Blob & { name?: string }) =>
      json<ImportRunView>(
        await apiFetch(`/imports?name=${encodeURIComponent(file.name ?? "vault.zip")}`, {
          method: "POST",
          body: file,
          headers: { "Content-Type": "application/zip" },
        }),
      ),
    onSuccess: set,
  });
  const uploadFolder = useMutation({
    mutationFn: async ({ name, files }: { name: string; files: FolderFile[] }) =>
      unwrap(await api.POST("/imports/folder", { body: { name, files } })),
    onSuccess: set,
  });
  const apply = useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.POST("/imports/{id}/apply", { params: { path: { id } } })),
    onSuccess: (v) => {
      set(v);
      // La importación crea registros en casi todos los módulos.
      void qc.invalidateQueries();
    },
  });
  return { uploadZip, uploadFolder, apply };
}

/** Descarga una exportación autenticada como archivo (sin exponer tokens en la URL). */
export async function downloadExport(kind: "json" | "zip"): Promise<string> {
  const r = await apiFetch(`/exports/${kind}`, { method: "GET" });
  if (!r.ok) throw toApiError(r.status, await r.json().catch(() => null));
  const blob = await r.blob();
  const cd = r.headers.get("content-disposition") ?? "";
  const name = /filename="([^"]+)"/.exec(cd)?.[1] ?? `dane-export.${kind}`;
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  return name;
}
