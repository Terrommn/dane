import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, type Schemas } from "../../lib/api";
import { ApiError, unwrap } from "../../lib/errors";
import { useAuth } from "../auth/AuthProvider";

export type Settings = Schemas["UserSettings"];
export type SettingsPatch = Omit<Schemas["SettingsPatch"], "version">;

export const settingsKey = ["settings"] as const;

export function useSettings() {
  return useQuery({ queryKey: settingsKey, queryFn: async () => unwrap(await api.GET("/settings")) });
}

/**
 * Guarda un parche de preferencias con la versión conocida. Si el servidor responde 409, expone `conflict` con el
 * estado actual para que la UI resuelva explícitamente (nunca se reintenta en silencio).
 */
export function useSaveSettings() {
  const qc = useQueryClient();
  const { refreshMe } = useAuth();
  const [conflict, setConflict] = useState<{ patch: SettingsPatch; current: Settings } | null>(null);
  const mutation = useMutation({
    mutationFn: async ({ patch, version }: { patch: SettingsPatch; version: number }) =>
      unwrap(await api.PUT("/settings", { body: { ...patch, version } })),
    onSuccess: (data) => {
      qc.setQueryData(settingsKey, data);
      void refreshMe();
    },
  });
  const save = async (patch: SettingsPatch) => {
    const cached = qc.getQueryData<Settings>(settingsKey);
    const current: Settings = cached ?? unwrap(await api.GET("/settings"));
    try {
      return await mutation.mutateAsync({ patch, version: current.version });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) {
        setConflict({ patch, current: e.current as Settings });
        qc.setQueryData(settingsKey, e.current as Settings);
      }
      throw e;
    }
  };
  const resolve = async (
    r: { choice: "mine" } | { choice: "theirs" } | { choice: "merged"; body: Record<string, unknown> },
  ) => {
    if (!conflict) return;
    const c = conflict;
    setConflict(null);
    if (r.choice === "theirs") return;
    const patch = r.choice === "merged" ? (r.body as SettingsPatch) : c.patch;
    await mutation.mutateAsync({ patch, version: c.current.version });
  };
  return {
    save,
    saving: mutation.isPending,
    error: mutation.error,
    conflict,
    resolve,
    dismissConflict: () => setConflict(null),
  };
}

export const MODULES: { key: string; label: string; hint: string }[] = [
  { key: "finance", label: "Finanzas", hint: "Gastos, ingresos, presupuestos, tarjetas y deudas" },
  { key: "people", label: "Personas", hint: "Contactos, cumpleaños e interacciones" },
  { key: "readings", label: "Lecturas", hint: "Libros y artículos" },
  { key: "content", label: "Contenido y Swipe", hint: "Pipeline de videos y referencias" },
  { key: "school", label: "Escuela", hint: "Materias, notas de clase y entregas" },
  { key: "dev", label: "Desarrollo", hint: "Devlog, ADRs, runbooks, TILs y snippets" },
];

export const WEEKDAYS = ["", "Lunes", "Martes", "Miércoles", "Jueves", "Viernes", "Sábado", "Domingo"];

export function timeZones(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (k: string) => string[] };
  const list = intl.supportedValuesOf?.("timeZone") ?? [];
  return list.length
    ? list
    : ["America/Monterrey", "America/Mexico_City", "America/Tijuana", "America/Cancun", "UTC"];
}
