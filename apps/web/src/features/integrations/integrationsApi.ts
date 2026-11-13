import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type Overview = Schemas["IntegrationsOverview"];
export type ConnectionStatus = Schemas["ConnectionStatus"];
export type CalendarInfo = Schemas["CalendarInfo"];
export type OrderedTask = Schemas["OrderedTask"];
export type SyncReport = Schemas["SyncReport"];

export const integrationsKey = ["integrations"] as const;

export function useIntegrations() {
  return useQuery({ queryKey: integrationsKey, queryFn: async () => unwrap(await api.GET("/integrations")) });
}

export function useGoogleCalendars(enabled: boolean) {
  return useQuery({
    queryKey: ["integrations", "google", "calendars"],
    enabled,
    retry: false,
    queryFn: async () => unwrap(await api.GET("/integrations/google/calendars")),
  });
}

export function useClickupTasks(enabled: boolean) {
  return useQuery({
    queryKey: ["integrations", "clickup", "tasks"],
    enabled,
    queryFn: async () => unwrap(await api.GET("/integrations/clickup/tasks")),
  });
}

export function useSuggestions() {
  return useQuery({
    queryKey: ["integrations", "suggestions"],
    queryFn: async () => unwrap(await api.GET("/integrations/suggestions")),
  });
}

export function useIntegrationMutations() {
  const qc = useQueryClient();
  const refresh = () => qc.invalidateQueries({ queryKey: integrationsKey });
  const googleConnect = useMutation({
    mutationFn: async (redirectTo: string) =>
      unwrap(
        await api.GET("/integrations/google/connect", { params: { query: { redirect_to: redirectTo } } }),
      ),
  });
  const googleConfig = useMutation({
    mutationFn: async (body: Schemas["GoogleConfigBody"]) =>
      unwrap(await api.PUT("/integrations/google/config", { body })),
    onSuccess: refresh,
  });
  const googleSync = useMutation({
    mutationFn: async () => unwrap(await api.POST("/integrations/google/sync", { body: {} })),
    onSuccess: refresh,
  });
  const googleDisconnect = useMutation({
    mutationFn: async () => {
      const r = await api.DELETE("/integrations/google");
      if (!r.response.ok) unwrap(r);
    },
    onSuccess: refresh,
  });
  const clickupSave = useMutation({
    mutationFn: async (body: Schemas["ClickupBody"]) =>
      unwrap(await api.PUT("/integrations/clickup", { body })),
    onSuccess: refresh,
  });
  const clickupSync = useMutation({
    mutationFn: async () => unwrap(await api.POST("/integrations/clickup/sync")),
    onSuccess: () => {
      void refresh();
      void qc.invalidateQueries({ queryKey: ["tasks"] });
    },
  });
  const clickupDisconnect = useMutation({
    mutationFn: async () => {
      const r = await api.DELETE("/integrations/clickup");
      if (!r.response.ok) unwrap(r);
    },
    onSuccess: refresh,
  });
  const refreshSuggestions = () => qc.invalidateQueries({ queryKey: ["integrations", "suggestions"] });
  const acceptInteraction = useMutation({
    mutationFn: async ({ id, topic }: { id: string; topic: string | null }) =>
      unwrap(
        await api.POST("/integrations/suggestions/interactions/{id}/accept", {
          params: { path: { id } },
          body: { topic },
        }),
      ),
    onSuccess: refreshSuggestions,
  });
  const dismissInteraction = useMutation({
    mutationFn: async (id: string) => {
      const r = await api.POST("/integrations/suggestions/interactions/{id}/dismiss", {
        params: { path: { id } },
      });
      if (!r.response.ok) unwrap(r);
    },
    onSuccess: refreshSuggestions,
  });
  const acceptPerson = useMutation({
    mutationFn: async ({
      id,
      name,
      area_id,
    }: {
      id: string;
      name: string | null;
      area_id: string | null;
    }) => {
      const r = await api.POST("/integrations/suggestions/people/{id}/accept", {
        params: { path: { id } },
        body: { name, area_id },
      });
      if (!r.response.ok) unwrap(r);
    },
    onSuccess: () => {
      void refreshSuggestions();
      void qc.invalidateQueries({ queryKey: ["people"] });
    },
  });
  const dismissPerson = useMutation({
    mutationFn: async (id: string) => {
      const r = await api.POST("/integrations/suggestions/people/{id}/dismiss", { params: { path: { id } } });
      if (!r.response.ok) unwrap(r);
    },
    onSuccess: refreshSuggestions,
  });
  return {
    googleConnect,
    googleConfig,
    googleSync,
    googleDisconnect,
    clickupSave,
    clickupSync,
    clickupDisconnect,
    acceptInteraction,
    dismissInteraction,
    acceptPerson,
    dismissPerson,
  };
}
