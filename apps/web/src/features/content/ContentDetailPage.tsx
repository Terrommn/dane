import { useState } from "react";
import { useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import { ConflictPrompt, isHttpUrl, LinksPanel, ListField, useArchiveMutation } from "../notes/common";
import {
  type ContentBody,
  type ContentPiece,
  type ContentView,
  compactHooks,
  contentBody,
  daysToPublishLabel,
  padHooks,
  type ShotRow,
  STATUS_LABEL,
  STATUSES,
  useAiContext,
  useContent,
  useContentMutations,
} from "./contentApi";

const SHOT_ROW =
  "grid gap-2 rounded-lg border border-line p-2 sm:grid-cols-[3rem_1fr_1fr_1fr_auto] sm:items-end";

function UrlLink({ url, label }: { url?: string | null; label: string }) {
  if (!url) return null;
  return (
    <a href={url} target="_blank" rel="noopener noreferrer" className="text-sm">
      {label} ↗
    </a>
  );
}

function Editor({ piece }: { piece: ContentView }) {
  const { update } = useContentMutations();
  const archive = useArchiveMutation();
  const toast = useToast();
  const ai = useAiContext();
  const [f, setF] = useState(() => ({ ...contentBody(piece), hooks: padHooks(piece.hooks) }));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [conflict, setConflict] = useState<ContentPiece | null>(null);
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF((x) => ({ ...x, [k]: v }));
  const shots = (f.shot_list ?? []) as ShotRow[];

  const save = async (data: typeof f, version: number) => {
    const errs: Record<string, string> = {};
    if (!data.title.trim()) errs.title = "El título es obligatorio";
    for (const k of ["raw_url", "render_url"] as const)
      if (data[k] && !isHttpUrl(data[k] ?? "")) errs[k] = "Debe ser un enlace http(s)";
    if (data.retention_pct && (Number(data.retention_pct) < 0 || Number(data.retention_pct) > 100))
      errs.retention_pct = "Entre 0 y 100";
    setErrors(errs);
    if (Object.keys(errs).length) return;
    const { hooks, chosen_hook } = compactHooks(data.hooks ?? [], data.chosen_hook ?? null);
    const body: ContentBody = {
      ...data,
      hooks,
      chosen_hook,
      shot_list: shots.map((s, i) => ({ ...s, n: i + 1 })),
      version,
    };
    try {
      const saved = await update.mutateAsync({ id: piece.id, body });
      setF({ ...contentBody(saved), hooks: padHooks(saved.hooks) });
      toast.show("Pieza guardada", { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as ContentPiece);
      else if (e instanceof ApiError && e.field) setErrors({ [e.field]: e.message });
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  return (
    <form
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        void save(f, piece.version);
      }}
    >
      <PageHeader
        title={piece.title}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Badge tone="accent">{STATUS_LABEL[piece.status]}</Badge>
            {piece.publish_on && (
              <span>
                Publicación {formatShortDate(piece.publish_on)} · {daysToPublishLabel(piece.days_to_publish)}
              </span>
            )}
          </span>
        }
        actions={
          <>
            <Button type="submit" loading={update.isPending}>
              Guardar
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                archive.mutate({ type: "content", id: piece.id, restore: piece.archived_at != null })
              }
            >
              {piece.archived_at ? "Recuperar" : "Archivar"}
            </Button>
            <ButtonLink to="/contenido">Volver</ButtonLink>
          </>
        }
      />
      <Card title="Pieza">
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField
            label="Título"
            value={f.title}
            onChange={(e) => set("title", e.target.value)}
            error={errors.title}
          />
          <SelectField
            label="Estado"
            value={f.status ?? "idea"}
            onChange={(e) => set("status", e.target.value)}
          >
            {STATUSES.map((s) => (
              <option key={s} value={s}>
                {STATUS_LABEL[s]}
              </option>
            ))}
          </SelectField>
          <TextField
            label="Formato"
            value={f.format ?? ""}
            onChange={(e) => set("format", e.target.value)}
            placeholder="reel, carrusel, ad…"
          />
          <ListField label="Plataformas" value={f.platforms ?? []} onChange={(v) => set("platforms", v)} />
          <TextField
            label="Pilar"
            value={f.pillar ?? ""}
            onChange={(e) => set("pillar", e.target.value || null)}
          />
          <TextField
            label="Fecha de publicación"
            type="date"
            value={f.publish_on ?? ""}
            onChange={(e) => set("publish_on", e.target.value || null)}
          />
        </div>
        <TextArea
          className="mt-3"
          label="Idea en una frase"
          rows={2}
          value={f.idea ?? ""}
          onChange={(e) => set("idea", e.target.value)}
        />
      </Card>

      <Card title="Hooks (escribe 10, elige 1)">
        <ol className="space-y-2">
          {(f.hooks ?? []).map((h, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: 10 posiciones fijas
            <li key={i} className="flex items-end gap-2">
              <input
                type="radio"
                name="chosen-hook"
                className="mb-3 h-5 w-5 accent-[var(--c-accent)]"
                aria-label={`Elegir hook ${i + 1}`}
                checked={f.chosen_hook === i}
                disabled={!h.trim()}
                onChange={() => set("chosen_hook", i)}
              />
              <TextField
                className="flex-1"
                label={`Hook ${i + 1}`}
                value={h}
                onChange={(e) =>
                  set(
                    "hooks",
                    (f.hooks ?? []).map((x, j) => (j === i ? e.target.value : x)),
                  )
                }
              />
            </li>
          ))}
        </ol>
      </Card>

      <Card title="Guion">
        <div className="space-y-3">
          <TextArea
            label="Hook (0–3 s)"
            rows={2}
            value={f.script_hook ?? ""}
            onChange={(e) => set("script_hook", e.target.value)}
          />
          <TextArea
            label="Desarrollo"
            rows={5}
            value={f.script_body ?? ""}
            onChange={(e) => set("script_body", e.target.value)}
          />
          <TextArea
            label="Cierre / CTA"
            rows={2}
            value={f.script_cta ?? ""}
            onChange={(e) => set("script_cta", e.target.value)}
          />
        </div>
        {ai.data && ai.data.recent_published.length > 0 && (
          <details className="mt-3 text-sm">
            <summary className="cursor-pointer text-muted">
              Últimas 5 piezas publicadas (referencia antes de escribir el guion)
            </summary>
            <ul className="mt-2 space-y-1">
              {ai.data.recent_published.map((p) => (
                <li key={p.id}>
                  <strong>{p.title}</strong> · {p.views ?? "—"} views · {p.retention_pct ?? "—"}% ret.
                  {p.result_worked && <span className="text-muted"> · funcionó: {p.result_worked}</span>}
                </li>
              ))}
            </ul>
          </details>
        )}
      </Card>

      <Card title="Shot list">
        <div className="space-y-3">
          {shots.map((s, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin id
            <fieldset key={i} className={SHOT_ROW}>
              <legend className="sr-only">Toma {i + 1}</legend>
              <span className="pb-2 font-semibold">#{i + 1}</span>
              <TextField
                label="Plano"
                value={s.shot ?? ""}
                onChange={(e) =>
                  set(
                    "shot_list",
                    shots.map((x, j) => (j === i ? { ...x, shot: e.target.value } : x)),
                  )
                }
              />
              <TextField
                label="Texto en pantalla"
                value={s.text ?? ""}
                onChange={(e) =>
                  set(
                    "shot_list",
                    shots.map((x, j) => (j === i ? { ...x, text: e.target.value } : x)),
                  )
                }
              />
              <TextField
                label="Motion graphic"
                value={s.motion ?? ""}
                onChange={(e) =>
                  set(
                    "shot_list",
                    shots.map((x, j) => (j === i ? { ...x, motion: e.target.value } : x)),
                  )
                }
              />
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Quitar toma ${i + 1}`}
                onClick={() =>
                  set(
                    "shot_list",
                    shots.filter((_, j) => j !== i),
                  )
                }
              >
                ✕
              </Button>
            </fieldset>
          ))}
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              set("shot_list", [...shots, { n: shots.length + 1, shot: "", text: "", motion: "" }])
            }
          >
            Agregar toma
          </Button>
        </div>
      </Card>

      <Card title="Producción y resultado">
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <TextField
              label="Original (raw)"
              type="url"
              value={f.raw_url ?? ""}
              onChange={(e) => set("raw_url", e.target.value || null)}
              error={errors.raw_url}
              placeholder="https://drive.google.com/…"
            />
            <UrlLink url={piece.raw_url} label="Abrir original" />
          </div>
          <div>
            <TextField
              label="Render"
              type="url"
              value={f.render_url ?? ""}
              onChange={(e) => set("render_url", e.target.value || null)}
              error={errors.render_url}
            />
            <UrlLink url={piece.render_url} label="Abrir render" />
          </div>
          <TextField
            label="Publicada el"
            type="date"
            value={f.published_on ?? ""}
            onChange={(e) => set("published_on", e.target.value || null)}
          />
          <div className="grid grid-cols-2 gap-3">
            <TextField
              label="Views"
              type="number"
              min={0}
              value={f.views ?? ""}
              onChange={(e) => set("views", e.target.value === "" ? null : Number(e.target.value))}
            />
            <TextField
              label="Retención %"
              inputMode="decimal"
              value={f.retention_pct ?? ""}
              onChange={(e) => set("retention_pct", e.target.value || null)}
              error={errors.retention_pct}
            />
          </div>
        </div>
        <TextArea
          className="mt-3"
          label="Notas de edición"
          rows={3}
          value={f.edit_notes ?? ""}
          onChange={(e) => set("edit_notes", e.target.value)}
        />
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <TextArea
            label="Lo que funcionó"
            rows={3}
            value={f.result_worked ?? ""}
            onChange={(e) => set("result_worked", e.target.value)}
          />
          <TextArea
            label="Lo que cambiaría"
            rows={3}
            value={f.result_change ?? ""}
            onChange={(e) => set("result_change", e.target.value)}
          />
        </div>
        <p className="mt-2 text-sm text-muted">
          Los videos no se descargan ni se suben: solo se guardan los enlaces.
        </p>
      </Card>
      <Card>
        <LinksPanel type="content" id={piece.id} />
      </Card>
      <ConflictPrompt
        conflict={conflict}
        mine={f as unknown as Record<string, unknown>}
        onClose={() => setConflict(null)}
        onTakeServer={(srv) => setF({ ...contentBody(srv), hooks: padHooks(srv.hooks) })}
        onSave={(data, v) => void save(data as typeof f, v)}
      />
    </form>
  );
}

export function ContentDetailPage() {
  const { id = "" } = useParams();
  const q = useContent(id);
  return <QueryState query={q}>{(p) => <Editor key={p.id} piece={p} />}</QueryState>;
}
