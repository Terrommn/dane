import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { SaveStatus } from "../../components/ui/SaveStatus";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import { defaultSender, useDraft, useSyncQueue } from "../../lib/offline/hooks";
import { enqueue, flushQueue } from "../../lib/offline/store";
import { useAuth } from "../auth/AuthProvider";
import { entityHref, ListField, useAreasAndProjects } from "../notes/common";
import {
  ACTION_LABEL,
  actionFromProposal,
  INBOX_STATUS,
  type InboxAction,
  type InboxItem,
  type Proposal,
  useInbox,
  useInboxMutations,
} from "./inboxApi";

/** Captura rápida: el texto se guarda como borrador local mientras escribes y se encola (con client_id) al enviar. */
function QuickCapture() {
  const { userId } = useAuth();
  const { draft, loaded, saveDebounced, clear } = useDraft<{ text: string }>("inbox", "quick");
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const qc = useQueryClient();
  const toast = useToast();
  // biome-ignore lint/correctness/useExhaustiveDependencies: solo restaurar el borrador una vez, al cargarlo
  useEffect(() => {
    if (loaded && draft?.data?.text && !text) setText(draft.data.text);
  }, [loaded]);

  const send = async () => {
    const t = text.trim();
    if (!t || !userId) return;
    setSending(true);
    try {
      const clientId = crypto.randomUUID();
      // La cola persiste en IndexedDB: si no hay red se envía al reconectar; el client_id evita duplicados.
      await enqueue(userId, {
        method: "POST",
        path: "/inbox",
        body: { text: t, client_id: clientId, source: "app" },
        id: clientId,
      });
      setText("");
      await clear();
      const r = await flushQueue(userId, defaultSender);
      if (r.sent > 0) toast.show("Capturado en el Inbox", { tone: "ok" });
      else toast.show("Guardado en este dispositivo; se enviará al reconectar", { tone: "info" });
      void qc.invalidateQueries({ queryKey: ["inbox"] });
    } finally {
      setSending(false);
    }
  };

  return (
    <Card title="Captura rápida">
      <form
        className="space-y-2"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <TextArea
          label="Lo que se te ocurra"
          rows={2}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            saveDebounced({ text: e.target.value }, null);
          }}
          hint="Escribe primero, organiza después. Funciona sin conexión."
        />
        <div className="flex flex-wrap items-center justify-between gap-2">
          {text.trim() ? <SaveStatus status="draft" /> : <span />}
          <Button type="submit" loading={sending} disabled={!text.trim()}>
            Capturar
          </Button>
        </div>
      </form>
    </Card>
  );
}

/** Capturas todavía en la cola local (no confirmadas por el servidor). */
function QueuedCaptures() {
  const q = useSyncQueue();
  const queued = q.items.filter((i) => i.path === "/inbox" && i.method === "POST");
  if (!queued.length) return null;
  return (
    <Card title="Pendientes de sincronizar">
      <ul className="space-y-2">
        {queued.map((i) => (
          <li key={i.id} className="flex flex-wrap items-center justify-between gap-2">
            <span className="min-w-0">{(i.body as { text?: string })?.text}</span>
            <span className="flex items-center gap-2">
              <SaveStatus
                status={i.status === "pending" ? "pending" : i.status === "conflict" ? "conflict" : "error"}
              />
              {i.status === "error" && (
                <Button size="sm" variant="ghost" onClick={() => void q.drop(i.id)}>
                  Descartar
                </Button>
              )}
            </span>
          </li>
        ))}
      </ul>
      <Button
        className="mt-3"
        size="sm"
        variant="secondary"
        onClick={() => void q.flush()}
        disabled={!q.online}
        loading={q.syncing}
      >
        {q.online ? "Sincronizar ahora" : "Sin conexión"}
      </Button>
    </Card>
  );
}

function ActionEditor({ action, onChange }: { action: InboxAction; onChange: (a: InboxAction) => void }) {
  const { areas, projects } = useAreasAndProjects();
  const areaSelect = (value: string | null | undefined, set: (v: string | null) => void) => (
    <SelectField label="Área" value={value ?? ""} onChange={(e) => set(e.target.value || null)}>
      <option value="">Sin área</option>
      {areas.map((a) => (
        <option key={a.id} value={a.id}>
          {a.name}
        </option>
      ))}
    </SelectField>
  );
  if (action.type === "task")
    return (
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          className="sm:col-span-2"
          label="Tarea"
          value={action.title}
          onChange={(e) => onChange({ ...action, title: e.target.value })}
        />
        {areaSelect(action.area_id, (v) => onChange({ ...action, area_id: v }))}
        <SelectField
          label="Proyecto"
          value={action.project_id ?? ""}
          onChange={(e) => onChange({ ...action, project_id: e.target.value || null })}
        >
          <option value="">Sin proyecto</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Para el día"
          type="date"
          value={action.planned_on ?? ""}
          onChange={(e) => onChange({ ...action, planned_on: e.target.value || null })}
        />
        <TextField
          label="Fecha límite"
          type="date"
          value={action.due_on ?? ""}
          onChange={(e) => onChange({ ...action, due_on: e.target.value || null })}
        />
      </div>
    );
  if (action.type === "note")
    return (
      <div className="space-y-3">
        <TextField
          label="Título"
          value={action.title}
          onChange={(e) => onChange({ ...action, title: e.target.value })}
        />
        <TextArea
          label="Contenido"
          rows={4}
          value={action.body_md ?? ""}
          onChange={(e) => onChange({ ...action, body_md: e.target.value })}
        />
        <ListField
          label="Tags"
          value={action.tags ?? []}
          onChange={(v) => onChange({ ...action, tags: v })}
        />
        {areaSelect(action.area_id, (v) => onChange({ ...action, area_id: v }))}
      </div>
    );
  if (action.type === "person")
    return (
      <div className="space-y-3">
        <TextField
          label="Nombre"
          value={action.name}
          onChange={(e) => onChange({ ...action, name: e.target.value })}
        />
        <ListField
          label="Alias"
          value={action.aliases ?? []}
          onChange={(v) => onChange({ ...action, aliases: v })}
        />
        <TextArea
          label="Contexto"
          rows={3}
          value={action.context_md ?? ""}
          onChange={(e) => onChange({ ...action, context_md: e.target.value })}
        />
        {areaSelect(action.area_id, (v) => onChange({ ...action, area_id: v }))}
      </div>
    );
  return (
    <p className="text-muted">
      {action.type === "discard"
        ? "Se marcará como descartado (recuperable)."
        : "Se archivará (recuperable)."}
    </p>
  );
}

/** Revisión: nada se mueve hasta que el usuario confirma la acción (editable). */
function ReviewDialog({ item, onClose }: { item: InboxItem; onClose: () => void }) {
  const { propose, apply } = useInboxMutations();
  const toast = useToast();
  const proposal = item.proposal as Proposal | null | undefined;
  const [action, setAction] = useState<InboxAction>(() => actionFromProposal(item.text, proposal));
  const setType = (type: InboxAction["type"]) =>
    setAction(
      type === action.type ? action : actionFromProposal(item.text, { ...proposal, destination: type }),
    );
  const confirm = async () => {
    try {
      const r = await apply.mutateAsync({ item, action });
      toast.show(
        r.result_type && r.result_id ? (
          <span>
            Listo · <Link to={entityHref(r.result_type, r.result_id)}>abrir</Link>
          </span>
        ) : (
          "Listo"
        ),
        { tone: "ok" },
      );
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.isConflict)
        toast.show("Este elemento cambió en otro dispositivo; recarga y revisa de nuevo.", { tone: "error" });
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };
  return (
    <Dialog
      open
      size="lg"
      onClose={onClose}
      title="Procesar elemento"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            onClick={confirm}
            loading={apply.isPending}
            variant={action.type === "discard" ? "danger" : "primary"}
          >
            Confirmar: {ACTION_LABEL[action.type]}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <blockquote className="rounded-lg border-l-4 border-accent bg-surface-2 p-3">{item.text}</blockquote>
        {proposal?.reason ? (
          <p className="text-sm text-muted">
            Propuesta ({item.proposal_source === "manual" ? "editada" : "automática"}): {proposal.reason}
          </p>
        ) : (
          <Button
            size="sm"
            variant="secondary"
            loading={propose.isPending}
            onClick={() =>
              propose.mutate(item.id, {
                onSuccess: (it) => setAction(actionFromProposal(it.text, it.proposal as Proposal)),
              })
            }
          >
            Sugerir destino
          </Button>
        )}
        <fieldset>
          <legend className="mb-2 text-sm font-medium">Destino</legend>
          <div className="flex flex-wrap gap-2">
            {(Object.keys(ACTION_LABEL) as InboxAction["type"][]).map((t) => (
              <Button
                key={t}
                size="sm"
                variant={action.type === t ? "primary" : "secondary"}
                aria-pressed={action.type === t}
                onClick={() => setType(t)}
              >
                {ACTION_LABEL[t]}
              </Button>
            ))}
          </div>
        </fieldset>
        <ActionEditor action={action} onChange={setAction} />
      </div>
    </Dialog>
  );
}

export function InboxPage() {
  const [status, setStatus] = useState("pending");
  const inbox = useInbox(status);
  const { reopen } = useInboxMutations();
  const [reviewing, setReviewing] = useState<InboxItem | null>(null);
  const toast = useToast();
  return (
    <div className="space-y-4">
      <PageHeader title="Inbox" subtitle="Captura rápida; se procesa en la revisión semanal" />
      <QuickCapture />
      <QueuedCaptures />
      <div className="flex flex-wrap items-end justify-between gap-2">
        <SelectField label="Mostrar" value={status} onChange={(e) => setStatus(e.target.value)}>
          {Object.entries(INBOX_STATUS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </SelectField>
      </div>
      <QueryState
        query={inbox}
        isEmpty={(d) => d.length === 0}
        empty={<EmptyState title={status === "pending" ? "Inbox vacío 🎉" : "Nada aquí"} />}
      >
        {(items) => (
          <ul className="space-y-2">
            {items.map((it) => (
              <li key={it.id} className="rounded-xl border border-line bg-surface p-3">
                <p className="whitespace-pre-wrap break-words">{it.text}</p>
                <div className="mt-2 flex flex-wrap items-center justify-between gap-2 text-sm">
                  <span className="flex flex-wrap items-center gap-2 text-muted">
                    {formatDateTime(it.captured_at)}
                    {it.source === "telegram" && <Badge>Telegram</Badge>}
                    {it.status !== "pending" && (
                      <Badge tone={it.status === "processed" ? "ok" : "muted"}>
                        {INBOX_STATUS[it.status] ?? it.status}
                      </Badge>
                    )}
                    {it.result_type && it.result_id && (
                      <Link to={entityHref(it.result_type, it.result_id)}>Ver resultado</Link>
                    )}
                  </span>
                  {it.status === "pending" ? (
                    <Button size="sm" onClick={() => setReviewing(it)}>
                      Procesar
                    </Button>
                  ) : (
                    it.status !== "processed" && (
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() =>
                          reopen.mutate(it.id, {
                            onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
                          })
                        }
                      >
                        Recuperar
                      </Button>
                    )
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
      {reviewing && <ReviewDialog item={reviewing} onClose={() => setReviewing(null)} />}
    </div>
  );
}
