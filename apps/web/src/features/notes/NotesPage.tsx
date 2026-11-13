import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import { ENTITY_LABEL, entityHref, TagList, useFavorites } from "./common";
import { NOTE_KIND, type NoteKind, useNoteMutations, useNotes, useTags } from "./notesApi";

/** Notas, runbooks, TILs y snippets; filtros por tipo, tag, texto y archivados; favoritos. */
export function NotesPage({ fixedKind }: { fixedKind?: NoteKind }) {
  const [params, setParams] = useSearchParams();
  const kind = fixedKind ?? (params.get("kind") || "");
  const tag = params.get("tag") ?? "";
  const [q, setQ] = useState(params.get("q") ?? "");
  const archived = params.get("archived") === "1";
  const [page, setPage] = useState(0);
  const notes = useNotes({
    kind: kind || undefined,
    tag: tag || undefined,
    q: params.get("q") || undefined,
    archived,
    limit: 30,
    offset: page * 30,
  });
  const tags = useTags();
  const favs = useFavorites();
  const { create } = useNoteMutations();
  const navigate = useNavigate();
  const toast = useToast();
  const set = (k: string, v: string) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    setParams(p, { replace: true });
    setPage(0);
  };

  const newNote = async () => {
    try {
      const n = await create.mutateAsync({
        kind: (kind || "note") as NoteKind,
        title: "Nueva nota",
        body_md: "",
        language: null,
        aliases: [],
        tags: tag ? [tag] : [],
        area_id: null,
        project_id: null,
        metadata: {},
        archived: false,
      });
      navigate(`/notas/${n.id}`);
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    }
  };

  return (
    <div className="space-y-4">
      {!fixedKind && (
        <PageHeader
          title="Notas"
          subtitle="Markdown con tags, alias, enlaces y backlinks"
          actions={
            <Button onClick={newNote} loading={create.isPending}>
              Nueva nota
            </Button>
          }
        />
      )}
      {fixedKind && (
        <div className="flex justify-end">
          <Button onClick={newNote} loading={create.isPending}>
            Nuevo {NOTE_KIND[fixedKind]}
          </Button>
        </div>
      )}

      {!fixedKind && favs.data && favs.data.length > 0 && (
        <Card title="Favoritos">
          <ul className="flex flex-wrap gap-2">
            {favs.data.map((f) => (
              <li key={`${f.entity_type}${f.entity_id}`}>
                <Link
                  to={entityHref(f.entity_type, f.entity_id, f.on_date)}
                  className="inline-flex items-center gap-1 rounded-full border border-line px-3 py-1 text-sm"
                >
                  ★ {f.title || "(sin título)"}{" "}
                  <span className="text-muted">· {ENTITY_LABEL[f.entity_type] ?? f.entity_type}</span>
                </Link>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <form
        className="grid gap-3 sm:grid-cols-4 sm:items-end"
        onSubmit={(e) => {
          e.preventDefault();
          set("q", q.trim());
        }}
      >
        <TextField
          label="Buscar"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="sm:col-span-2"
        />
        {!fixedKind && (
          <SelectField label="Tipo" value={kind} onChange={(e) => set("kind", e.target.value)}>
            <option value="">Todos</option>
            {Object.entries(NOTE_KIND).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </SelectField>
        )}
        <SelectField label="Tag" value={tag} onChange={(e) => set("tag", e.target.value)}>
          <option value="">Todos</option>
          {tags.data?.map((t) => (
            <option key={t.tag} value={t.tag}>
              #{t.tag} ({t.count})
            </option>
          ))}
        </SelectField>
        <CheckboxField
          label="Ver archivadas"
          checked={archived}
          onChange={(e) => set("archived", e.target.checked ? "1" : "")}
        />
        <Button type="submit" variant="secondary">
          Filtrar
        </Button>
      </form>

      <QueryState
        query={notes}
        isEmpty={(d) => d.items.length === 0}
        empty={
          <EmptyState
            title={archived ? "No hay notas archivadas" : "Sin notas"}
            action={<Button onClick={newNote}>Crear la primera</Button>}
          >
            Las notas aceptan Markdown, tags y enlaces a cualquier registro.
          </EmptyState>
        }
      >
        {(d) => (
          <>
            <ul className="space-y-2">
              {d.items.map((n) => (
                <li key={n.id}>
                  <Link
                    to={`/notas/${n.id}`}
                    className="block rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-semibold text-fg">{n.title}</span>
                      <span className="flex items-center gap-2">
                        <Badge>{NOTE_KIND[n.kind as NoteKind] ?? n.kind}</Badge>
                        {n.archived_at && <Badge tone="warn">Archivada</Badge>}
                        <span className="text-sm text-muted">
                          {formatShortDate(n.updated_at.slice(0, 10))}
                        </span>
                      </span>
                    </div>
                    {n.body_md && (
                      <p className="mt-1 line-clamp-2 text-sm text-muted">{n.body_md.slice(0, 200)}</p>
                    )}
                    <div className="mt-1">
                      <TagList tags={n.tags} />
                    </div>
                  </Link>
                </li>
              ))}
            </ul>
            {d.total > 30 && (
              <div className="mt-3 flex items-center justify-between">
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={page === 0}
                  onClick={() => setPage((p) => p - 1)}
                >
                  Anterior
                </Button>
                <span className="text-sm text-muted">
                  {page * 30 + 1}–{Math.min((page + 1) * 30, d.total)} de {d.total}
                </span>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={(page + 1) * 30 >= d.total}
                  onClick={() => setPage((p) => p + 1)}
                >
                  Siguiente
                </Button>
              </div>
            )}
          </>
        )}
      </QueryState>
    </div>
  );
}
