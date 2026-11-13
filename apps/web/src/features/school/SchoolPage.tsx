import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { errorMessage, unwrap } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { LinksPanel, SafeMarkdown, useAreasAndProjects } from "../notes/common";
import { PRIORITY } from "../tasks/tasksApi";

type Subject = Schemas["Subject"];
type ClassNote = Schemas["ClassNote"];

function useSubjects() {
  return useQuery({
    queryKey: ["subjects"],
    queryFn: async () => unwrap(await api.GET("/subjects", { params: { query: {} } })),
  });
}
function useClassNotes(subjectId: string) {
  return useQuery({
    queryKey: ["class-notes", subjectId],
    queryFn: async () =>
      unwrap(await api.GET("/class-notes", { params: { query: { subject_id: subjectId || undefined } } })),
  });
}

function SubjectDialog({ subject, onClose }: { subject: Subject | null; onClose: () => void }) {
  const { areas } = useAreasAndProjects();
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState(subject?.name ?? "");
  const [areaId, setAreaId] = useState(subject?.area_id ?? "");
  const [archived, setArchived] = useState(subject?.archived_at != null);
  const save = useMutation({
    mutationFn: async () => {
      const body = {
        name: name.trim(),
        area_id: areaId || null,
        archived,
        version: subject?.version ?? null,
      };
      return subject
        ? unwrap(await api.PUT("/subjects/{id}", { params: { path: { id: subject.id } }, body }))
        : unwrap(await api.POST("/subjects", { body }));
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["subjects"] });
      onClose();
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={subject ? "Editar materia" : "Nueva materia"}
      footer={
        <Button onClick={() => save.mutate()} disabled={!name.trim()} loading={save.isPending}>
          Guardar
        </Button>
      }
    >
      <div className="space-y-3">
        <TextField label="Materia" value={name} onChange={(e) => setName(e.target.value)} />
        <SelectField
          label="Área"
          value={areaId}
          onChange={(e) => setAreaId(e.target.value)}
          hint="Por defecto, Escuela"
        >
          <option value="">Escuela (por defecto)</option>
          {areas.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        {subject && (
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={archived} onChange={(e) => setArchived(e.target.checked)} />{" "}
            Archivada
          </label>
        )}
      </div>
    </Dialog>
  );
}

function NewClassDialog({
  subjects,
  defaultSubject,
  onClose,
}: {
  subjects: Subject[];
  defaultSubject: string;
  onClose: () => void;
}) {
  const { me } = useAuth();
  const navigate = useNavigate();
  const toast = useToast();
  const [subjectId, setSubjectId] = useState(defaultSubject || subjects[0]?.id || "");
  const [topic, setTopic] = useState("");
  const [date, setDate] = useState(todayIso(me?.settings.timezone ?? undefined));
  const create = useMutation({
    mutationFn: async () =>
      unwrap(
        await api.POST("/class-notes", {
          body: { subject_id: subjectId, topic: topic.trim(), on_date: date },
        }),
      ),
    onSuccess: (n) => navigate(`/escuela/clase/${n.id}`),
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title="Nueva clase"
      footer={
        <Button
          onClick={() => create.mutate()}
          disabled={!topic.trim() || !subjectId}
          loading={create.isPending}
        >
          Crear
        </Button>
      }
    >
      <div className="space-y-3">
        <SelectField label="Materia" value={subjectId} onChange={(e) => setSubjectId(e.target.value)}>
          {subjects.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </SelectField>
        <TextField label="Tema" value={topic} onChange={(e) => setTopic(e.target.value)} />
        <TextField label="Fecha" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
      </div>
    </Dialog>
  );
}

export function SchoolPage() {
  const subjects = useSubjects();
  const [subjectId, setSubjectId] = useState("");
  const notes = useClassNotes(subjectId);
  const [editing, setEditing] = useState<Subject | "new" | null>(null);
  const [newClass, setNewClass] = useState(false);
  return (
    <div className="space-y-4">
      <PageHeader
        title="Escuela"
        subtitle="Materias, notas de clase, dudas y entregas"
        actions={
          <>
            <Button variant="secondary" onClick={() => setEditing("new")}>
              Nueva materia
            </Button>
            <Button onClick={() => setNewClass(true)} disabled={!subjects.data?.length}>
              Nueva clase
            </Button>
          </>
        }
      />
      <QueryState
        query={subjects}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Agrega tu primera materia"
            action={<Button onClick={() => setEditing("new")}>Nueva materia</Button>}
          />
        }
      >
        {(subs) => (
          <>
            <ul className="flex flex-wrap gap-2" aria-label="Materias">
              <li>
                <Button
                  size="sm"
                  variant={subjectId === "" ? "primary" : "secondary"}
                  onClick={() => setSubjectId("")}
                >
                  Todas
                </Button>
              </li>
              {subs.map((s) => (
                <li key={s.id} className="flex">
                  <Button
                    size="sm"
                    variant={subjectId === s.id ? "primary" : "secondary"}
                    onClick={() => setSubjectId(s.id)}
                  >
                    {s.name} <span className="text-xs opacity-80">({s.note_count})</span>
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`Editar ${s.name}`}
                    onClick={() => setEditing(s)}
                  >
                    ✎
                  </Button>
                </li>
              ))}
            </ul>
            <QueryState
              query={notes}
              isEmpty={(d) => d.length === 0}
              empty={<EmptyState title="Sin notas de clase" />}
            >
              {(list) => (
                <ul className="space-y-2">
                  {list.map((n) => (
                    <li key={n.id}>
                      <Link
                        to={`/escuela/clase/${n.id}`}
                        className="block rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
                      >
                        <span className="flex flex-wrap items-center justify-between gap-2">
                          <span className="font-semibold text-fg">{n.topic}</span>
                          <span className="text-sm text-muted">
                            {n.subject_name} · {formatShortDate(n.on_date)}
                          </span>
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              )}
            </QueryState>
            {newClass && (
              <NewClassDialog subjects={subs} defaultSubject={subjectId} onClose={() => setNewClass(false)} />
            )}
          </>
        )}
      </QueryState>
      {editing && (
        <SubjectDialog subject={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
    </div>
  );
}

function ClassEditor({ note, deliverables }: { note: ClassNote; deliverables: Schemas["Task"][] }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [f, setF] = useState({
    topic: note.topic,
    on_date: note.on_date,
    topics_md: note.topics_md,
    questions_md: note.questions_md,
  });
  const [deliv, setDeliv] = useState({ title: "", due_on: "", priority: "" });
  const [preview, setPreview] = useState(true);
  const refresh = () => qc.invalidateQueries({ queryKey: ["class-note", note.id] });
  const save = useMutation({
    mutationFn: async () =>
      unwrap(
        await api.PUT("/class-notes/{id}", {
          params: { path: { id: note.id } },
          body: { subject_id: note.subject_id, ...f, version: note.version },
        }),
      ),
    onSuccess: () => {
      void refresh();
      void qc.invalidateQueries({ queryKey: ["class-notes"] });
      toast.show("Clase guardada", { tone: "ok" });
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  const addDeliverable = useMutation({
    mutationFn: async () =>
      unwrap(
        await api.POST("/class-notes/{id}/deliverables", {
          params: { path: { id: note.id } },
          body: {
            title: deliv.title.trim(),
            due_on: deliv.due_on || null,
            priority: deliv.priority ? Number(deliv.priority) : null,
          },
        }),
      ),
    onSuccess: () => {
      setDeliv({ title: "", due_on: "", priority: "" });
      void refresh();
      void qc.invalidateQueries({ queryKey: ["tasks"] });
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <div className="space-y-4">
      <PageHeader
        title={note.topic}
        subtitle={`${note.subject_name} · ${formatShortDate(note.on_date)}`}
        actions={
          <>
            <Button variant="secondary" onClick={() => setPreview((p) => !p)}>
              {preview ? "Editar" : "Vista previa"}
            </Button>
            <Button onClick={() => save.mutate()} loading={save.isPending}>
              Guardar
            </Button>
            <ButtonLink to="/escuela">Volver</ButtonLink>
          </>
        }
      />
      <Card>
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField label="Tema" value={f.topic} onChange={(e) => setF({ ...f, topic: e.target.value })} />
          <TextField
            label="Fecha"
            type="date"
            value={f.on_date}
            onChange={(e) => setF({ ...f, on_date: e.target.value })}
          />
        </div>
      </Card>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card title="Temas">
          {preview ? (
            f.topics_md ? (
              <SafeMarkdown>{f.topics_md}</SafeMarkdown>
            ) : (
              <p className="text-muted">Sin temas.</p>
            )
          ) : (
            <TextArea
              label="Temas (Markdown)"
              rows={10}
              value={f.topics_md}
              onChange={(e) => setF({ ...f, topics_md: e.target.value })}
            />
          )}
        </Card>
        <Card title="Dudas">
          {preview ? (
            f.questions_md ? (
              <SafeMarkdown>{f.questions_md}</SafeMarkdown>
            ) : (
              <p className="text-muted">Sin dudas.</p>
            )
          ) : (
            <TextArea
              label="Dudas (Markdown)"
              rows={10}
              value={f.questions_md}
              onChange={(e) => setF({ ...f, questions_md: e.target.value })}
            />
          )}
        </Card>
      </div>
      <Card title="Tareas / entregas">
        <ul className="divide-y divide-line">
          {deliverables.length === 0 && <li className="py-2 text-sm text-muted">Sin entregas.</li>}
          {deliverables.map((t) => (
            <li key={t.id} className="flex flex-wrap items-center justify-between gap-2 py-2">
              <Link to={`/tareas?task=${t.id}`} className={t.status === "done" ? "line-through" : ""}>
                {t.title}
              </Link>
              <span className="flex gap-1">
                {t.due_on && <Badge tone="warn">entrega {formatShortDate(t.due_on)}</Badge>}
                {t.status === "done" && <Badge tone="ok">Hecha</Badge>}
              </span>
            </li>
          ))}
        </ul>
        <form
          className="mt-3 grid gap-2 sm:grid-cols-[1fr_10rem_8rem_auto] sm:items-end"
          onSubmit={(e) => {
            e.preventDefault();
            if (deliv.title.trim()) addDeliverable.mutate();
          }}
        >
          <TextField
            label="Nueva entrega"
            value={deliv.title}
            onChange={(e) => setDeliv({ ...deliv, title: e.target.value })}
          />
          <TextField
            label="Fecha de entrega"
            type="date"
            value={deliv.due_on}
            onChange={(e) => setDeliv({ ...deliv, due_on: e.target.value })}
          />
          <SelectField
            label="Prioridad"
            value={deliv.priority}
            onChange={(e) => setDeliv({ ...deliv, priority: e.target.value })}
          >
            <option value="">—</option>
            {Object.entries(PRIORITY).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </SelectField>
          <Button type="submit" loading={addDeliverable.isPending} disabled={!deliv.title.trim()}>
            Agregar
          </Button>
        </form>
        <p className="mt-2 text-sm text-muted">
          Cada entrega es una tarea del área Escuela y aparece en Hoy el día que toca.
        </p>
      </Card>
      <Card>
        <LinksPanel type="class_note" id={note.id} />
      </Card>
    </div>
  );
}

export function ClassNotePage() {
  const { id = "" } = useParams();
  const q = useQuery({
    queryKey: ["class-note", id],
    queryFn: async () => unwrap(await api.GET("/class-notes/{id}", { params: { path: { id } } })),
  });
  return (
    <QueryState query={q}>
      {(d) => (
        <ClassEditor key={`${d.note.id}:${d.note.version}`} note={d.note} deliverables={d.deliverables} />
      )}
    </QueryState>
  );
}
