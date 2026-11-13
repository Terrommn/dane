import { useState } from "react";
import ReactMarkdown from "react-markdown";
import { Link, useParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { ActivityList } from "./ActivityList";
import {
  PROJECT_STATUS,
  type Project,
  type ProjectLink,
  projectBody,
  useAreas,
  useProject,
  useProjectMutations,
} from "./projectsApi";

/** Solo enlaces http(s); el servidor también lo valida. */
export function isSafeUrl(u: string) {
  return /^https?:\/\/\S+$/i.test(u.trim());
}

function ProjectForm({ project }: { project: Project }) {
  const areas = useAreas();
  const { update } = useProjectMutations();
  const toast = useToast();
  const [form, setForm] = useState(() => ({
    name: project.name,
    area_id: project.area_id ?? "",
    status: project.status,
    description: project.description,
    repo_url: project.repo_url ?? "",
    notes_md: project.notes_md,
    links: (project.links as unknown as ProjectLink[]) ?? [],
  }));
  const [preview, setPreview] = useState(false);
  const [conflict, setConflict] = useState<Project | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const set = <K extends keyof typeof form>(k: K, v: (typeof form)[K]) => setForm((f) => ({ ...f, [k]: v }));
  const payload = () => ({
    name: form.name.trim(),
    area_id: form.area_id || null,
    status: form.status,
    description: form.description,
    repo_url: form.repo_url.trim() || null,
    notes_md: form.notes_md,
    links: form.links.filter((l) => l.url.trim()) as never,
  });

  const submit = async (base: Project) => {
    const errs: Record<string, string> = {};
    if (!form.name.trim()) errs.name = "El nombre es obligatorio";
    if (form.repo_url.trim() && !isSafeUrl(form.repo_url))
      errs.repo_url = "Usa una URL que empiece con https://";
    if (form.links.some((l) => l.url.trim() && !isSafeUrl(l.url)))
      errs.links = "Cada enlace debe empezar con https://";
    setErrors(errs);
    if (Object.keys(errs).length) return;
    try {
      await update.mutateAsync({ id: base.id, body: projectBody(base, payload()) });
      toast.show("Proyecto guardado", { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as Project);
      else if (e instanceof ApiError && e.field) setErrors({ [e.field]: e.message });
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const archived = project.archived_at != null;
  return (
    <form
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        void submit(project);
      }}
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          label="Nombre"
          value={form.name}
          onChange={(e) => set("name", e.target.value)}
          error={errors.name}
        />
        <SelectField label="Área" value={form.area_id} onChange={(e) => set("area_id", e.target.value)}>
          <option value="">Sin área</option>
          {areas.data?.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        <SelectField label="Estado" value={form.status} onChange={(e) => set("status", e.target.value)}>
          {Object.entries(PROJECT_STATUS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Repositorio"
          type="url"
          placeholder="https://github.com/…"
          value={form.repo_url}
          onChange={(e) => set("repo_url", e.target.value)}
          error={errors.repo_url}
        />
      </div>
      <TextArea
        label="Descripción (qué es)"
        rows={3}
        value={form.description}
        onChange={(e) => set("description", e.target.value)}
      />

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium">Enlaces (dashboards, docs)</legend>
        {form.links.map((l, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: lista editable sin id
          <div key={i} className="grid gap-2 sm:grid-cols-[1fr_2fr_auto] sm:items-end">
            <TextField
              label={`Etiqueta ${i + 1}`}
              value={l.label}
              onChange={(e) =>
                set(
                  "links",
                  form.links.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)),
                )
              }
            />
            <TextField
              label={`URL ${i + 1}`}
              type="url"
              value={l.url}
              onChange={(e) =>
                set(
                  "links",
                  form.links.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)),
                )
              }
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={() =>
                set(
                  "links",
                  form.links.filter((_, j) => j !== i),
                )
              }
              aria-label={`Quitar enlace ${i + 1}`}
            >
              Quitar
            </Button>
          </div>
        ))}
        {errors.links && (
          <p role="alert" className="text-sm text-danger">
            {errors.links}
          </p>
        )}
        <Button
          variant="secondary"
          size="sm"
          onClick={() => set("links", [...form.links, { label: "", url: "" }])}
        >
          Agregar enlace
        </Button>
      </fieldset>

      <div>
        <div className="mb-1 flex items-center justify-between">
          <span className="text-sm font-medium">Notas, runbooks y decisiones (Markdown)</span>
          <Button variant="ghost" size="sm" onClick={() => setPreview((p) => !p)} aria-pressed={preview}>
            {preview ? "Editar" : "Vista previa"}
          </Button>
        </div>
        {preview ? (
          <div className="prose max-w-none rounded-lg border border-line bg-surface p-3 text-sm">
            {/* react-markdown no interpreta HTML crudo: el contenido se muestra seguro. */}
            <ReactMarkdown
              urlTransform={(u) => (isSafeUrl(u) || u.startsWith("/") || u.startsWith("#") ? u : "")}
            >
              {form.notes_md || "_Sin notas_"}
            </ReactMarkdown>
          </div>
        ) : (
          <TextArea
            label="Notas"
            className="[&>label]:sr-only"
            rows={8}
            value={form.notes_md}
            onChange={(e) => set("notes_md", e.target.value)}
          />
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        <Button type="submit" loading={update.isPending}>
          Guardar
        </Button>
        <Button
          variant="secondary"
          onClick={async () => {
            try {
              await update.mutateAsync({
                id: project.id,
                body: projectBody(project, { archived: !archived }),
              });
              toast.show(archived ? "Proyecto recuperado" : "Proyecto archivado", { tone: "ok" });
            } catch (e) {
              toast.show(errorMessage(e), { tone: "error" });
            }
          }}
        >
          {archived ? "Recuperar" : "Archivar"}
        </Button>
      </div>

      {conflict && (
        <ConflictDialog
          open
          mine={payload() as Record<string, unknown>}
          theirs={conflict as unknown as Record<string, unknown>}
          labels={{
            name: "Nombre",
            area_id: "Área",
            status: "Estado",
            description: "Descripción",
            repo_url: "Repositorio",
            notes_md: "Notas",
            links: "Enlaces",
          }}
          onClose={() => setConflict(null)}
          onResolve={async (r) => {
            const current = conflict;
            setConflict(null);
            if (r.choice === "theirs") {
              setForm({
                name: current.name,
                area_id: current.area_id ?? "",
                status: current.status,
                description: current.description,
                repo_url: current.repo_url ?? "",
                notes_md: current.notes_md,
                links: (current.links as unknown as ProjectLink[]) ?? [],
              });
              return;
            }
            const body = r.choice === "merged" ? r.body : payload();
            try {
              await update.mutateAsync({ id: current.id, body: projectBody(current, body as never) });
              toast.show("Proyecto guardado", { tone: "ok" });
            } catch (e) {
              toast.show(errorMessage(e), { tone: "error" });
            }
          }}
        />
      )}
    </form>
  );
}

export function ProjectDetailPage() {
  const { id = "" } = useParams();
  const project = useProject(id);
  return (
    <div className="space-y-4">
      <p className="text-sm">
        <Link to="/proyectos">← Áreas y proyectos</Link>
      </p>
      <QueryState query={project}>
        {(p) => (
          <>
            <PageHeader
              title={p.name}
              subtitle={
                <span className="inline-flex gap-2">
                  <Badge tone={p.status === "active" ? "ok" : "muted"}>
                    {PROJECT_STATUS[p.status] ?? p.status}
                  </Badge>
                  {p.archived_at && <Badge>Archivado</Badge>}
                </span>
              }
            />
            <div className="grid gap-4 lg:grid-cols-[2fr_1fr]">
              <Card>
                <ProjectForm key={p.version} project={p} />
              </Card>
              <Card title="Actividad reciente">
                <ActivityList projectId={p.id} />
              </Card>
            </div>
          </>
        )}
      </QueryState>
    </div>
  );
}
