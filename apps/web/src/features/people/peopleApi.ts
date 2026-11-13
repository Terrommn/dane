import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";

export type Person = Schemas["Person"];
export type PersonView = Schemas["PersonView"];
export type PersonBody = Schemas["PersonBody"];

export const RELATIONSHIPS = [
  "jefe",
  "subordinado",
  "compañero",
  "cliente",
  "familia",
  "pareja",
  "amigo",
  "otro",
] as const;

/** Cumpleaños para el formulario: `YYYY-MM-DD` si se conoce el año, si no `MM-DD`. */
export function birthdayString(p: Pick<Person, "birthday_month" | "birthday_day" | "birthday_year">): string {
  if (!p.birthday_month || !p.birthday_day) return "";
  const md = `${String(p.birthday_month).padStart(2, "0")}-${String(p.birthday_day).padStart(2, "0")}`;
  return p.birthday_year ? `${p.birthday_year}-${md}` : md;
}

/** Valida `YYYY-MM-DD` o `MM-DD` (acepta 02-29 sin año). */
export function validBirthday(s: string): boolean {
  const t = s.trim();
  if (!t) return true;
  const m = /^(?:(\d{4})-)?(\d{2})-(\d{2})$/.exec(t);
  if (!m) return false;
  const y = m[1] ? Number(m[1]) : 2000;
  const mo = Number(m[2]);
  const d = Number(m[3]);
  const dt = new Date(Date.UTC(y, mo - 1, d));
  return dt.getUTCMonth() === mo - 1 && dt.getUTCDate() === d;
}

export function birthdayLabel(p: PersonView): string | null {
  if (!p.birthday_month || !p.birthday_day) return null;
  const date = new Intl.DateTimeFormat("es-MX", { day: "numeric", month: "long", timeZone: "UTC" }).format(
    new Date(Date.UTC(2000, p.birthday_month - 1, p.birthday_day)),
  );
  return p.birthday_year ? `${date} de ${p.birthday_year}` : date;
}

export function personBody(p: Person, patch: Partial<PersonBody> = {}): PersonBody {
  return {
    name: p.name,
    aliases: p.aliases,
    area_id: p.area_id ?? null,
    relationship: p.relationship ?? null,
    org: p.org ?? null,
    role: p.role ?? null,
    contact: p.contact ?? null,
    email: p.email ?? null,
    context_md: p.context_md,
    birthday: birthdayString(p) || null,
    archived: p.archived_at != null,
    ...patch,
    version: p.version,
  };
}

export function usePeopleByArea() {
  return useQuery({
    queryKey: ["people", "by-area"],
    queryFn: async () => unwrap(await api.GET("/people/by-area")),
  });
}

export function usePeopleSearch(q: string, archived: boolean) {
  return useQuery({
    queryKey: ["people", "search", q, archived],
    queryFn: async () =>
      unwrap(await api.GET("/people", { params: { query: { q: q || undefined, archived } } })),
  });
}

export function useBirthdays(days = 60) {
  return useQuery({
    queryKey: ["people", "birthdays", days],
    queryFn: async () => unwrap(await api.GET("/people/birthdays", { params: { query: { days } } })),
  });
}

export function usePerson(id: string) {
  return useQuery({
    queryKey: ["person", id],
    queryFn: async () => unwrap(await api.GET("/people/{id}", { params: { path: { id } } })),
  });
}

export function useSuggestions(status = "pending") {
  return useQuery({
    queryKey: ["person-suggestions", status],
    queryFn: async () => unwrap(await api.GET("/person-suggestions", { params: { query: { status } } })),
  });
}

export function usePeopleMutations() {
  const qc = useQueryClient();
  const refresh = (id?: string) => {
    void qc.invalidateQueries({ queryKey: ["people"] });
    void qc.invalidateQueries({ queryKey: ["person-suggestions"] });
    if (id) void qc.invalidateQueries({ queryKey: ["person", id] });
  };
  const create = useMutation({
    mutationFn: async (body: PersonBody) => unwrap(await api.POST("/people", { body })),
    onSuccess: (p) => refresh(p.id),
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: PersonBody }) =>
      unwrap(await api.PUT("/people/{id}", { params: { path: { id } }, body })),
    onSuccess: (p) => refresh(p.id),
  });
  const addNote = useMutation({
    mutationFn: async ({ id, text, on_date }: { id: string; text: string; on_date: string | null }) =>
      unwrap(await api.POST("/people/{id}/notes", { params: { path: { id } }, body: { text, on_date } })),
    onSuccess: (_, v) => refresh(v.id),
  });
  const deleteNote = useMutation({
    mutationFn: async ({ id, noteId }: { id: string; noteId: string }) =>
      unwrap(await api.DELETE("/people/{id}/notes/{note_id}", { params: { path: { id, note_id: noteId } } })),
    onSuccess: (_, v) => refresh(v.id),
  });
  const addInteraction = useMutation({
    mutationFn: async ({ id, topic, on_date }: { id: string; topic: string; on_date: string | null }) =>
      unwrap(
        await api.POST("/people/{id}/interactions", { params: { path: { id } }, body: { topic, on_date } }),
      ),
    onSuccess: (_, v) => refresh(v.id),
  });
  const deleteInteraction = useMutation({
    mutationFn: async ({ interactionId }: { id: string; interactionId: string }) =>
      unwrap(await api.DELETE("/interactions/{id}", { params: { path: { id: interactionId } } })),
    onSuccess: (_, v) => refresh(v.id),
  });
  const acceptSuggestion = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["AcceptSuggestion"] }) =>
      unwrap(await api.POST("/person-suggestions/{id}/accept", { params: { path: { id } }, body })),
    onSuccess: () => refresh(),
  });
  const dismissSuggestion = useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.POST("/person-suggestions/{id}/dismiss", { params: { path: { id } } })),
    onSuccess: () => refresh(),
  });
  return {
    create,
    update,
    addNote,
    deleteNote,
    addInteraction,
    deleteInteraction,
    acceptSuggestion,
    dismissSuggestion,
  };
}
