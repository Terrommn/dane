// /ajustes/integraciones — Google (Calendar + Gmail, solo lectura), ClickUp (solo lectura), IA y sugerencias.
// Estados conectado/error/desconectado visibles; ninguna integración escribe en el proveedor.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, ErrorState, LoadingState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { getTelegram } from "../../lib/telegram";
import {
  type ConnectionStatus,
  type Overview,
  type SyncReport,
  useClickupTasks,
  useGoogleCalendars,
  useIntegrationMutations,
  useIntegrations,
  useSuggestions,
} from "./integrationsApi";
import {
  CATEGORY_LABEL,
  clickupConfig,
  connectionInfo,
  googleConfig,
  googleReturnMessage,
  groupClickupTasks,
  rowsToStatusMap,
  type StatusCategory,
  type StatusRow,
  statusMapToRows,
} from "./status";

function ConnectionMeta({ c }: { c: ConnectionStatus }) {
  const info = connectionInfo(c.status);
  return (
    <div className="space-y-1 text-sm">
      <p className="flex flex-wrap items-center gap-2">
        <Badge tone={info.tone}>{info.label}</Badge>
        {c.account_label && <span className="text-muted">{c.account_label}</span>}
      </p>
      <p className="text-muted">
        Última sincronización: {c.last_sync_at ? formatDateTime(c.last_sync_at) : "nunca"}
      </p>
      {c.last_error && (
        <p role="alert" className="text-danger">
          Último error{c.last_error_at ? ` (${formatDateTime(c.last_error_at)})` : ""}: {c.last_error}
        </p>
      )}
    </div>
  );
}

function SyncReports({ reports }: { reports: SyncReport[] }) {
  if (reports.length === 0) return null;
  return (
    <ul className="mt-2 space-y-1 text-sm" aria-live="polite">
      {reports.map((r) => (
        <li key={`${r.provider}:${r.kind}`} className={r.error ? "text-danger" : ""}>
          {r.kind === "calendar" ? "Agenda" : r.kind === "gmail" ? "Correos enviados" : r.kind}:{" "}
          {r.error ??
            `${r.upserted} actualizados · ${r.removed} quitados · ${r.suggestions} sugerencias · ${r.skipped} omitidos`}
        </li>
      ))}
    </ul>
  );
}

function openExternal(url: string) {
  const tg = getTelegram() as (ReturnType<typeof getTelegram> & { openLink?: (u: string) => void }) | null;
  // Google no permite autorizar dentro de WebViews: en la Mini App se abre el navegador del sistema.
  if (tg?.openLink) tg.openLink(url);
  else window.location.assign(url);
}

function GoogleCard({ overview }: { overview: Overview }) {
  const g = overview.google;
  const connected = g.status === "connected" || g.status === "error";
  const cfg = googleConfig(g.config);
  const calendars = useGoogleCalendars(connected);
  const m = useIntegrationMutations();
  const toast = useToast();
  const [selected, setSelected] = useState<string[]>(cfg.calendars);
  const [reports, setReports] = useState<SyncReport[]>([]);
  const [confirmOff, setConfirmOff] = useState(false);
  const savedCalendars = cfg.calendars.join("|");
  useEffect(() => setSelected(savedCalendars ? savedCalendars.split("|") : []), [savedCalendars]);

  const saveConfig = async (patch: {
    calendars?: string[];
    calendar_enabled?: boolean;
    gmail_enabled?: boolean;
  }) => {
    try {
      await m.googleConfig.mutateAsync({ version: g.version, ...patch });
      toast.show("Configuración de Google guardada", { tone: "ok" });
    } catch (e) {
      toast.show(
        e instanceof ApiError && e.isConflict
          ? "Cambió en otro dispositivo; recargué el estado actual."
          : errorMessage(e),
        { tone: "error" },
      );
    }
  };

  return (
    <Card title="Google Calendar y Gmail">
      <p className="mb-3 text-sm text-muted">
        Solo lectura (scopes calendar.readonly y gmail.readonly). La app no crea eventos ni envía correos. Los
        eventos se usan como propuesta: no prueban asistencia. Las personas detectadas quedan como
        sugerencias.
      </p>
      {!overview.google_available ? (
        <p role="note" className="rounded-lg border border-warn p-3 text-sm">
          El servidor no tiene credenciales de Google (GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET). Redirect URI
          a registrar: <code className="break-all">{overview.google_redirect_uri}</code>
        </p>
      ) : (
        <div className="space-y-4">
          <ConnectionMeta c={g} />
          {!connected ? (
            <Button
              loading={m.googleConnect.isPending}
              onClick={async () => {
                try {
                  const r = await m.googleConnect.mutateAsync("/ajustes/integraciones");
                  openExternal(r.url);
                } catch (e) {
                  toast.show(errorMessage(e), { tone: "error" });
                }
              }}
            >
              Conectar Google
            </Button>
          ) : (
            <>
              <fieldset className="space-y-2">
                <legend className="mb-1 text-sm font-medium">Calendarios a leer</legend>
                {calendars.isPending ? (
                  <LoadingState rows={2} label="Cargando calendarios…" />
                ) : calendars.isError ? (
                  <ErrorState
                    error={calendars.error}
                    onRetry={() => calendars.refetch()}
                    title="No se pudieron leer los calendarios"
                  />
                ) : (calendars.data ?? []).length === 0 ? (
                  <p className="text-sm text-muted">Sin calendarios.</p>
                ) : (
                  (calendars.data ?? []).map((c) => (
                    <CheckboxField
                      key={c.id}
                      label={`${c.summary}${c.primary ? " (principal)" : ""}`}
                      hint={
                        c.excluded_by_rule ? "Festivos/observancias/contactos: siempre excluido" : undefined
                      }
                      disabled={c.excluded_by_rule}
                      checked={!c.excluded_by_rule && selected.includes(c.id)}
                      onChange={(e) =>
                        setSelected((xs) => (e.target.checked ? [...xs, c.id] : xs.filter((x) => x !== c.id)))
                      }
                    />
                  ))
                )}
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={calendars.isPending || calendars.isError}
                  loading={m.googleConfig.isPending}
                  onClick={() => void saveConfig({ calendars: selected })}
                >
                  Guardar calendarios
                </Button>
              </fieldset>
              <div className="grid gap-2 sm:grid-cols-2">
                <CheckboxField
                  label="Leer agenda"
                  checked={cfg.calendarEnabled}
                  onChange={(e) => void saveConfig({ calendar_enabled: e.target.checked })}
                />
                <CheckboxField
                  label="Leer correos enviados (Gmail)"
                  hint="Solo «Enviados»; se ignoran automáticos y newsletters."
                  checked={cfg.gmailEnabled}
                  onChange={(e) => void saveConfig({ gmail_enabled: e.target.checked })}
                />
              </div>
              <div className="flex flex-wrap gap-2">
                <Button
                  loading={m.googleSync.isPending}
                  onClick={async () => {
                    try {
                      setReports(await m.googleSync.mutateAsync());
                    } catch (e) {
                      toast.show(errorMessage(e), { tone: "error" });
                    }
                  }}
                >
                  Sincronizar ahora
                </Button>
                <Button variant="danger" onClick={() => setConfirmOff(true)}>
                  Desconectar
                </Button>
              </div>
              <SyncReports reports={reports} />
            </>
          )}
        </div>
      )}
      <ConfirmDialog
        open={confirmOff}
        title="¿Desconectar Google?"
        danger
        confirmLabel="Desconectar y revocar"
        loading={m.googleDisconnect.isPending}
        onClose={() => setConfirmOff(false)}
        onConfirm={async () => {
          try {
            await m.googleDisconnect.mutateAsync();
            toast.show("Acceso de Google revocado y credenciales borradas", { tone: "ok" });
          } catch (e) {
            toast.show(errorMessage(e), { tone: "error" });
          } finally {
            setConfirmOff(false);
          }
        }}
        message="Se revoca el acceso en Google y se borran las credenciales cifradas. Los eventos ya sincronizados se conservan."
      />
    </Card>
  );
}

function useAreasList() {
  return useQuery({
    queryKey: ["areas", { includeArchived: false }],
    queryFn: async () => unwrap(await api.GET("/areas", { params: { query: { archived: false } } })),
  });
}

function StatusMapEditor({ rows, onChange }: { rows: StatusRow[]; onChange: (r: StatusRow[]) => void }) {
  return (
    <fieldset className="space-y-2">
      <legend className="mb-1 text-sm font-medium">Estados de ClickUp → estado en la app</legend>
      {rows.map((r, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <div key={i} className="flex flex-wrap items-end gap-2">
          <TextField
            className="min-w-0 flex-1"
            label={`Estado ${i + 1} en ClickUp`}
            value={r.status}
            onChange={(e) => onChange(rows.map((x, j) => (j === i ? { ...x, status: e.target.value } : x)))}
          />
          <SelectField
            className="min-w-0 flex-1"
            label="Significa"
            value={r.category}
            onChange={(e) =>
              onChange(
                rows.map((x, j) => (j === i ? { ...x, category: e.target.value as StatusCategory } : x)),
              )
            }
          >
            {(Object.keys(CATEGORY_LABEL) as StatusCategory[]).map((k) => (
              <option key={k} value={k}>
                {CATEGORY_LABEL[k]}
              </option>
            ))}
          </SelectField>
          <Button
            variant="ghost"
            size="sm"
            aria-label={`Quitar estado ${i + 1}`}
            onClick={() => onChange(rows.filter((_, j) => j !== i))}
          >
            ✕
          </Button>
        </div>
      ))}
      <Button
        variant="secondary"
        size="sm"
        onClick={() => onChange([...rows, { status: "", category: "open" }])}
      >
        Agregar estado
      </Button>
    </fieldset>
  );
}

function ClickupCard({ overview }: { overview: Overview }) {
  const c = overview.clickup;
  const connected = c.status === "connected" || c.status === "error";
  const cfg = clickupConfig(c.config);
  const m = useIntegrationMutations();
  const toast = useToast();
  const areas = useAreasList();
  const tasks = useClickupTasks(connected);
  const [token, setToken] = useState("");
  const [viewId, setViewId] = useState(cfg.viewId);
  const [areaId, setAreaId] = useState(cfg.areaId);
  const [rows, setRows] = useState<StatusRow[]>(() => statusMapToRows(cfg.statusMap));
  const [errors, setErrors] = useState<string[]>([]);
  const [report, setReport] = useState<SyncReport | null>(null);
  const [confirmOff, setConfirmOff] = useState(false);

  const save = async () => {
    const { map, errors: errs } = rowsToStatusMap(rows);
    setErrors(errs);
    if (errs.length) return;
    if (!connected && !token.trim()) {
      setErrors(["Escribe tu token personal de ClickUp (empieza con pk_)."]);
      return;
    }
    try {
      await m.clickupSave.mutateAsync({
        token: token.trim() || null,
        view_id: viewId.trim() || null,
        area_id: areaId || null,
        status_map: map,
        version: token.trim() ? null : c.version,
      });
      setToken("");
      toast.show(connected ? "Configuración de ClickUp guardada" : "ClickUp conectado", { tone: "ok" });
    } catch (e) {
      setErrors([
        e instanceof ApiError && e.isConflict
          ? "Cambió en otro dispositivo; recarga la página."
          : errorMessage(e),
      ]);
    }
  };

  return (
    <Card title="ClickUp">
      <p role="note" className="mb-3 rounded-lg border border-line bg-surface-2 p-3 text-sm">
        Solo lectura: la app lee únicamente la vista configurada. Marcar una tarea aquí no la cambia en
        ClickUp.
      </p>
      <div className="space-y-4">
        <ConnectionMeta c={c} />
        <form
          className="space-y-3"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <TextField
            label={connected ? "Reemplazar token personal (opcional)" : "Token personal de ClickUp"}
            type="password"
            autoComplete="off"
            value={token}
            hint="ClickUp → Settings → Apps → API Token. Se guarda cifrado en el servidor; nunca en este dispositivo."
            onChange={(e) => setToken(e.target.value)}
          />
          <TextField
            label="ID de la vista"
            value={viewId}
            hint="El último segmento de la URL de la vista (…/v/l/<view_id>)."
            onChange={(e) => setViewId(e.target.value)}
          />
          <SelectField
            label="Área donde aparecen las tareas"
            value={areaId}
            onChange={(e) => setAreaId(e.target.value)}
          >
            <option value="">Trabajo (por defecto)</option>
            {(areas.data ?? []).map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </SelectField>
          <StatusMapEditor rows={rows} onChange={setRows} />
          {errors.length > 0 && (
            <ul role="alert" className="list-disc pl-5 text-sm text-danger">
              {errors.map((x) => (
                <li key={x}>{x}</li>
              ))}
            </ul>
          )}
          <div className="flex flex-wrap gap-2">
            <Button type="submit" loading={m.clickupSave.isPending}>
              {connected ? "Guardar configuración" : "Conectar ClickUp"}
            </Button>
            {connected && (
              <>
                <Button
                  variant="secondary"
                  loading={m.clickupSync.isPending}
                  onClick={async () => {
                    try {
                      setReport(await m.clickupSync.mutateAsync());
                      void tasks.refetch();
                    } catch (e) {
                      toast.show(errorMessage(e), { tone: "error" });
                    }
                  }}
                >
                  Sincronizar ahora
                </Button>
                <Button variant="danger" onClick={() => setConfirmOff(true)}>
                  Desconectar
                </Button>
              </>
            )}
          </div>
          {report && <SyncReports reports={[report]} />}
        </form>
        {connected && (
          <section aria-labelledby="clickup-tasks">
            <h3 id="clickup-tasks" className="mb-2 font-semibold">
              Tareas de la vista (orden de la daily)
            </h3>
            <QueryState
              query={tasks}
              isEmpty={(d) => d.length === 0}
              empty={
                <EmptyState title="Sin tareas abiertas">
                  Sincroniza para traer las tareas de la vista.
                </EmptyState>
              }
            >
              {(list) => (
                <div className="space-y-3">
                  {groupClickupTasks(list).map((g) => (
                    <div key={g.key}>
                      <p className="text-sm font-medium text-muted">{g.title}</p>
                      <ul className="divide-y divide-line">
                        {g.items.map((t) => (
                          <li key={t.task_id} className="py-2">
                            {t.url ? (
                              <a href={t.url} target="_blank" rel="noreferrer" className="font-medium">
                                {t.title}
                              </a>
                            ) : (
                              <span className="font-medium">{t.title}</span>
                            )}
                            <span className="ml-2 text-sm text-muted">
                              {t.status}
                              {t.priority ? ` · ${t.priority}` : ""}
                            </span>
                            {t.reason && <p className="text-sm text-ok">🟢 fácil: {t.reason}</p>}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              )}
            </QueryState>
          </section>
        )}
      </div>
      <ConfirmDialog
        open={confirmOff}
        title="¿Desconectar ClickUp?"
        danger
        confirmLabel="Desconectar"
        loading={m.clickupDisconnect.isPending}
        onClose={() => setConfirmOff(false)}
        onConfirm={async () => {
          try {
            await m.clickupDisconnect.mutateAsync();
            toast.show("Token de ClickUp borrado del servidor", { tone: "ok" });
          } catch (e) {
            toast.show(errorMessage(e), { tone: "error" });
          } finally {
            setConfirmOff(false);
          }
        }}
        message={
          <>
            <p>Se borra el token cifrado del servidor. Las tareas ya importadas se conservan como enlaces.</p>
            <p className="mt-2 text-sm text-muted">
              ClickUp no ofrece revocación de tokens personales por API: si quieres invalidarlo, regenéralo en
              ClickUp → Settings → Apps.
            </p>
          </>
        }
      />
    </Card>
  );
}

function LlmCard({ overview }: { overview: Overview }) {
  const l = overview.llm;
  return (
    <Card title="Asistente de captura (IA)">
      {l.configured ? (
        <p className="text-sm">
          <Badge tone="ok">Configurado</Badge> <span className="ml-1">Modelo: {l.model ?? "—"}</span>
        </p>
      ) : (
        <p className="text-sm">
          <Badge tone="muted">Modo manual</Badge>{" "}
          <span className="ml-1 text-muted">
            Sin ANTHROPIC_API_KEY en el servidor: las capturas se completan con formularios, sin respuestas
            automáticas.
          </span>
        </p>
      )}
      <p className="mt-2 text-sm text-muted">
        Solo se envía el texto de la captura y catálogos mínimos (categorías, áreas, cuentas, nombres). Toda
        propuesta se valida en el servidor y requiere tu confirmación.
      </p>
    </Card>
  );
}

function SuggestionsCard() {
  const s = useSuggestions();
  const m = useIntegrationMutations();
  const toast = useToast();
  const areas = useAreasList();
  const [topics, setTopics] = useState<Record<string, string>>({});
  const [names, setNames] = useState<Record<string, string>>({});
  const [areaOf, setAreaOf] = useState<Record<string, string>>({});
  const act = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      toast.show(ok, { tone: "ok" });
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    }
  };
  return (
    <Card title="Sugerencias de Calendar y Gmail">
      <p className="mb-3 text-sm text-muted">
        Nada se registra hasta que lo aceptes. Un evento no prueba que asististe.
      </p>
      <QueryState
        query={s}
        isEmpty={(d) => d.interactions.length === 0 && d.people.length === 0}
        empty={<EmptyState title="Sin sugerencias pendientes" />}
      >
        {(d) => (
          <div className="space-y-4">
            {d.interactions.length > 0 && (
              <section>
                <h3 className="mb-2 font-semibold">Interacciones propuestas</h3>
                <ul className="space-y-3">
                  {d.interactions.map((x) => (
                    <li key={x.id} className="rounded-lg border border-line p-3">
                      <p className="font-medium">
                        {x.person_name} · {formatShortDate(x.on_date)}{" "}
                        <span className="text-sm text-muted">
                          ({x.source === "gmail" ? "correo enviado" : "agenda"})
                        </span>
                      </p>
                      <TextField
                        className="mt-2"
                        label="Tema"
                        value={topics[x.id] ?? x.topic}
                        onChange={(e) => setTopics((t) => ({ ...t, [x.id]: e.target.value }))}
                      />
                      <div className="mt-2 flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          onClick={() =>
                            void act(
                              () =>
                                m.acceptInteraction.mutateAsync({ id: x.id, topic: topics[x.id] ?? null }),
                              "Interacción registrada",
                            )
                          }
                        >
                          Registrar
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => void act(() => m.dismissInteraction.mutateAsync(x.id), "Descartada")}
                        >
                          Descartar
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {d.people.length > 0 && (
              <section>
                <h3 className="mb-2 font-semibold">Personas nuevas detectadas</h3>
                <ul className="space-y-3">
                  {d.people.map((p) => (
                    <li key={p.id} className="rounded-lg border border-line p-3">
                      <p className="text-sm text-muted">
                        {p.email ?? ""} ·{" "}
                        {p.source === "gmail" ? "correo" : p.source === "calendar" ? "agenda" : p.source}
                        {p.on_date ? ` · ${formatShortDate(p.on_date)}` : ""}
                      </p>
                      {p.context && <p className="text-sm">{p.context}</p>}
                      <div className="mt-2 grid gap-2 sm:grid-cols-2">
                        <TextField
                          label="Nombre"
                          value={names[p.id] ?? p.name}
                          onChange={(e) => setNames((n) => ({ ...n, [p.id]: e.target.value }))}
                        />
                        <SelectField
                          label="Área"
                          value={areaOf[p.id] ?? ""}
                          onChange={(e) => setAreaOf((a) => ({ ...a, [p.id]: e.target.value }))}
                        >
                          <option value="">Sin área</option>
                          {(areas.data ?? []).map((a) => (
                            <option key={a.id} value={a.id}>
                              {a.name}
                            </option>
                          ))}
                        </SelectField>
                      </div>
                      <div className="mt-2 flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          onClick={() =>
                            void act(
                              () =>
                                m.acceptPerson.mutateAsync({
                                  id: p.id,
                                  name: names[p.id] ?? null,
                                  area_id: areaOf[p.id] || null,
                                }),
                              "Persona creada",
                            )
                          }
                        >
                          Crear persona
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => void act(() => m.dismissPerson.mutateAsync(p.id), "Descartada")}
                        >
                          Descartar
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </div>
        )}
      </QueryState>
    </Card>
  );
}

function RecentRuns({ overview }: { overview: Overview }) {
  if (overview.recent_runs.length === 0) return null;
  return (
    <Card title="Sincronizaciones recientes">
      <ul className="divide-y divide-line text-sm">
        {overview.recent_runs.map((r) => (
          <li key={`${r.provider}:${r.kind}:${r.started_at}`} className="flex flex-wrap gap-2 py-2">
            <span className="font-medium">
              {r.provider === "google" ? "Google" : r.provider === "clickup" ? "ClickUp" : r.provider} ·{" "}
              {r.kind}
            </span>
            <span className="text-muted">{formatDateTime(r.started_at)}</span>
            {r.ok === false ? (
              <Badge tone="danger">Error</Badge>
            ) : r.ok ? (
              <Badge tone="ok">{r.items} elementos</Badge>
            ) : (
              <Badge>En curso</Badge>
            )}
            {r.error && <span className="w-full text-danger">{r.error}</span>}
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function IntegrationsPage() {
  const q = useIntegrations();
  const [params, setParams] = useSearchParams();
  const toast = useToast();
  const ret = googleReturnMessage(params.get("google"));
  // biome-ignore lint/correctness/useExhaustiveDependencies: solo al volver del callback de Google
  useEffect(() => {
    if (!ret) return;
    toast.show(ret.text, { tone: ret.tone });
    params.delete("google");
    setParams(params, { replace: true });
  }, [ret?.text]);
  return (
    <div className="space-y-4">
      <PageHeader
        title="Integraciones"
        subtitle="Conexiones opcionales de solo lectura. Puedes desconectarlas cuando quieras."
      />
      <QueryState query={q} loadingRows={4}>
        {(o) => (
          <div className="grid min-w-0 gap-4 lg:grid-cols-2">
            <GoogleCard overview={o} />
            <ClickupCard overview={o} />
            <LlmCard overview={o} />
            <SuggestionsCard />
            <div className="lg:col-span-2">
              <RecentRuns overview={o} />
            </div>
          </div>
        )}
      </QueryState>
    </div>
  );
}
