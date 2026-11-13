// /importar — subir el vault (ZIP o carpeta), revisar el dry-run (resumen, advertencias, conflictos, archivos no
// soportados) y confirmar. /exportar — descargar todos los datos. Nada se escribe sin confirmación.
import { useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { type Column, DataTable } from "../../components/ui/DataTable";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { Badge, Card, KpiCard, PageHeader } from "../../components/ui/Display";
import { SelectField } from "../../components/ui/Field";
import { EmptyState, ErrorState, LoadingState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import { ACTION_LABEL, ENTITY_LABEL, parseResult, parseSummary, readFolder } from "./folder";
import {
  downloadExport,
  type ImportRunView,
  useImportMutations,
  useImportRun,
  useImportRuns,
} from "./importsApi";

type PlanItem = ImportRunView["items"][number];

const RUN_STATUS: Record<string, { label: string; tone: "accent" | "ok" | "danger" | "muted" }> = {
  dry_run: { label: "Simulación (sin aplicar)", tone: "accent" },
  applied: { label: "Aplicada", tone: "ok" },
  failed: { label: "Falló (no se aplicó nada)", tone: "danger" },
};

const ACTION_TONE: Record<string, "ok" | "accent" | "muted" | "danger"> = {
  create: "ok",
  link: "accent",
  unchanged: "muted",
  conflict: "danger",
};

function Upload({ onDone }: { onDone: (id: string) => void }) {
  const m = useImportMutations();
  const toast = useToast();
  const zipRef = useRef<HTMLInputElement>(null);
  const dirRef = useRef<HTMLInputElement>(null);
  const [reading, setReading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const busy = reading || m.uploadZip.isPending || m.uploadFolder.isPending;
  return (
    <Card title="1. Sube tu vault">
      <p className="mb-3 text-sm text-muted">
        Se genera primero una simulación: verás qué se crearía, qué ya existe y los conflictos, sin escribir
        nada. Plantillas, comandos, scripts, vistas Bases y ejemplos del vault no se convierten en datos. No
        se ejecuta ningún contenido ni plugin.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="primary"
          loading={m.uploadZip.isPending}
          disabled={busy}
          onClick={() => zipRef.current?.click()}
        >
          Elegir ZIP del vault
        </Button>
        <Button
          variant="secondary"
          loading={reading || m.uploadFolder.isPending}
          disabled={busy}
          onClick={() => dirRef.current?.click()}
        >
          Elegir carpeta
        </Button>
      </div>
      <input
        ref={zipRef}
        type="file"
        accept=".zip,application/zip"
        className="sr-only"
        aria-label="Archivo ZIP del vault"
        onChange={async (e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (!f) return;
          setErr(null);
          try {
            const v = await m.uploadZip.mutateAsync(f);
            onDone(v.run.id);
          } catch (ex) {
            setErr(errorMessage(ex));
          }
        }}
      />
      <input
        ref={dirRef}
        type="file"
        multiple
        className="sr-only"
        aria-label="Carpeta del vault"
        // @ts-expect-error atributo no estándar, soportado por Chromium, Firefox y Safari
        webkitdirectory=""
        onChange={async (e) => {
          const list = Array.from(e.target.files ?? []);
          e.target.value = "";
          if (!list.length) return;
          setErr(null);
          setReading(true);
          try {
            const sel = await readFolder(list);
            setReading(false);
            const v = await m.uploadFolder.mutateAsync({ name: sel.name, files: sel.files });
            if (sel.skipped)
              toast.show(`Se omitieron ${sel.skipped} archivos de configuración (.obsidian, .git…)`);
            onDone(v.run.id);
          } catch (ex) {
            setErr(errorMessage(ex));
          } finally {
            setReading(false);
          }
        }}
      />
      {busy && (
        <p role="status" className="mt-3 text-sm text-muted">
          {reading ? "Leyendo archivos…" : "Analizando el vault…"}
        </p>
      )}
      {err && (
        <p role="alert" className="mt-3 text-sm text-danger">
          {err}
        </p>
      )}
    </Card>
  );
}

function RunView({ view }: { view: ImportRunView }) {
  const m = useImportMutations();
  const toast = useToast();
  const [confirm, setConfirm] = useState(false);
  const [action, setAction] = useState("");
  const s = parseSummary(view.run.summary);
  const result = parseResult(view.run.summary);
  const status = RUN_STATUS[view.run.status] ?? { label: view.run.status, tone: "muted" as const };
  const actionable = s.create + s.link;
  const rows = action ? view.items.filter((i) => i.action === action) : view.items;
  const columns: Column<PlanItem>[] = [
    { key: "title", header: "Registro", render: (r) => r.title || r.path, sortValue: (r) => r.title },
    {
      key: "entity",
      header: "Tipo",
      render: (r) => ENTITY_LABEL[r.entity_type] ?? r.entity_type,
      sortValue: (r) => r.entity_type,
    },
    {
      key: "action",
      header: "Acción",
      render: (r) => (
        <Badge tone={ACTION_TONE[r.action] ?? "muted"}>{ACTION_LABEL[r.action] ?? r.action}</Badge>
      ),
      sortValue: (r) => r.action,
    },
    {
      key: "path",
      header: "Archivo",
      render: (r) => <span className="break-all text-sm text-muted">{r.path}</span>,
      hideOnCard: true,
    },
    { key: "reason", header: "Detalle", render: (r) => r.reason ?? "", hideOnCard: false },
  ];
  return (
    <div className="space-y-4">
      <Card
        title={`2. Revisión: ${view.run.source_name}`}
        actions={<Badge tone={status.tone}>{status.label}</Badge>}
      >
        <p className="mb-3 text-sm text-muted">Generado {formatDateTime(view.run.created_at)}.</p>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <KpiCard label="Por crear" value={s.create} tone={s.create ? "ok" : undefined} />
          <KpiCard label="Por vincular/actualizar" value={s.link} />
          <KpiCard label="Sin cambios" value={s.unchanged} sub="ya importados" />
          <KpiCard label="Conflictos" value={s.conflicts} tone={s.conflicts ? "danger" : undefined} />
          <KpiCard label="Referencias" value={s.references} sub="archivos no soportados" />
          <KpiCard label="Ignorados" value={s.ignored} sub="plantillas, scripts…" />
          <KpiCard label="Advertencias" value={s.warnings} tone={s.warnings ? "warn" : undefined} />
          <KpiCard label="Total analizado" value={s.total} />
        </div>
        {result && (
          <p role="status" className="mt-3 rounded-lg border border-ok p-3 text-sm">
            Aplicada: {result.created} creados, {result.linked} vinculados, {result.skipped} omitidos,{" "}
            {result.conflicts} conflictos sin tocar.
          </p>
        )}
        {s.byEntity.length > 0 && (
          <div className="mt-3 overflow-x-auto">
            <table className="w-full min-w-[20rem] text-sm">
              <caption className="sr-only">Registros por tipo y acción</caption>
              <thead>
                <tr className="text-left text-muted">
                  <th className="py-1 pr-2 font-medium">Tipo</th>
                  {Object.keys(ACTION_LABEL).map((a) => (
                    <th key={a} className="py-1 pr-2 font-medium">
                      {ACTION_LABEL[a]}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {s.byEntity.map((e) => (
                  <tr key={e.entity} className="border-t border-line">
                    <td className="py-1 pr-2">{ENTITY_LABEL[e.entity] ?? e.entity}</td>
                    {Object.keys(ACTION_LABEL).map((a) => (
                      <td key={a} className="py-1 pr-2 tabular-nums">
                        {e.actions[a] ?? 0}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {view.run.status === "dry_run" && (
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button disabled={actionable === 0} onClick={() => setConfirm(true)}>
              Aplicar importación…
            </Button>
            {actionable === 0 && <span className="text-sm text-muted">No hay nada nuevo que aplicar.</span>}
          </div>
        )}
      </Card>

      {view.run.warnings.length > 0 && (
        <Card title="Advertencias">
          <ul className="list-disc space-y-1 pl-5 text-sm">
            {view.run.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </Card>
      )}

      {s.conflicts > 0 && (
        <Card title="Conflictos">
          <p className="mb-2 text-sm text-muted">
            Estos registros cambiaron en la app y en el archivo desde la última importación. No se
            sobrescriben: decide editando el registro en la app.
          </p>
          <ul className="list-disc space-y-1 pl-5 text-sm">
            {view.items
              .filter((i) => i.action === "conflict")
              .map((i) => (
                <li key={i.source_key}>
                  <span className="font-medium">{i.title || i.path}</span> —{" "}
                  {i.reason ?? "modificado en ambos lados"}
                </li>
              ))}
          </ul>
        </Card>
      )}

      <Card title="Detalle por archivo">
        <DataTable
          caption="Plan de importación"
          rows={rows}
          columns={columns}
          rowKey={(r) => `${r.source_key}:${r.entity_type}`}
          searchText={(r) => `${r.title} ${r.path} ${r.entity_type}`}
          pageSize={25}
          emptyTitle="Sin registros en este filtro"
          filters={
            <SelectField label="Acción" value={action} onChange={(e) => setAction(e.target.value)}>
              <option value="">Todas</option>
              {Object.entries(ACTION_LABEL).map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </SelectField>
          }
        />
      </Card>

      {(view.references.length > 0 || view.ignored.length > 0) && (
        <Card title="Archivos no importados como datos">
          {view.references.length > 0 && (
            <details className="mb-2">
              <summary className="cursor-pointer font-medium">
                Referencias conservadas ({view.references.length})
              </summary>
              <ul className="mt-2 space-y-1 text-sm">
                {view.references.map((f) => (
                  <li key={f.path}>
                    <span className="break-all">{f.path}</span>{" "}
                    <span className="text-muted">— {f.reason}</span>
                  </li>
                ))}
              </ul>
            </details>
          )}
          {view.ignored.length > 0 && (
            <details>
              <summary className="cursor-pointer font-medium">Ignorados ({view.ignored.length})</summary>
              <ul className="mt-2 space-y-1 text-sm">
                {view.ignored.map((f) => (
                  <li key={f.path}>
                    <span className="break-all">{f.path}</span>{" "}
                    <span className="text-muted">— {f.reason}</span>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </Card>
      )}

      <ConfirmDialog
        open={confirm}
        title="¿Aplicar la importación?"
        confirmLabel="Sí, importar"
        loading={m.apply.isPending}
        onClose={() => setConfirm(false)}
        onConfirm={async () => {
          try {
            await m.apply.mutateAsync(view.run.id);
            toast.show("Importación aplicada", { tone: "ok" });
          } catch (e) {
            toast.show(errorMessage(e), { tone: "error" });
          } finally {
            setConfirm(false);
          }
        }}
        message={
          <div className="space-y-2">
            <p>
              Se crearán {s.create} registros y se vincularán/actualizarán {s.link}, en una sola transacción.
              Si algo falla, no se aplica nada.
            </p>
            {s.conflicts > 0 && <p>Los {s.conflicts} conflictos se dejan sin tocar.</p>}
            <p className="text-sm text-muted">Reimportar el mismo vault no duplica registros.</p>
          </div>
        }
      />
    </div>
  );
}

function History({ current, onPick }: { current: string | null; onPick: (id: string) => void }) {
  const runs = useImportRuns();
  return (
    <Card title="Importaciones anteriores">
      <QueryState
        query={runs}
        isEmpty={(d) => d.length === 0}
        empty={<EmptyState title="Aún no has importado nada" />}
      >
        {(list) => (
          <ul className="divide-y divide-line">
            {list.map((r) => {
              const st = RUN_STATUS[r.status] ?? { label: r.status, tone: "muted" as const };
              return (
                <li key={r.id} className="flex flex-wrap items-center gap-2 py-2">
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium">{r.source_name}</p>
                    <p className="text-sm text-muted">{formatDateTime(r.created_at)}</p>
                  </div>
                  <Badge tone={st.tone}>{st.label}</Badge>
                  <Button
                    size="sm"
                    variant={r.id === current ? "primary" : "secondary"}
                    onClick={() => onPick(r.id)}
                  >
                    Ver
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </QueryState>
    </Card>
  );
}

export function ExportCard() {
  const toast = useToast();
  const [busy, setBusy] = useState<"json" | "zip" | null>(null);
  const run = async (kind: "json" | "zip") => {
    setBusy(kind);
    try {
      const name = await downloadExport(kind);
      toast.show(`Descargado ${name}`, { tone: "ok" });
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    } finally {
      setBusy(null);
    }
  };
  return (
    <Card title="Exportar mis datos">
      <p className="mb-3 text-sm text-muted">
        Incluye todos tus registros. Nunca incluye sesiones, identidades, credenciales de integraciones ni
        claves de idempotencia.
      </p>
      <ul className="mb-3 list-disc pl-5 text-sm">
        <li>
          <strong>JSON</strong>: todos los datos en un archivo.
        </li>
        <li>
          <strong>ZIP</strong>: el JSON + CSV (movimientos, hábitos, tareas) + Markdown con formato del vault,
          que puedes volver a importar.
        </li>
      </ul>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          loading={busy === "json"}
          disabled={busy !== null}
          onClick={() => void run("json")}
        >
          Descargar JSON
        </Button>
        <Button
          variant="secondary"
          loading={busy === "zip"}
          disabled={busy !== null}
          onClick={() => void run("zip")}
        >
          Descargar ZIP (JSON + CSV + Markdown)
        </Button>
      </div>
    </Card>
  );
}

export function ImportPage() {
  const [params, setParams] = useSearchParams();
  const runId = params.get("run");
  const run = useImportRun(runId);
  const pick = (id: string) => setParams({ run: id });
  return (
    <div className="space-y-4">
      <PageHeader
        title="Importar / exportar"
        subtitle="Trae tu vault de Obsidian con simulación previa, o descarga tus datos."
        actions={<ButtonLink to="/exportar">Ir a exportar</ButtonLink>}
      />
      <div className="grid min-w-0 gap-4 lg:grid-cols-2">
        <Upload onDone={pick} />
        <History current={runId} onPick={pick} />
      </div>
      {runId &&
        (run.isPending ? (
          <LoadingState rows={3} label="Cargando simulación…" />
        ) : run.isError ? (
          <ErrorState error={run.error} onRetry={() => run.refetch()} />
        ) : (
          <RunView view={run.data} />
        ))}
      <ExportCard />
    </div>
  );
}

export function ExportPage() {
  return (
    <div className="space-y-4">
      <PageHeader title="Exportar" actions={<ButtonLink to="/importar">Ir a importar</ButtonLink>} />
      <ExportCard />
    </div>
  );
}
