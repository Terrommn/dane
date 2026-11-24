// Piezas compartidas por los módulos de conocimiento: Markdown seguro, tags, enlaces/backlinks, favoritos, archivo y
// rutas por tipo de entidad (contrato 6 de docs/CONVENTIONS.md).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useId, useState } from "react";
import ReactMarkdown from "react-markdown";
import { Link } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Badge } from "../../components/ui/Display";
import { TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { errorMessage, unwrap } from "../../lib/errors";

export type EntityType =
  | "area"
  | "project"
  | "goal"
  | "habit"
  | "checkin"
  | "person"
  | "task"
  | "inbox"
  | "weekly_review"
  | "reading"
  | "content"
  | "swipe"
  | "subject"
  | "class_note"
  | "devlog_session"
  | "adr"
  | "note"
  | "drawing"
  | "transaction";

export const ENTITY_LABEL: Record<string, string> = {
  area: "Área",
  project: "Proyecto",
  goal: "Objetivo",
  habit: "Hábito",
  checkin: "Día",
  person: "Persona",
  task: "Tarea",
  inbox: "Inbox",
  weekly_review: "Revisión semanal",
  reading: "Lectura",
  content: "Contenido",
  swipe: "Swipe",
  subject: "Materia",
  class_note: "Clase",
  devlog_session: "Devlog",
  adr: "ADR",
  note: "Nota",
  drawing: "Dibujo",
  transaction: "Movimiento",
};

/** Ruta de la web para abrir una entidad (resultados de búsqueda, enlaces y backlinks). */
export function entityHref(type: string, id: string, onDate?: string | null, title?: string | null): string {
  switch (type) {
    case "area":
      return "/proyectos";
    case "project":
      return `/proyectos/${id}`;
    case "goal":
      return "/objetivos";
    case "habit":
      return "/habitos";
    case "checkin":
      return onDate ? `/hoy?date=${onDate}` : "/hoy";
    case "person":
      return `/personas/${id}`;
    case "task":
      return `/tareas?task=${id}`;
    case "inbox":
      return "/inbox";
    case "weekly_review":
      return title && /^\d{4}-W\d{2}$/.test(title) ? `/semanal?week=${title}` : "/semanal";
    case "reading":
      return "/lecturas";
    case "content":
      return `/contenido/${id}`;
    case "swipe":
      return `/swipe?id=${id}`;
    case "subject":
      return "/escuela";
    case "class_note":
      return `/escuela/clase/${id}`;
    case "devlog_session":
      return onDate ? `/dev?date=${onDate}` : "/dev";
    case "adr":
      return `/dev/adr/${id}`;
    case "note":
      return `/notas/${id}`;
    case "drawing":
      return `/dibujos/${id}`;
    case "transaction":
      return "/finanzas/movimientos";
    default:
      return "/buscar";
  }
}

/** Solo enlaces http(s)/mailto o rutas internas; cualquier otro esquema (javascript:, data:) se neutraliza. */
export function safeUrl(url: string): string {
  const u = url.trim();
  if (/^(https?:|mailto:)/i.test(u)) return u;
  if (u.startsWith("/") && !u.startsWith("//")) return u;
  if (u.startsWith("#")) return u;
  return "";
}

export function isHttpUrl(u: string) {
  return /^https?:\/\/\S+$/i.test(u.trim());
}

/** Markdown sin HTML crudo (skipHtml) y con URLs saneadas. Los snippets/código se muestran como texto. */
export function SafeMarkdown({ children, className = "" }: { children: string; className?: string }) {
  return (
    <div
      className={`prose-dane space-y-2 break-words [&_a]:text-link [&_a]:underline [&_code]:rounded [&_code]:bg-surface-2 [&_code]:px-1 [&_h1]:text-xl [&_h1]:font-bold [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:font-semibold [&_li]:ml-5 [&_ol]:list-decimal [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-surface-2 [&_pre]:p-3 [&_ul]:list-disc ${className}`}
    >
      <ReactMarkdown
        skipHtml
        urlTransform={(url) => safeUrl(url)}
        components={{
          a: ({ href, children: c }) => (
            <a href={href} target="_blank" rel="noopener noreferrer nofollow">
              {c}
            </a>
          ),
          img: ({ alt }) => <span className="text-muted">[imagen: {alt}]</span>,
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

/** "a, b , c" → ["a","b","c"] sin vacíos ni duplicados (sin distinguir mayúsculas). */
export function parseList(s: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of s.split(",")) {
    const v = raw.trim().replace(/^#/, "");
    if (!v) continue;
    const k = v.toLowerCase();
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(v);
  }
  return out;
}

/** Campo de lista separada por comas (tags, alias, plataformas). */
export function ListField({
  label,
  value,
  onChange,
  hint,
  placeholder,
}: {
  label: string;
  value: string[];
  onChange: (v: string[]) => void;
  hint?: ReactNode;
  placeholder?: string;
}) {
  const [text, setText] = useState(value.join(", "));
  useEffect(() => {
    setText((t) => (parseList(t).join("|") === value.join("|") ? t : value.join(", ")));
  }, [value]);
  return (
    <TextField
      label={label}
      hint={hint ?? "Separa con comas"}
      placeholder={placeholder}
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        onChange(parseList(e.target.value));
      }}
    />
  );
}

export function TagList({ tags }: { tags: string[] }) {
  if (!tags.length) return null;
  return (
    <span className="flex flex-wrap gap-1">
      {tags.map((t) => (
        <Badge key={t} tone="accent">
          #{t}
        </Badge>
      ))}
    </span>
  );
}

// ---------- Favoritos y archivo ----------

export function useFavorites() {
  return useQuery({
    queryKey: ["favorites"],
    queryFn: async () => unwrap(await api.GET("/favorites")),
  });
}

export function FavoriteButton({ type, id, initial }: { type: EntityType; id: string; initial?: boolean }) {
  const favs = useFavorites();
  const qc = useQueryClient();
  const toast = useToast();
  const isFav = favs.data?.some((f) => f.entity_type === type && f.entity_id === id) ?? initial ?? false;
  const m = useMutation({
    mutationFn: async (on: boolean) => {
      const params = { params: { path: { entity_type: type, id } } };
      unwrap(
        on
          ? await api.PUT("/favorites/{entity_type}/{id}", params)
          : await api.DELETE("/favorites/{entity_type}/{id}", params),
      );
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ["favorites"] }),
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-pressed={isFav}
      onClick={() => m.mutate(!isFav)}
      loading={m.isPending}
    >
      {isFav ? "★ Favorito" : "☆ Favorito"}
    </Button>
  );
}

export function useArchiveMutation(onDone?: () => void) {
  const qc = useQueryClient();
  const toast = useToast();
  return useMutation({
    mutationFn: async ({ type, id, restore }: { type: EntityType; id: string; restore?: boolean }) => {
      const params = { params: { path: { entity_type: type, id } } };
      unwrap(
        restore
          ? await api.POST("/archive/{entity_type}/{id}/restore", params)
          : await api.POST("/archive/{entity_type}/{id}", params),
      );
    },
    onSuccess: (_, v) => {
      toast.show(v.restore ? "Recuperado" : "Archivado", { tone: "ok" });
      void qc.invalidateQueries();
      onDone?.();
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
}

// ---------- Enlaces y backlinks ----------

type LinkView = Schemas["LinkView"];

export function useLinks(type: EntityType, id: string) {
  return useQuery({
    queryKey: ["links", type, id],
    queryFn: async () => unwrap(await api.GET("/links", { params: { query: { type, id } } })),
  });
}

/** Busca entidades del usuario para enlazar (usa /search). */
export function EntityPicker({
  onPick,
  exclude,
  types,
  label = "Buscar para enlazar",
}: {
  onPick: (hit: Schemas["SearchHit"]) => void;
  exclude?: { type: string; id: string };
  types?: string;
  label?: string;
}) {
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 250);
    return () => clearTimeout(t);
  }, [q]);
  const res = useQuery({
    queryKey: ["search", "picker", debounced, types],
    enabled: debounced.length >= 2,
    queryFn: async () =>
      unwrap(await api.GET("/search", { params: { query: { q: debounced, types, limit: 8 } } })),
  });
  const listId = useId();
  const hits = (res.data ?? []).filter(
    (h) => !(exclude && h.entity_type === exclude.type && h.entity_id === exclude.id),
  );
  return (
    <div>
      <TextField
        label={label}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Escribe al menos 2 letras"
        aria-controls={listId}
      />
      {debounced.length >= 2 && (
        <ul id={listId} className="mt-2 divide-y divide-line rounded-lg border border-line">
          {res.isPending && <li className="p-2 text-sm text-muted">Buscando…</li>}
          {!res.isPending && hits.length === 0 && <li className="p-2 text-sm text-muted">Sin resultados</li>}
          {hits.map((h) => (
            <li key={`${h.entity_type}:${h.entity_id}`}>
              <button
                type="button"
                className="flex w-full items-center justify-between gap-2 p-2 text-left hover:bg-surface-2"
                onClick={() => {
                  onPick(h);
                  setQ("");
                }}
              >
                <span className="min-w-0 truncate">{h.title || "(sin título)"}</span>
                <Badge>{ENTITY_LABEL[h.entity_type] ?? h.entity_type}</Badge>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function LinkRow({ l, outgoing, onRemove }: { l: LinkView; outgoing: boolean; onRemove?: () => void }) {
  const type = outgoing ? l.dst_type : l.src_type;
  const id = outgoing ? l.dst_id : l.src_id;
  return (
    <li className="flex items-center justify-between gap-2 py-1.5">
      <Link to={entityHref(type, id, null, l.other_title)} className="min-w-0 truncate">
        {l.other_title || "(sin título)"}
      </Link>
      <span className="flex shrink-0 items-center gap-2">
        <Badge>{ENTITY_LABEL[type] ?? type}</Badge>
        {l.relation !== "related" && (
          <Badge tone="accent">{l.relation === "contributes_to" ? "aporta a" : l.relation}</Badge>
        )}
        {onRemove && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onRemove}
            aria-label={`Quitar enlace a ${l.other_title ?? ""}`}
          >
            ✕
          </Button>
        )}
      </span>
    </li>
  );
}

/** Panel de enlaces salientes y backlinks de una entidad, con alta y baja de enlaces. */
export function LinksPanel({ type, id }: { type: EntityType; id: string }) {
  const links = useLinks(type, id);
  const qc = useQueryClient();
  const toast = useToast();
  const [adding, setAdding] = useState(false);
  const refresh = () => qc.invalidateQueries({ queryKey: ["links"] });
  const add = useMutation({
    mutationFn: async (h: Schemas["SearchHit"]) =>
      unwrap(
        await api.POST("/links", {
          body: { src_type: type, src_id: id, dst_type: h.entity_type, dst_id: h.entity_id },
        }),
      ),
    onSuccess: () => {
      void refresh();
      setAdding(false);
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  const remove = useMutation({
    mutationFn: async (l: LinkView) =>
      unwrap(
        await api.DELETE("/links", {
          body: {
            src_type: l.src_type,
            src_id: l.src_id,
            dst_type: l.dst_type,
            dst_id: l.dst_id,
            relation: l.relation,
          },
        }),
      ),
    onSuccess: () => void refresh(),
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="font-semibold">Enlaces</h3>
        <Button variant="secondary" size="sm" onClick={() => setAdding((a) => !a)}>
          {adding ? "Cancelar" : "Enlazar"}
        </Button>
      </div>
      {adding && <EntityPicker onPick={(h) => add.mutate(h)} exclude={{ type, id }} />}
      {links.isPending ? (
        <p className="text-sm text-muted">Cargando enlaces…</p>
      ) : links.isError ? (
        <p className="text-sm text-danger">{errorMessage(links.error)}</p>
      ) : (
        <>
          {links.data.links.length === 0 ? (
            <p className="text-sm text-muted">Sin enlaces.</p>
          ) : (
            <ul className="divide-y divide-line">
              {links.data.links.map((l) => (
                <LinkRow
                  key={`${l.dst_type}${l.dst_id}${l.relation}`}
                  l={l}
                  outgoing
                  onRemove={() => remove.mutate(l)}
                />
              ))}
            </ul>
          )}
          <h3 className="pt-2 font-semibold">Backlinks</h3>
          {links.data.backlinks.length === 0 ? (
            <p className="text-sm text-muted">Nada enlaza aquí todavía.</p>
          ) : (
            <ul className="divide-y divide-line">
              {links.data.backlinks.map((l) => (
                <LinkRow key={`${l.src_type}${l.src_id}${l.relation}`} l={l} outgoing={false} />
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

/** Selector de área/proyecto reutilizable. */
export function useAreasAndProjects() {
  const areas = useQuery({
    queryKey: ["areas", { includeArchived: false }],
    queryFn: async () => unwrap(await api.GET("/areas", { params: { query: { archived: false } } })),
  });
  const projects = useQuery({
    queryKey: ["projects", { includeArchived: false }],
    queryFn: async () => unwrap(await api.GET("/projects", { params: { query: { archived: false } } })),
  });
  return { areas: areas.data ?? [], projects: projects.data ?? [] };
}

export function areaName(areas: { id: string; name: string }[], id?: string | null) {
  return areas.find((a) => a.id === id)?.name ?? "Sin área";
}

/**
 * Resolución explícita de un 409: "servidor" descarta mis cambios, "mía"/"mezcla" vuelven a guardar sobre la versión
 * actual del servidor (nunca en silencio).
 */
export function ConflictPrompt<T extends { version: number }>({
  conflict,
  mine,
  labels,
  onTakeServer,
  onSave,
  onClose,
}: {
  conflict: T | null;
  mine: Record<string, unknown>;
  labels?: Record<string, string>;
  onTakeServer: (server: T) => void;
  onSave: (data: Record<string, unknown>, serverVersion: number) => void;
  onClose: () => void;
}) {
  if (!conflict) return null;
  return (
    <ConflictDialog
      open
      mine={mine}
      theirs={conflict as unknown as Record<string, unknown>}
      labels={labels}
      onClose={onClose}
      onResolve={(r) => {
        onClose();
        if (r.choice === "theirs") onTakeServer(conflict);
        else onSave(r.choice === "mine" ? mine : r.body, conflict.version);
      }}
    />
  );
}
