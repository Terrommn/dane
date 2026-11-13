import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { api } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import { ENTITY_LABEL, entityHref, TagList, useAreasAndProjects } from "../notes/common";

/** Módulos buscables (tipos del índice de búsqueda). */
export const SEARCH_TYPES = [
  "note",
  "task",
  "person",
  "goal",
  "habit",
  "project",
  "area",
  "inbox",
  "devotional",
  "reading",
  "content",
  "swipe",
  "class_note",
  "subject",
  "devlog_session",
  "adr",
  "weekly_review",
  "drawing",
  "transaction",
] as const;

/** Construye los parámetros de /search a partir de la URL (vacíos se omiten). */
export function searchQuery(p: URLSearchParams) {
  const opt = (k: string) => p.get(k) || undefined;
  return {
    q: p.get("q") ?? "",
    types: opt("types"),
    area_id: opt("area"),
    status: opt("status"),
    from: opt("from"),
    to: opt("to"),
    archived: p.get("archived") === "1" ? true : undefined,
    limit: 50,
  };
}

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const [text, setText] = useState(params.get("q") ?? "");
  const query = searchQuery(params);
  const { areas } = useAreasAndProjects();
  const selected = new Set((params.get("types") ?? "").split(",").filter(Boolean));
  const results = useQuery({
    queryKey: ["search", query],
    enabled: query.q.trim().length > 0,
    queryFn: async () => unwrap(await api.GET("/search", { params: { query } })),
  });
  const update = (patch: Record<string, string>) => {
    const p = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v) p.set(k, v);
      else p.delete(k);
    }
    setParams(p, { replace: true });
  };
  const toggleType = (t: string) => {
    const next = new Set(selected);
    if (next.has(t)) next.delete(t);
    else next.add(t);
    update({ types: [...next].join(",") });
  };

  return (
    <div className="space-y-4">
      <PageHeader title="Buscar" subtitle="En todos tus módulos; no importan acentos ni mayúsculas" />
      <form
        aria-label="Buscar en todos los módulos"
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          update({ q: text.trim() });
        }}
      >
        <div className="flex items-end gap-2">
          <TextField
            label="¿Qué buscas?"
            className="flex-1"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="cancion, Juan, ADR…"
            autoFocus
          />
          <Button type="submit">Buscar</Button>
        </div>
        <details className="rounded-lg border border-line bg-surface p-3">
          <summary className="cursor-pointer font-medium">Filtros</summary>
          <fieldset className="mt-3">
            <legend className="mb-2 text-sm font-medium">Módulos</legend>
            <div className="flex flex-wrap gap-2">
              {SEARCH_TYPES.map((t) => (
                <label
                  key={t}
                  className="inline-flex items-center gap-1 rounded-full border border-line px-3 py-1 text-sm"
                >
                  <input type="checkbox" checked={selected.has(t)} onChange={() => toggleType(t)} />
                  {ENTITY_LABEL[t]}
                </label>
              ))}
            </div>
          </fieldset>
          <div className="mt-3 grid gap-3 sm:grid-cols-4">
            <SelectField
              label="Área"
              value={params.get("area") ?? ""}
              onChange={(e) => update({ area: e.target.value })}
            >
              <option value="">Todas</option>
              {areas.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Estado"
              value={params.get("status") ?? ""}
              onChange={(e) => update({ status: e.target.value })}
              placeholder="open, published…"
            />
            <TextField
              label="Desde"
              type="date"
              value={params.get("from") ?? ""}
              onChange={(e) => update({ from: e.target.value })}
            />
            <TextField
              label="Hasta"
              type="date"
              value={params.get("to") ?? ""}
              onChange={(e) => update({ to: e.target.value })}
            />
          </div>
          <CheckboxField
            className="mt-3"
            label="Incluir archivados"
            checked={params.get("archived") === "1"}
            onChange={(e) => update({ archived: e.target.checked ? "1" : "" })}
          />
        </details>
      </form>

      {!query.q ? (
        <EmptyState title="Escribe algo para buscar">
          Busca notas, tareas, personas, contenido, ADRs y más.
        </EmptyState>
      ) : (
        <QueryState
          query={results}
          isEmpty={(d) => d.length === 0}
          empty={<EmptyState title="Sin resultados">Prueba otras palabras o quita filtros.</EmptyState>}
        >
          {(hits) => (
            <ul className="space-y-2" aria-label="Resultados">
              {hits.map((h) => (
                <li key={`${h.entity_type}:${h.entity_id}`}>
                  <Link
                    to={entityHref(h.entity_type, h.entity_id, h.on_date, h.title)}
                    className="block rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="min-w-0 font-semibold text-fg">{h.title || "(sin título)"}</span>
                      <span className="flex items-center gap-2">
                        <Badge>{ENTITY_LABEL[h.entity_type] ?? h.entity_type}</Badge>
                        {h.archived && <Badge tone="warn">Archivado</Badge>}
                        {h.status && <Badge tone="accent">{h.status}</Badge>}
                        {h.on_date && (
                          <span className="text-sm text-muted">{formatShortDate(h.on_date)}</span>
                        )}
                      </span>
                    </div>
                    {h.snippet && <p className="mt-1 line-clamp-2 text-sm text-muted">{h.snippet}</p>}
                    <TagList tags={h.tags} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </QueryState>
      )}
    </div>
  );
}
