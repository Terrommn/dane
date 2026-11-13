// Lecturas (Lecturas.base): tarjetas agrupadas por estado y tabla ordenada por rating. Terminar una lectura aporta
// una sola vez a los objetivos enlazados explícitamente (p. ej. "Leer 4 libros"); minutos leídos son un hábito aparte.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { useGoals } from "../goals/goalsApi";

type Reading = Schemas["Reading"];

export const READING_STATUS: Record<string, string> = {
  want: "Por leer",
  reading: "Leyendo",
  finished: "Terminado",
  abandoned: "Abandonado",
};
const ORDER = ["reading", "want", "finished", "abandoned"];

/** Orden de la tabla: rating desc (sin rating al final), luego fecha de fin desc. */
export function byRating(a: Reading, b: Reading): number {
  const ra = a.rating ?? -1;
  const rb = b.rating ?? -1;
  if (ra !== rb) return rb - ra;
  return (b.finished_on ?? "").localeCompare(a.finished_on ?? "");
}

export function ReadingsPage() {
  const [view, setView] = useState<"tarjetas" | "tabla">("tarjetas");
  const [status, setStatus] = useState("");
  const [q, setQ] = useState("");
  const navigate = useNavigate();
  const list = useQuery({
    queryKey: ["readings", status, q],
    queryFn: async () =>
      unwrap(await api.GET("/readings", { params: { query: { status: status || null, q: q || null } } })),
  });
  return (
    <div className="space-y-4">
      <PageHeader
        title="Lecturas"
        actions={<Button onClick={() => navigate("/lecturas/nueva")}>Nueva lectura</Button>}
      />
      <div className="grid gap-2 sm:grid-cols-[1fr_12rem_auto]">
        <TextField label="Buscar" type="search" value={q} onChange={(e) => setQ(e.target.value)} />
        <SelectField label="Estado" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Todos</option>
          {ORDER.map((s) => (
            <option key={s} value={s}>
              {READING_STATUS[s]}
            </option>
          ))}
        </SelectField>
        <fieldset className="flex items-end gap-1">
          <legend className="sr-only">Vista</legend>
          {(["tarjetas", "tabla"] as const).map((v) => (
            <Button
              key={v}
              size="sm"
              variant={view === v ? "primary" : "secondary"}
              aria-pressed={view === v}
              onClick={() => setView(v)}
            >
              {v === "tarjetas" ? "Tarjetas" : "Tabla"}
            </Button>
          ))}
        </fieldset>
      </div>
      <QueryState
        query={list}
        isEmpty={(d) => d.length === 0}
        empty={<EmptyState title="Sin lecturas" action={<Link to="/lecturas/nueva">Agregar una</Link>} />}
      >
        {(items) =>
          view === "tarjetas" ? (
            <div className="space-y-4">
              {ORDER.map((s) => {
                const group = items.filter((r) => r.status === s);
                if (!group.length) return null;
                return (
                  <Card key={s} title={`${READING_STATUS[s]} (${group.length})`}>
                    <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                      {group.map((r) => (
                        <li key={r.id} className="rounded-xl border border-line p-3">
                          <Link to={`/lecturas/${r.id}`} className="font-medium">
                            {r.title}
                          </Link>
                          <p className="text-sm text-muted">
                            {r.author || "—"} · {r.kind === "article" ? "Artículo" : "Libro"}
                            {r.rating ? ` · ${"★".repeat(r.rating)}` : ""}
                          </p>
                        </li>
                      ))}
                    </ul>
                  </Card>
                );
              })}
            </div>
          ) : (
            <Card>
              <table className="w-full text-sm">
                <thead className="text-left text-muted">
                  <tr>
                    <th className="py-1 font-medium">Título</th>
                    <th className="hidden font-medium sm:table-cell">Autor</th>
                    <th className="font-medium">Estado</th>
                    <th className="font-medium">Rating</th>
                    <th className="hidden font-medium sm:table-cell">Fin</th>
                  </tr>
                </thead>
                <tbody>
                  {[...items].sort(byRating).map((r) => (
                    <tr key={r.id} className="border-t border-line">
                      <td className="py-1.5">
                        <Link to={`/lecturas/${r.id}`}>{r.title}</Link>
                      </td>
                      <td className="hidden sm:table-cell">{r.author}</td>
                      <td>{READING_STATUS[r.status]}</td>
                      <td>{r.rating ? `${r.rating}/5` : "—"}</td>
                      <td className="hidden sm:table-cell">{r.finished_on ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
          )
        }
      </QueryState>
    </div>
  );
}

export function ReadingEditorPage() {
  const { id } = useParams();
  const isNew = !id || id === "nueva";
  const reading = useQuery({
    queryKey: ["reading", id],
    queryFn: async () => unwrap(await api.GET("/readings/{id}", { params: { path: { id: id as string } } })),
    enabled: !isNew,
  });
  return (
    <div className="space-y-4">
      <p>
        <Link to="/lecturas">← Lecturas</Link>
      </p>
      {isNew ? (
        <ReadingForm r={null} />
      ) : (
        <QueryState query={reading}>{(r) => <ReadingForm key={r.version} r={r} />}</QueryState>
      )}
    </div>
  );
}

function ReadingForm({ r }: { r: Schemas["ReadingView"] | null }) {
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const qc = useQueryClient();
  const navigate = useNavigate();
  const toast = useToast();
  const goals = useGoals({ status: "active" });
  const [f, setF] = useState({
    kind: r?.kind ?? "book",
    title: r?.title ?? "",
    author: r?.author ?? "",
    url: r?.url ?? "",
    status: r?.status ?? "want",
    rating: r?.rating ? String(r.rating) : "",
    started_on: r?.started_on ?? "",
    finished_on: r?.finished_on ?? "",
    ideas_md: r?.ideas_md ?? "",
    quotes_md: r?.quotes_md ?? "",
    application_md: r?.application_md ?? "",
  });
  const [goalIds, setGoalIds] = useState<string[]>(r?.goal_ids ?? []);
  const [err, setErr] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const save = useMutation({
    mutationFn: async (archived: boolean) => {
      const body: Schemas["ReadingBody"] = {
        kind: f.kind,
        title: f.title.trim(),
        author: f.author,
        url: f.url.trim() || null,
        status: f.status,
        rating: f.rating ? Number(f.rating) : null,
        started_on: f.started_on || null,
        finished_on: f.status === "finished" ? f.finished_on || today : f.finished_on || null,
        ideas_md: f.ideas_md,
        quotes_md: f.quotes_md,
        application_md: f.application_md,
        archived,
        goal_ids: goalIds,
        version: r?.version ?? null,
      };
      return r
        ? unwrap(await api.PUT("/readings/{id}", { params: { path: { id: r.id } }, body }))
        : unwrap(await api.POST("/readings", { body }));
    },
    onSuccess: (saved) => {
      toast.show("Guardado", { tone: "ok" });
      void qc.invalidateQueries({ queryKey: ["readings"] });
      void qc.invalidateQueries({ queryKey: ["reading"] });
      void qc.invalidateQueries({ queryKey: ["goals"] });
      void qc.invalidateQueries({ queryKey: ["dashboard"] });
      if (!r) navigate(`/lecturas/${saved.id}`, { replace: true });
    },
    onError: (e) =>
      setErr(
        e instanceof ApiError && e.isConflict ? "Se editó en otro dispositivo; recarga." : errorMessage(e),
      ),
  });
  const remove = useMutation({
    mutationFn: async () =>
      unwrap(await api.DELETE("/readings/{id}", { params: { path: { id: r?.id as string } } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["readings"] });
      void qc.invalidateQueries({ queryKey: ["goals"] });
      navigate("/lecturas");
    },
  });
  const cumulative = (goals.data ?? []).filter(
    (g) => g.goal.progress_type === "cumulative" || goalIds.includes(g.goal.id),
  );

  return (
    <>
      <PageHeader
        title={r ? r.title : "Nueva lectura"}
        actions={r?.archived_at ? <Badge>archivada</Badge> : undefined}
      />
      <Card>
        <form
          className="grid gap-3 sm:grid-cols-2"
          onSubmit={(e) => {
            e.preventDefault();
            setErr(null);
            if (!f.title.trim()) return setErr("Escribe el título");
            save.mutate(r?.archived_at != null);
          }}
        >
          <TextField
            className="sm:col-span-2"
            label="Título"
            value={f.title}
            onChange={(e) => setF({ ...f, title: e.target.value })}
          />
          <TextField
            label="Autor"
            value={f.author}
            onChange={(e) => setF({ ...f, author: e.target.value })}
          />
          <SelectField label="Tipo" value={f.kind} onChange={(e) => setF({ ...f, kind: e.target.value })}>
            <option value="book">Libro</option>
            <option value="article">Artículo</option>
          </SelectField>
          <SelectField
            label="Estado"
            value={f.status}
            onChange={(e) => setF({ ...f, status: e.target.value })}
          >
            {ORDER.map((s) => (
              <option key={s} value={s}>
                {READING_STATUS[s]}
              </option>
            ))}
          </SelectField>
          <SelectField
            label="Rating"
            value={f.rating}
            onChange={(e) => setF({ ...f, rating: e.target.value })}
          >
            <option value="">Sin rating</option>
            {[1, 2, 3, 4, 5].map((n) => (
              <option key={n} value={n}>
                {n} / 5
              </option>
            ))}
          </SelectField>
          <TextField
            label="Inicio"
            type="date"
            value={f.started_on}
            onChange={(e) => setF({ ...f, started_on: e.target.value })}
          />
          <TextField
            label="Fin"
            type="date"
            value={f.finished_on}
            onChange={(e) => setF({ ...f, finished_on: e.target.value })}
            hint={f.status === "finished" && !f.finished_on ? "Si lo dejas vacío se usa hoy" : undefined}
          />
          <TextField
            className="sm:col-span-2"
            label="URL (artículos)"
            type="url"
            value={f.url}
            onChange={(e) => setF({ ...f, url: e.target.value })}
          />
          <TextArea
            className="sm:col-span-2"
            label="Ideas clave"
            value={f.ideas_md}
            onChange={(e) => setF({ ...f, ideas_md: e.target.value })}
          />
          <TextArea
            className="sm:col-span-2"
            label="Citas"
            value={f.quotes_md}
            onChange={(e) => setF({ ...f, quotes_md: e.target.value })}
          />
          <TextArea
            className="sm:col-span-2"
            label="Qué voy a aplicar"
            value={f.application_md}
            onChange={(e) => setF({ ...f, application_md: e.target.value })}
          />
          <fieldset className="sm:col-span-2">
            <legend className="mb-1 text-sm font-medium">Aporta a objetivos (al terminarla)</legend>
            {cumulative.length === 0 ? (
              <p className="text-sm text-muted">
                No hay objetivos acumulativos activos (p. ej. "Leer 4 libros").
              </p>
            ) : (
              <ul className="space-y-1">
                {cumulative.map((g) => (
                  <li key={g.goal.id}>
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        className="h-5 w-5"
                        checked={goalIds.includes(g.goal.id)}
                        onChange={(e) =>
                          setGoalIds(
                            e.target.checked
                              ? [...goalIds, g.goal.id]
                              : goalIds.filter((x) => x !== g.goal.id),
                          )
                        }
                      />
                      {g.goal.title}{" "}
                      <span className="text-muted">
                        ({g.goal.period_key} · {g.goal.unit})
                      </span>
                    </label>
                  </li>
                ))}
              </ul>
            )}
            <p className="mt-1 text-xs text-muted">
              Aporta +1 una sola vez a cada objetivo marcado; si deja de estar terminada, se retira.
            </p>
          </fieldset>
          {err && (
            <p role="alert" className="text-sm text-danger sm:col-span-2">
              {err}
            </p>
          )}
          <div className="flex flex-wrap gap-2 sm:col-span-2">
            <Button type="submit" loading={save.isPending}>
              Guardar
            </Button>
            {r && (
              <>
                <Button variant="secondary" onClick={() => save.mutate(r.archived_at == null)}>
                  {r.archived_at ? "Recuperar" : "Archivar"}
                </Button>
                <Button variant="danger" onClick={() => setConfirmDelete(true)}>
                  Eliminar
                </Button>
              </>
            )}
          </div>
        </form>
      </Card>
      <ConfirmDialog
        open={confirmDelete}
        title="¿Eliminar esta lectura?"
        message="También se retira su aporte a objetivos."
        confirmLabel="Eliminar"
        danger
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          remove.mutate();
        }}
      />
    </>
  );
}
