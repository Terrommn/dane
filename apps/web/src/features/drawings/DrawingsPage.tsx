import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { SaveStatus } from "../../components/ui/SaveStatus";
import { EmptyState, LoadingState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import { useArchiveMutation } from "../notes/common";
import { useNotes } from "../notes/notesApi";
import { asScene, MAX_SCENE_BYTES, type Scene, sceneSize } from "./scene";

const ExcalidrawCanvas = lazy(() => import("./ExcalidrawCanvas"));

type Drawing = Schemas["Drawing"];

export function DrawingsPage() {
  const [archived, setArchived] = useState(false);
  const list = useQuery({
    queryKey: ["drawings", archived],
    queryFn: async () => unwrap(await api.GET("/drawings", { params: { query: { archived } } })),
  });
  const navigate = useNavigate();
  const toast = useToast();
  const create = useMutation({
    mutationFn: async () =>
      unwrap(
        await api.POST("/drawings", { body: { title: "Nuevo dibujo", scene: { elements: [] } as never } }),
      ),
    onSuccess: (d) => navigate(`/dibujos/${d.id}`),
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <div className="space-y-4">
      <PageHeader
        title="Dibujos"
        subtitle="Notas visuales con Excalidraw"
        actions={
          <Button onClick={() => create.mutate()} loading={create.isPending}>
            Nuevo dibujo
          </Button>
        }
      />
      <CheckboxField label="Archivados" checked={archived} onChange={(e) => setArchived(e.target.checked)} />
      <QueryState
        query={list}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState title="Sin dibujos" action={<Button onClick={() => create.mutate()}>Crear</Button>} />
        }
      >
        {(items) => (
          <ul className="grid gap-2 sm:grid-cols-2">
            {items.map((d) => (
              <li key={d.id}>
                <Link
                  to={`/dibujos/${d.id}`}
                  className="block rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
                >
                  <span className="font-semibold text-fg">✏️ {d.title}</span>
                  <span className="mt-1 block text-sm text-muted">
                    {formatDateTime(d.updated_at)} {d.note_id && "· enlazado a nota"}{" "}
                    {d.archived_at && <Badge tone="warn">Archivado</Badge>}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
    </div>
  );
}

function DrawingEditor({ drawing }: { drawing: Drawing }) {
  const qc = useQueryClient();
  const toast = useToast();
  const archive = useArchiveMutation();
  const notes = useNotes({ limit: 200 });
  const [title, setTitle] = useState(drawing.title);
  const [noteId, setNoteId] = useState(drawing.note_id ?? "");
  const [scene, setScene] = useState<Scene>(() => asScene(drawing.scene));
  const [version, setVersion] = useState(drawing.version);
  const [dirty, setDirty] = useState(false);
  const [conflict, setConflict] = useState(false);
  const dark = typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  const size = sceneSize(scene);
  const save = useMutation({
    mutationFn: async (overrideVersion?: number) =>
      unwrap(
        await api.PUT("/drawings/{id}", {
          params: { path: { id: drawing.id } },
          body: {
            title: title.trim() || "Dibujo",
            scene: scene as never,
            note_id: noteId || null,
            archived: drawing.archived_at != null,
            version: overrideVersion ?? version,
          },
        }),
      ),
    onSuccess: (d) => {
      setVersion(d.version);
      setDirty(false);
      setConflict(false);
      void qc.invalidateQueries({ queryKey: ["drawings"] });
      toast.show("Dibujo guardado", { tone: "ok" });
    },
    onError: (e) => {
      if (e instanceof ApiError && e.isConflict) setConflict(true);
      else toast.show(errorMessage(e), { tone: "error" });
    },
  });
  return (
    <div className="space-y-3">
      <PageHeader
        title={title || "Dibujo"}
        actions={
          <>
            <SaveStatus
              status={conflict ? "conflict" : dirty ? "draft" : "saved"}
              detail={dirty ? "Sin guardar" : null}
            />
            <Button
              onClick={() => save.mutate(undefined)}
              loading={save.isPending}
              disabled={size > MAX_SCENE_BYTES}
            >
              Guardar
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                archive.mutate({ type: "drawing", id: drawing.id, restore: drawing.archived_at != null })
              }
            >
              {drawing.archived_at ? "Recuperar" : "Archivar"}
            </Button>
            <ButtonLink to="/dibujos">Volver</ButtonLink>
          </>
        }
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          label="Título"
          value={title}
          onChange={(e) => {
            setTitle(e.target.value);
            setDirty(true);
          }}
        />
        <SelectField
          label="Nota enlazada"
          value={noteId}
          onChange={(e) => {
            setNoteId(e.target.value);
            setDirty(true);
          }}
        >
          <option value="">Sin nota</option>
          {(notes.data?.items ?? []).map((n) => (
            <option key={n.id} value={n.id}>
              {n.title}
            </option>
          ))}
        </SelectField>
      </div>
      {noteId && <Link to={`/notas/${noteId}`}>Abrir nota enlazada</Link>}
      {size > MAX_SCENE_BYTES && (
        <p role="alert" className="text-sm text-danger">
          El dibujo es demasiado grande para guardarse ({Math.round(size / 1024)} KB). Quita imágenes pesadas.
        </p>
      )}
      {conflict && (
        <div role="alert" className="rounded-lg border border-warn p-3 text-sm">
          Este dibujo cambió en otro dispositivo.{" "}
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void qc.invalidateQueries({ queryKey: ["drawing", drawing.id] })}
          >
            Cargar la versión del servidor
          </Button>{" "}
          <Button
            size="sm"
            onClick={async () => {
              const fresh = unwrap(await api.GET("/drawings/{id}", { params: { path: { id: drawing.id } } }));
              save.mutate(fresh.version);
            }}
          >
            Conservar el mío
          </Button>
        </div>
      )}
      <Suspense fallback={<LoadingState label="Cargando lienzo…" rows={4} />}>
        <ExcalidrawCanvas
          initial={scene}
          dark={dark}
          onChange={(s) => {
            setScene(s);
            setDirty(true);
          }}
        />
      </Suspense>
    </div>
  );
}

export function DrawingPage() {
  const { id = "" } = useParams();
  const q = useQuery({
    queryKey: ["drawing", id],
    queryFn: async () => unwrap(await api.GET("/drawings/{id}", { params: { path: { id } } })),
  });
  return (
    <QueryState query={q}>{(d) => <DrawingEditor key={`${d.id}:${d.version}`} drawing={d} />}</QueryState>
  );
}
