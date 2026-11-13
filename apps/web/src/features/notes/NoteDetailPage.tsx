import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import {
  ConflictPrompt,
  FavoriteButton,
  LinksPanel,
  ListField,
  SafeMarkdown,
  useArchiveMutation,
  useAreasAndProjects,
} from "./common";
import {
  NOTE_KIND,
  type Note,
  type NoteInput,
  type NoteKind,
  noteInput,
  useNote,
  useNoteMutations,
} from "./notesApi";

function NoteEditor({ note }: { note: Note }) {
  const { update } = useNoteMutations();
  const { areas, projects } = useAreasAndProjects();
  const toast = useToast();
  const navigate = useNavigate();
  const archive = useArchiveMutation();
  const [form, setForm] = useState<NoteInput>(() => noteInput(note));
  const [mode, setMode] = useState<"edit" | "preview">(note.body_md ? "preview" : "edit");
  const [conflict, setConflict] = useState<Note | null>(null);
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof NoteInput>(k: K, v: NoteInput[K]) => setForm((f) => ({ ...f, [k]: v }));
  const dirty =
    JSON.stringify({ ...form, version: 0 }) !== JSON.stringify({ ...noteInput(note), version: 0 });

  const save = async (data: NoteInput, version: number) => {
    if (!data.title.trim()) return setError("El título es obligatorio");
    setError(null);
    try {
      const saved = await update.mutateAsync({
        id: note.id,
        body: { ...data, title: data.title.trim(), version },
      });
      setForm(noteInput(saved));
      toast.show("Nota guardada", { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as Note);
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  return (
    <div className="space-y-4">
      <PageHeader
        title={note.title}
        subtitle={<span>Actualizada {formatDateTime(note.updated_at)}</span>}
        actions={
          <>
            <FavoriteButton type="note" id={note.id} />
            {note.archived_at ? (
              <Button
                variant="secondary"
                onClick={() => archive.mutate({ type: "note", id: note.id, restore: true })}
              >
                Recuperar
              </Button>
            ) : (
              <Button
                variant="secondary"
                onClick={() =>
                  archive.mutate({ type: "note", id: note.id }, { onSuccess: () => navigate("/notas") })
                }
              >
                Archivar
              </Button>
            )}
            <ButtonLink to="/notas">Volver</ButtonLink>
          </>
        }
      />
      {note.archived_at && <Badge tone="warn">Archivada</Badge>}
      <form
        className="grid gap-4 lg:grid-cols-[2fr_1fr]"
        onSubmit={(e) => {
          e.preventDefault();
          void save(form, note.version);
        }}
      >
        <Card>
          <div className="space-y-3">
            <TextField
              label="Título"
              value={form.title}
              onChange={(e) => set("title", e.target.value)}
              error={error ?? undefined}
            />
            <div className="flex gap-2" role="tablist" aria-label="Modo del editor">
              <Button
                size="sm"
                role="tab"
                aria-selected={mode === "edit"}
                variant={mode === "edit" ? "primary" : "secondary"}
                onClick={() => setMode("edit")}
              >
                Editar
              </Button>
              <Button
                size="sm"
                role="tab"
                aria-selected={mode === "preview"}
                variant={mode === "preview" ? "primary" : "secondary"}
                onClick={() => setMode("preview")}
              >
                Vista previa
              </Button>
            </div>
            {mode === "edit" ? (
              <TextArea
                label={form.kind === "snippet" ? "Código / contenido" : "Contenido (Markdown)"}
                rows={16}
                className="font-mono"
                value={form.body_md}
                onChange={(e) => set("body_md", e.target.value)}
                hint={
                  form.kind === "snippet"
                    ? "Se guarda y muestra como texto; nunca se ejecuta."
                    : "Sin HTML: se muestra de forma segura."
                }
              />
            ) : form.kind === "snippet" ? (
              <pre className="max-h-[60vh] overflow-auto rounded-lg bg-surface-2 p-3 text-sm">
                <code>{form.body_md}</code>
              </pre>
            ) : (
              <div className="min-h-32 rounded-lg border border-line p-3">
                {form.body_md ? (
                  <SafeMarkdown>{form.body_md}</SafeMarkdown>
                ) : (
                  <p className="text-muted">Vacía.</p>
                )}
              </div>
            )}
          </div>
        </Card>
        <div className="space-y-4">
          <Card title="Metadatos">
            <div className="space-y-3">
              <SelectField
                label="Tipo"
                value={form.kind}
                onChange={(e) => set("kind", e.target.value as NoteKind)}
              >
                {Object.entries(NOTE_KIND).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v}
                  </option>
                ))}
              </SelectField>
              {form.kind === "snippet" && (
                <TextField
                  label="Lenguaje"
                  value={form.language ?? ""}
                  onChange={(e) => set("language", e.target.value || null)}
                  placeholder="rust, sql, bash…"
                />
              )}
              <ListField
                label="Tags"
                value={form.tags}
                onChange={(v) => set("tags", v)}
                hint="Ej.: lang/rust, tool/git"
              />
              <ListField label="Alias" value={form.aliases} onChange={(v) => set("aliases", v)} />
              <SelectField
                label="Área"
                value={form.area_id ?? ""}
                onChange={(e) => set("area_id", e.target.value || null)}
              >
                <option value="">Sin área</option>
                {areas.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
              </SelectField>
              <SelectField
                label="Proyecto"
                value={form.project_id ?? ""}
                onChange={(e) => set("project_id", e.target.value || null)}
              >
                <option value="">Sin proyecto</option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </SelectField>
              <Button type="submit" loading={update.isPending} disabled={!dirty} className="w-full">
                {dirty ? "Guardar" : "Guardado"}
              </Button>
            </div>
          </Card>
          <Card>
            <LinksPanel type="note" id={note.id} />
          </Card>
        </div>
      </form>
      <ConflictPrompt
        conflict={conflict}
        mine={form as unknown as Record<string, unknown>}
        labels={{ title: "Título", body_md: "Contenido", tags: "Tags", aliases: "Alias", kind: "Tipo" }}
        onClose={() => setConflict(null)}
        onTakeServer={(srv) => setForm(noteInput(srv))}
        onSave={(data, v) => void save(data as unknown as NoteInput, v)}
      />
    </div>
  );
}

export function NoteDetailPage() {
  const { id = "" } = useParams();
  const q = useNote(id);
  return <QueryState query={q}>{(d) => <NoteEditor key={d.note.id} note={d.note} />}</QueryState>;
}
