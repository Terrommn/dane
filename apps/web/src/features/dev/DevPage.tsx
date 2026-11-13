import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { DatePicker } from "../../components/ui/DatePicker";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { SafeMarkdown, useAreasAndProjects } from "../notes/common";
import { NotesPage } from "../notes/NotesPage";
import {
  ADR_STATUS,
  adrCode,
  type DevSession,
  type SessionBody,
  sessionBody,
  useAdrMutations,
  useAdrs,
  useDevlogDay,
  useDevMutations,
} from "./devApi";

type Tab = "devlog" | "adrs" | "runbook" | "til" | "snippet";

const EMPTY_SESSION: SessionBody = {
  repo: "",
  branch: "",
  at_time: null,
  changes_md: "",
  bugs_md: "",
  decisions_md: "",
  next_steps_md: "",
  project_ids: [],
};

function SessionForm({
  initial,
  onSave,
  loading,
  onCancel,
}: {
  initial: SessionBody;
  onSave: (b: SessionBody) => void;
  loading?: boolean;
  onCancel?: () => void;
}) {
  const { projects } = useAreasAndProjects();
  const [f, setF] = useState(initial);
  const set = <K extends keyof SessionBody>(k: K, v: SessionBody[K]) => setF((x) => ({ ...x, [k]: v }));
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(f);
      }}
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <TextField label="Repositorio" value={f.repo ?? ""} onChange={(e) => set("repo", e.target.value)} />
        <TextField label="Rama" value={f.branch ?? ""} onChange={(e) => set("branch", e.target.value)} />
        <TextField
          label="Hora"
          type="time"
          value={f.at_time?.slice(0, 5) ?? ""}
          onChange={(e) => set("at_time", e.target.value || null)}
        />
      </div>
      <fieldset>
        <legend className="mb-1 text-sm font-medium">Proyectos</legend>
        <div className="flex flex-wrap gap-2">
          {projects.map((p) => (
            <label
              key={p.id}
              className="inline-flex items-center gap-1 rounded-full border border-line px-3 py-1 text-sm"
            >
              <input
                type="checkbox"
                checked={f.project_ids?.includes(p.id) ?? false}
                onChange={(e) =>
                  set(
                    "project_ids",
                    e.target.checked
                      ? [...(f.project_ids ?? []), p.id]
                      : (f.project_ids ?? []).filter((x) => x !== p.id),
                  )
                }
              />
              {p.name}
            </label>
          ))}
        </div>
      </fieldset>
      <TextArea
        label="Qué hice / cambios"
        rows={4}
        value={f.changes_md ?? ""}
        onChange={(e) => set("changes_md", e.target.value)}
      />
      <TextArea
        label="Bloqueos / bugs"
        rows={3}
        value={f.bugs_md ?? ""}
        onChange={(e) => set("bugs_md", e.target.value)}
      />
      <TextArea
        label="Decisiones (→ ADR si es grande)"
        rows={3}
        value={f.decisions_md ?? ""}
        onChange={(e) => set("decisions_md", e.target.value)}
      />
      <TextArea
        label="Siguientes pasos"
        rows={3}
        value={f.next_steps_md ?? ""}
        onChange={(e) => set("next_steps_md", e.target.value)}
      />
      <div className="flex justify-end gap-2">
        {onCancel && (
          <Button variant="secondary" onClick={onCancel}>
            Cancelar
          </Button>
        )}
        <Button type="submit" loading={loading}>
          Guardar sesión
        </Button>
      </div>
    </form>
  );
}

function SessionCard({ s, date }: { s: DevSession; date: string }) {
  const m = useDevMutations(date);
  const toast = useToast();
  const { projects } = useAreasAndProjects();
  const [editing, setEditing] = useState(false);
  if (editing)
    return (
      <Card title="Editar sesión">
        <SessionForm
          initial={sessionBody(s)}
          loading={m.updateSession.isPending}
          onCancel={() => setEditing(false)}
          onSave={(b) =>
            m.updateSession.mutate(
              { id: s.id, body: { ...b, version: s.version } },
              {
                onSuccess: () => setEditing(false),
                onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
              },
            )
          }
        />
      </Card>
    );
  const section = (title: string, md: string) =>
    md.trim() ? (
      <div>
        <h4 className="font-semibold">{title}</h4>
        <SafeMarkdown>{md}</SafeMarkdown>
      </div>
    ) : null;
  return (
    <Card
      title={
        <span>
          {s.repo || "Sesión"} {s.branch && <span className="font-normal text-muted">· {s.branch}</span>}{" "}
          {s.at_time && <span className="font-normal text-muted">· {s.at_time.slice(0, 5)}</span>}
        </span>
      }
      actions={
        <span className="flex gap-2">
          <Button size="sm" variant="secondary" onClick={() => setEditing(true)}>
            Editar
          </Button>
          <Button size="sm" variant="ghost" onClick={() => m.deleteSession.mutate(s.id)}>
            Eliminar
          </Button>
        </span>
      }
    >
      <div className="mb-2 flex flex-wrap gap-1">
        {s.project_ids.map((id) => (
          <Badge key={id} tone="accent">
            {projects.find((p) => p.id === id)?.name ?? "proyecto"}
          </Badge>
        ))}
        {s.raw_text && <Badge>importada</Badge>}
      </div>
      <div className="space-y-2">
        {section("Qué hice", s.changes_md)}
        {section("Bloqueos / bugs", s.bugs_md)}
        {section("Decisiones", s.decisions_md)}
        {section("Siguiente", s.next_steps_md)}
      </div>
    </Card>
  );
}

function ImportDialog({ date, onClose }: { date: string; onClose: () => void }) {
  const m = useDevMutations(date);
  const toast = useToast();
  const [text, setText] = useState("");
  const p = m.preview.data;
  return (
    <Dialog
      open
      size="lg"
      onClose={onClose}
      title="Importar resumen de sesión"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={() => m.preview.mutate(text)}
            disabled={!text.trim()}
            loading={m.preview.isPending}
          >
            Vista previa
          </Button>
          <Button
            disabled={!p}
            loading={m.importText.isPending}
            onClick={() =>
              m.importText.mutate(text, {
                onSuccess: () => {
                  toast.show("Sesión importada", { tone: "ok" });
                  onClose();
                },
                onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
              })
            }
          >
            Guardar
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <p className="text-sm text-muted">
          Pega el resumen de una sesión de código (equivalente a /log-session). La web no lee tus
          repositorios: solo el texto que pegues, que se trata como datos.
        </p>
        <TextArea
          label="Texto"
          rows={10}
          className="font-mono"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        {m.preview.isError && <p className="text-sm text-danger">{errorMessage(m.preview.error)}</p>}
        {p && (
          <Card title="Así se guardará">
            <dl className="grid gap-1 text-sm sm:grid-cols-[8rem_1fr]">
              <dt className="text-muted">Repositorio</dt>
              <dd>{p.parsed.repo || "—"}</dd>
              <dt className="text-muted">Rama</dt>
              <dd>{p.parsed.branch || "—"}</dd>
              <dt className="text-muted">Hora</dt>
              <dd>{p.parsed.at_time ?? "—"}</dd>
            </dl>
            {p.unknown_projects.length > 0 && (
              <p className="mt-2 text-sm text-warn">
                Proyectos no reconocidos (no se crean): {p.unknown_projects.join(", ")}
              </p>
            )}
            {(["changes_md", "bugs_md", "decisions_md", "next_steps_md"] as const).map((k) =>
              p.parsed[k] ? (
                <div key={k} className="mt-2">
                  <SafeMarkdown>{p.parsed[k]}</SafeMarkdown>
                </div>
              ) : null,
            )}
          </Card>
        )}
      </div>
    </Dialog>
  );
}

function DevlogView() {
  const { me } = useAuth();
  const [params, setParams] = useSearchParams();
  const date = params.get("date") ?? todayIso(me?.settings.timezone ?? undefined);
  const day = useDevlogDay(date);
  const m = useDevMutations(date);
  const toast = useToast();
  const [adding, setAdding] = useState(false);
  const [importing, setImporting] = useState(false);
  const [summary, setSummary] = useState<string | null>(null);
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <DatePicker
          value={date}
          onChange={(d) => {
            const p = new URLSearchParams(params);
            p.set("date", d);
            setParams(p, { replace: true });
            setSummary(null);
          }}
        />
        <div className="flex gap-2">
          <Button variant="secondary" onClick={() => setImporting(true)}>
            Pegar resumen
          </Button>
          <Button onClick={() => setAdding(true)}>Nueva sesión</Button>
        </div>
      </div>
      <QueryState query={day}>
        {(d) => (
          <>
            <Card title={`Devlog ${formatShortDate(date)}`}>
              <TextArea
                label="Resumen del día"
                rows={3}
                value={summary ?? d.devlog?.summary_md ?? ""}
                onChange={(e) => setSummary(e.target.value)}
              />
              {summary !== null && (
                <Button
                  className="mt-2"
                  size="sm"
                  loading={m.summary.isPending}
                  onClick={() =>
                    m.summary.mutate(
                      { summary_md: summary, version: d.devlog?.version ?? null },
                      {
                        onSuccess: () => setSummary(null),
                        onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
                      },
                    )
                  }
                >
                  Guardar resumen
                </Button>
              )}
            </Card>
            {adding && (
              <Card title="Nueva sesión">
                <SessionForm
                  initial={EMPTY_SESSION}
                  loading={m.addSession.isPending}
                  onCancel={() => setAdding(false)}
                  onSave={(b) =>
                    m.addSession.mutate(b, {
                      onSuccess: () => setAdding(false),
                      onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
                    })
                  }
                />
              </Card>
            )}
            {d.sessions.length === 0 && !adding ? (
              <EmptyState title="Sin sesiones este día" />
            ) : (
              d.sessions.map((s) => <SessionCard key={`${s.id}:${s.version}`} s={s} date={date} />)
            )}
          </>
        )}
      </QueryState>
      {importing && <ImportDialog date={date} onClose={() => setImporting(false)} />}
    </div>
  );
}

function AdrsView() {
  const { projects } = useAreasAndProjects();
  const [project, setProject] = useState("");
  const [status, setStatus] = useState("");
  const adrs = useAdrs(project, status);
  const { create } = useAdrMutations();
  const navigate = useNavigate();
  const toast = useToast();
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [newProject, setNewProject] = useState("");
  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end">
        <SelectField label="Proyecto" value={project} onChange={(e) => setProject(e.target.value)}>
          <option value="">Todos</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </SelectField>
        <SelectField label="Estado" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Todos</option>
          {Object.entries(ADR_STATUS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
        <Button onClick={() => setCreating(true)}>Nuevo ADR</Button>
      </div>
      <QueryState query={adrs} isEmpty={(d) => d.length === 0} empty={<EmptyState title="Sin ADRs" />}>
        {(list) => (
          <ul className="space-y-2">
            {list.map((a) => (
              <li key={a.id}>
                <Link
                  to={`/dev/adr/${a.id}`}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
                >
                  <span className="text-fg">
                    <span className="font-mono text-muted">{adrCode(a.number)}</span> {a.title}
                  </span>
                  <span className="flex items-center gap-2 text-sm text-muted">
                    {projects.find((p) => p.id === a.project_id)?.name}
                    <Badge
                      tone={a.status === "accepted" ? "ok" : a.status === "superseded" ? "warn" : "muted"}
                    >
                      {ADR_STATUS[a.status] ?? a.status}
                    </Badge>
                    {formatShortDate(a.decided_on)}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
      <Dialog
        open={creating}
        onClose={() => setCreating(false)}
        title="Nuevo ADR"
        footer={
          <Button
            disabled={!title.trim() || !newProject}
            loading={create.isPending}
            onClick={() =>
              create.mutate(
                { project_id: newProject, title: title.trim() },
                {
                  onSuccess: (a) => navigate(`/dev/adr/${a.id}`),
                  onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
                },
              )
            }
          >
            Crear (número automático)
          </Button>
        }
      >
        <div className="space-y-3">
          <SelectField label="Proyecto" value={newProject} onChange={(e) => setNewProject(e.target.value)}>
            <option value="">Elige…</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </SelectField>
          <TextField label="Título" value={title} onChange={(e) => setTitle(e.target.value)} />
        </div>
      </Dialog>
    </div>
  );
}

export function DevPage() {
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") as Tab) || "devlog";
  const tabs: [Tab, string][] = [
    ["devlog", "Devlog"],
    ["adrs", "ADRs"],
    ["runbook", "Runbooks"],
    ["til", "TILs"],
    ["snippet", "Snippets"],
  ];
  return (
    <div className="space-y-4">
      <PageHeader
        title="Desarrollo"
        subtitle="Devlog de sesiones, decisiones de arquitectura, runbooks, TILs y snippets"
      />
      <div role="tablist" aria-label="Sección" className="flex flex-wrap gap-2">
        {tabs.map(([k, l]) => (
          <Button
            key={k}
            role="tab"
            size="sm"
            aria-selected={tab === k}
            variant={tab === k ? "primary" : "secondary"}
            onClick={() => setParams({ tab: k }, { replace: true })}
          >
            {l}
          </Button>
        ))}
      </div>
      {tab === "devlog" && <DevlogView />}
      {tab === "adrs" && <AdrsView />}
      {(tab === "runbook" || tab === "til" || tab === "snippet") && <NotesPage key={tab} fixedKind={tab} />}
    </div>
  );
}
