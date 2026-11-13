import { useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, PageHeader } from "../../components/ui/Display";
import { CheckboxField, TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { type SwipeRef, useSwipe, useSwipeMutations } from "../content/contentApi";
import { isHttpUrl, ListField, TagList, useArchiveMutation } from "../notes/common";

function SwipeDialog({ item, onClose }: { item: SwipeRef | null; onClose: () => void }) {
  const save = useSwipeMutations();
  const toast = useToast();
  const [f, setF] = useState({
    title: item?.title ?? "",
    url: item?.url ?? "",
    creator: item?.creator ?? "",
    platform: item?.platform ?? "",
    why_it_works: item?.why_it_works ?? "",
    hook: item?.hook ?? "",
    tags: item?.tags ?? [],
  });
  const [err, setErr] = useState<string | null>(null);
  const submit = () => {
    if (!isHttpUrl(f.url)) return setErr("Pega el enlace completo (https://…)");
    setErr(null);
    save.mutate(
      {
        id: item?.id,
        body: {
          ...f,
          creator: f.creator || null,
          platform: f.platform || null,
          archived: item?.archived_at != null,
          version: item?.version ?? null,
        },
      },
      {
        onSuccess: () => {
          toast.show("Referencia guardada", { tone: "ok" });
          onClose();
        },
        onError: (e) => setErr(errorMessage(e)),
      },
    );
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={item ? "Editar referencia" : "Nueva referencia"}
      footer={
        <Button onClick={submit} loading={save.isPending}>
          Guardar
        </Button>
      }
    >
      <div className="space-y-3">
        <TextField
          label="URL"
          type="url"
          value={f.url}
          onChange={(e) => setF({ ...f, url: e.target.value })}
          error={err ?? undefined}
        />
        <TextField label="Título" value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} />
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField
            label="Creador"
            value={f.creator}
            onChange={(e) => setF({ ...f, creator: e.target.value })}
          />
          <TextField
            label="Plataforma"
            value={f.platform}
            onChange={(e) => setF({ ...f, platform: e.target.value })}
          />
        </div>
        <TextArea
          label="Hook (transcrito)"
          rows={2}
          value={f.hook}
          onChange={(e) => setF({ ...f, hook: e.target.value })}
        />
        <TextArea
          label="Por qué funciona"
          rows={3}
          value={f.why_it_works}
          onChange={(e) => setF({ ...f, why_it_works: e.target.value })}
        />
        <ListField label="Tags" value={f.tags} onChange={(v) => setF({ ...f, tags: v })} />
      </div>
    </Dialog>
  );
}

export function SwipePage() {
  const [params] = useSearchParams();
  const [q, setQ] = useState("");
  const [archived, setArchived] = useState(false);
  const list = useSwipe(q.trim(), "", archived);
  const archive = useArchiveMutation();
  const [editing, setEditing] = useState<SwipeRef | "new" | null>(null);
  const focus = params.get("id");
  return (
    <div className="space-y-4">
      <PageHeader
        title="Swipe"
        subtitle="Referencias de otros creadores: enlace, hook y por qué funciona"
        actions={<Button onClick={() => setEditing("new")}>Nueva referencia</Button>}
      />
      <div className="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-end">
        <TextField label="Buscar" value={q} onChange={(e) => setQ(e.target.value)} />
        <CheckboxField
          label="Archivadas"
          checked={archived}
          onChange={(e) => setArchived(e.target.checked)}
        />
      </div>
      <QueryState
        query={list}
        isEmpty={(d) => d.length === 0}
        empty={
          <EmptyState
            title="Sin referencias"
            action={<Button onClick={() => setEditing("new")}>Agregar</Button>}
          />
        }
      >
        {(items) => (
          <ul className="grid gap-3 md:grid-cols-2">
            {items.map((s) => (
              <li
                key={s.id}
                className={`rounded-xl border bg-surface p-3 ${focus === s.id ? "border-accent" : "border-line"}`}
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <a
                    href={s.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="min-w-0 break-words font-semibold"
                  >
                    {s.title || s.url} ↗
                  </a>
                  {s.platform && <Badge>{s.platform}</Badge>}
                </div>
                {s.creator && <p className="text-sm text-muted">{s.creator}</p>}
                {s.hook && <p className="mt-2 italic">“{s.hook}”</p>}
                {s.why_it_works && <p className="mt-1 text-sm">{s.why_it_works}</p>}
                <TagList tags={s.tags} />
                <div className="mt-2 flex gap-2">
                  <Button size="sm" variant="secondary" onClick={() => setEditing(s)}>
                    Editar
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      archive.mutate({ type: "swipe", id: s.id, restore: s.archived_at != null })
                    }
                  >
                    {s.archived_at ? "Recuperar" : "Archivar"}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
      {editing && <SwipeDialog item={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
