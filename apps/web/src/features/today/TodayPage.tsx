// Hoy: check-in diario (mañana y noche sobre el mismo registro). Guardado parcial con borrador en el dispositivo,
// cola offline con Idempotency-Key y versión base, y resolución explícita de conflictos (409).
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { DatePicker } from "../../components/ui/DatePicker";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { TextArea, TextField } from "../../components/ui/Field";
import { SaveStatus } from "../../components/ui/SaveStatus";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { isNetworkError, unwrap } from "../../lib/errors";
import { formatLongDate, formatMoney, todayIso } from "../../lib/format";
import { loadSnapshot, saveSnapshot } from "../../lib/offline/db";
import { useDraft, useSyncQueue } from "../../lib/offline/hooks";
import { getSession } from "../../lib/session";
import { isValidIsoDate } from "../../lib/telegram";
import { useAuth } from "../auth/AuthProvider";
import { HabitStatusControl } from "../habits/HabitStatusControl";
import { asToday } from "../habits/habitsApi";
import {
  buildPatch,
  type CheckinDraft,
  type CheckinForm,
  draftToLocal,
  ENERGY_LABEL,
  formFromFull,
  type HabitMark,
  isDirty,
  patchView,
  serverAsPatch,
} from "./checkin";
import { GuidedCheckin, type GuidedPart } from "./GuidedCheckin";
import { ListEditor } from "./ListEditor";
import { parsePesosToCents } from "./numbers";

type DayView = Schemas["DayView"];
type HabitDay = Schemas["HabitDay"];

const CONFLICT_LABELS: Record<string, string> = {
  weight_kg: "Peso",
  energy: "Energía",
  planned_spend_text: "Planeo gastar",
  planned_spend_cents: "Monto planeado (centavos)",
  planned_spend_none: "No planeo gastar",
  notes_md: "Notas",
  log: "Log",
  went_well: "Lo que hice bien",
  went_badly: "Lo que hice mal",
  morning_done: "Mañana lista",
  evening_done: "Día cerrado",
  habits: "Hábitos",
};

/** Estado visible: conflicto, error y pendiente mandan sobre "borrador"; "guardado" solo si no hay cambios locales. */
export function displayStatus(
  status: "draft" | "pending" | "saved" | "conflict" | "error" | null,
  dirty: boolean,
): "draft" | "pending" | "saved" | "conflict" | "error" | null {
  if (status === "conflict" || status === "error" || status === "pending") return status;
  return dirty ? "draft" : status;
}

export function useDayView(date: string) {
  return useQuery({
    queryKey: ["day", date],
    // Se ejecuta aun sin red: el queryFn cae a la última vista guardada en este dispositivo.
    networkMode: "always",
    queryFn: async () => {
      const userId = getSession().userId;
      try {
        const data = unwrap(await api.GET("/checkins/{date}/view", { params: { path: { date } } }));
        if (userId) void saveSnapshot(userId, `day:${date}`, data).catch(() => {});
        return data;
      } catch (e) {
        // Sin red: usar la última vista conocida de ese día (los cambios siguen en el borrador/cola local).
        if (isNetworkError(e) && userId) {
          const snap = await loadSnapshot<DayView>(userId, `day:${date}`).catch(() => null);
          if (snap) return snap.data;
        }
        throw e;
      }
    },
  });
}

function timeIn(tz: string, instant: string | null | undefined) {
  if (!instant) return "";
  return new Intl.DateTimeFormat("es-MX", { hour: "2-digit", minute: "2-digit", timeZone: tz }).format(
    new Date(instant),
  );
}

export function TodayPage() {
  const { me } = useAuth();
  const [params, setParams] = useSearchParams();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const requested = params.get("date");
  const date = isValidIsoDate(requested) && requested <= today ? requested : today;
  const fromOldMessage = isValidIsoDate(requested) && requested !== today;
  const view = useDayView(date);
  // ?guiado=manana|noche (botones del bot y de Inicio): una pregunta por pantalla en vez del formulario completo.
  const g = params.get("guiado");
  const guided: GuidedPart | null = g === "manana" || g === "noche" ? g : null;

  if (guided)
    return (
      <div className="space-y-4">
        {fromOldMessage && (
          <p role="status" className="rounded-xl border border-warn bg-surface p-3 text-sm">
            Es el check-in del <strong>{formatLongDate(date)}</strong>, no el de hoy.
          </p>
        )}
        {view.isPending && <LoadingState rows={4} label="Cargando el día…" />}
        {view.isError && <ErrorState error={view.error} onRetry={() => view.refetch()} />}
        {view.data && <CheckinEditor key={date} date={date} view={view.data} tz={tz} guided={guided} />}
      </div>
    );

  return (
    <div className="space-y-4">
      <PageHeader
        title={date === today ? "Hoy" : "Check-in"}
        subtitle={<span className="capitalize">{formatLongDate(date)}</span>}
        actions={
          <DatePicker value={date} max={today} onChange={(d) => setParams(d === today ? {} : { date: d })} />
        }
      />
      {fromOldMessage && (
        <p role="status" className="rounded-xl border border-warn bg-surface p-3 text-sm">
          Estás editando el check-in del <strong>{formatLongDate(date)}</strong>, no el de hoy.{" "}
          <button type="button" className="font-semibold text-link underline" onClick={() => setParams({})}>
            Ir a hoy
          </button>
        </p>
      )}
      {me && !me.settings.onboarding_completed_at && (
        <Card>
          <p className="font-semibold">Termina de configurar tu app</p>
          <p className="mt-1 text-sm text-muted">
            Zona, áreas, módulos, hábitos fijos, recordatorios y metas opcionales. Puedes hacerlo después.
          </p>
          <Link to="/onboarding" className="mt-2 inline-block font-semibold">
            Configurar ahora →
          </Link>
        </Card>
      )}
      {view.isPending && <LoadingState rows={5} label="Cargando el día…" />}
      {view.isError && <ErrorState error={view.error} onRetry={() => view.refetch()} />}
      {view.data && <CheckinEditor key={date} date={date} view={view.data} tz={tz} />}
    </div>
  );
}

function CheckinEditor({
  date,
  view,
  tz,
  guided = null,
}: {
  date: string;
  view: DayView;
  tz: string;
  guided?: GuidedPart | null;
}) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const toast = useToast();
  const { me } = useAuth();
  const draftApi = useDraft<unknown>("checkin", date);
  const queue = useSyncQueue();
  const server = useMemo(() => formFromFull(view), [view]);
  const [local, setLocal] = useState<CheckinDraft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const version = draftApi.draft?.baseVersion ?? view.version;

  // Fuente del formulario: cambios locales sin sincronizar (borrador/pendiente/conflicto/error) o el servidor.
  useEffect(() => {
    if (!draftApi.loaded) return;
    const d = draftApi.draft;
    if (d && d.status !== "saved") {
      setLocal((cur) => cur ?? draftToLocal(d.data, server));
      return;
    }
    // Guardado confirmado: esperar a que llegue la versión nueva del servidor antes de reemplazar.
    if (d?.status === "saved" && d.baseVersion !== null && d.baseVersion !== view.version) return;
    // Sin borrador: conservar solo ediciones recientes sobre este mismo snapshot (aún dentro del retardo de guardado).
    setLocal((cur) =>
      cur && !d && isDirty(cur) && cur.base === server ? cur : { form: server, base: server },
    );
  }, [draftApi.loaded, draftApi.draft, server, view.version]);

  // Al confirmarse el guardado, refrescar los datos del día (vista, dashboard, hábitos).
  useEffect(() => {
    if (draftApi.status === "saved") {
      void qc.invalidateQueries({ queryKey: ["day", date] });
      void qc.invalidateQueries({ queryKey: ["dashboard"] });
      void qc.invalidateQueries({ queryKey: ["habits"] });
    }
  }, [draftApi.status, date, qc]);

  const conflict = queue.conflicts.find((c) => c.draftId === `checkin:${date}`);

  if (!local) return <LoadingState rows={3} />;
  const form = local.form;
  const dirty = isDirty(local);

  const update = (patch: Partial<CheckinForm>) => {
    const next = { ...local, form: { ...local.form, ...patch } };
    setLocal(next);
    setError(null);
    draftApi.saveDebounced(next, version);
  };
  const setHabit = (id: string, m: HabitMark) => update({ habits: { ...form.habits, [id]: m } });

  const save = async (extra: Partial<CheckinForm> = {}) => {
    const target: CheckinDraft = { ...local, form: { ...local.form, ...extra } };
    const r = buildPatch(target.form, target.base, version);
    if (!r.ok) {
      setError(r.error);
      return;
    }
    if (r.empty) {
      toast.show("No hay cambios que guardar");
      return;
    }
    setLocal(target);
    setSaving(true);
    try {
      const res = await draftApi.submit("PUT", `/checkins/${date}`, r.patch, version);
      if (!res) return;
      if (res.sent > 0 && res.remaining === 0) toast.show("Guardado", { tone: "ok" });
      else if (res.conflicts)
        toast.show("Hay un conflicto con otro dispositivo: revísalo", { tone: "error" });
      else if (res.errors) toast.show("No se pudo guardar: revisa los datos", { tone: "error" });
      else toast.show("Guardado en este dispositivo; se sincronizará al volver la conexión");
    } finally {
      setSaving(false);
    }
  };

  const habits = view.habits;
  const modules = (me?.settings.modules ?? {}) as Record<string, unknown>;
  const finance = modules.finance !== false;

  const conflictDialog = conflict && (
    <ConflictDialog
      open
      mine={patchView(conflict.body as Record<string, unknown>)}
      theirs={serverAsPatch(conflict.current as Schemas["CheckinFull"])}
      labels={CONFLICT_LABELS}
      onClose={() => undefined}
      onResolve={(r) => {
        void queue.resolve(conflict.id, r).then(() => {
          if (r.choice === "theirs") void qc.invalidateQueries({ queryKey: ["day", date] });
        });
      }}
    />
  );

  if (guided)
    return (
      <>
        <GuidedCheckin
          part={guided}
          date={date}
          view={view}
          form={form}
          update={update}
          setHabit={setHabit}
          save={save}
          saving={saving}
          finance={finance}
          error={error}
          timeIn={(i) => timeIn(tz, i)}
          onDone={() => navigate("/")}
        />
        {conflictDialog}
      </>
    );

  return (
    <div className="space-y-4">
      <div className="sticky top-0 z-10 -mx-1 flex flex-wrap items-center gap-2 rounded-xl bg-bg/95 px-1 py-2 backdrop-blur">
        <SaveStatus status={displayStatus(draftApi.status, dirty)} />
        {!queue.online && <Badge tone="warn">Sin conexión</Badge>}
        {view.is_open ? <Badge tone="accent">Día abierto</Badge> : <Badge>Día cerrado</Badge>}
        <div className="ml-auto flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void save()} loading={saving} disabled={!dirty}>
            Guardar
          </Button>
        </div>
      </div>
      {error && (
        <p role="alert" className="rounded-xl border border-danger p-3 text-sm text-danger">
          {error}
        </p>
      )}
      {draftApi.status === "error" && draftApi.draft?.error && (
        <p role="alert" className="rounded-xl border border-danger p-3 text-sm text-danger">
          El servidor rechazó el guardado: {draftApi.draft.error}. Corrige y vuelve a guardar, o{" "}
          <Link to="/sincronizacion">revisa la sincronización</Link>.
        </p>
      )}

      <Card title="Cómo estás">
        <div className="grid gap-4 sm:grid-cols-2">
          <fieldset>
            <legend className="mb-1 text-sm font-medium">Energía (1–5)</legend>
            <div className="flex flex-wrap gap-1.5">
              {[1, 2, 3, 4, 5].map((n) => (
                <label key={n}>
                  <input
                    type="radio"
                    name={`energy-${date}`}
                    className="peer sr-only"
                    checked={form.energy === n}
                    onChange={() => update({ energy: n })}
                  />
                  <span
                    title={ENERGY_LABEL[n]}
                    className="flex h-11 w-11 cursor-pointer items-center justify-center rounded-lg border border-line text-lg peer-checked:bg-accent peer-checked:text-accent-fg peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent"
                  >
                    {n}
                  </span>
                </label>
              ))}
              {form.energy !== null && (
                <Button variant="ghost" size="sm" onClick={() => update({ energy: null })}>
                  Sin dato
                </Button>
              )}
            </div>
            <p className="mt-1 text-xs text-muted">
              {form.energy ? ENERGY_LABEL[form.energy] : "Sin registrar (no es 0)"}
            </p>
          </fieldset>
          <TextField
            label="Peso (kg, opcional)"
            inputMode="decimal"
            placeholder="Sin registrar"
            value={form.weight_kg}
            onChange={(e) => update({ weight_kg: e.target.value })}
            hint="Déjalo vacío si no te pesaste. Un vínculo configurado marca tu hábito de pesarte."
          />
        </div>
      </Card>

      <HabitsSection
        title="Hábitos fijos"
        items={habits.fixed}
        form={form}
        onChange={setHabit}
        empty={
          <>
            Aún no configuras hábitos fijos. <Link to="/onboarding">Configúralos</Link> o en{" "}
            <Link to="/habitos">Hábitos</Link>.
          </>
        }
      />
      <HabitsSection
        title="Hábitos del mes"
        items={habits.month}
        form={form}
        onChange={setHabit}
        goals={view.month_goals}
        empty={
          <>
            Sin hábitos de objetivos del mes. <Link to="/objetivos/planificar">Planifica tus objetivos</Link>.
          </>
        }
      />
      <HabitsSection
        title="Hábitos de la semana"
        items={habits.week}
        form={form}
        onChange={setHabit}
        goals={view.week_goals}
        empty={
          <>
            Sin hábitos de la semana. <Link to="/objetivos/planificar">Planifica la semana</Link>.
          </>
        }
      />
      {habits.other.length > 0 && (
        <HabitsSection title="Otros hábitos" items={habits.other} form={form} onChange={setHabit} />
      )}

      {view.devotional_enabled && <DevotionalCard view={view} date={date} />}

      <Card title="Agenda">
        {view.agenda.length === 0 ? (
          <p className="text-sm text-muted">
            Sin eventos para este día. <Link to="/agenda">Agregar a la agenda</Link>
          </p>
        ) : (
          <ul className="space-y-1.5">
            {view.agenda.map((e) => (
              <li key={e.id} className="flex flex-wrap gap-x-2 text-sm">
                <span className="w-24 shrink-0 tabular-nums text-muted">
                  {e.all_day ? "Todo el día" : `${timeIn(tz, e.starts_at)}–${timeIn(tz, e.ends_at)}`}
                </span>
                <span className="min-w-0 flex-1 break-words">{e.title}</span>
                {e.source === "google" && <Badge>Calendar</Badge>}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-2 text-xs text-muted">
          La agenda es una propuesta: un evento no confirma que asististe ni con quién hablaste.
        </p>
      </Card>

      {finance && (
        <SpendCard
          date={date}
          form={form}
          update={update}
          currency={view.currency}
          spent={view.derived.expenses_total_cents}
          spentCount={view.derived.expenses_count}
        />
      )}

      <Card title="Tareas por área" actions={<Link to={`/tareas?date=${date}`}>Abrir tareas →</Link>}>
        {view.tasks_by_area.every((a) => a.tasks.length === 0) ? (
          <p className="text-sm text-muted">Sin pendientes para este día.</p>
        ) : (
          <div className="space-y-3">
            {view.tasks_by_area
              .filter((a) => a.tasks.length)
              .map((a) => (
                <div key={a.area_id ?? a.area_name}>
                  <h3 className="text-sm font-semibold">{a.area_name}</h3>
                  <ul className="mt-1 space-y-1">
                    {a.tasks.map((t) => (
                      <li key={t.id} className="flex flex-wrap items-baseline gap-2 text-sm">
                        <span aria-hidden>{t.status === "done" ? "☑" : "☐"}</span>
                        <span
                          className={`min-w-0 flex-1 break-words ${t.status === "done" ? "line-through text-muted" : ""}`}
                        >
                          {t.title}
                        </span>
                        {t.ease_hint && <Badge tone="ok">fácil</Badge>}
                        {t.external_status_category === "need_info" && <Badge tone="warn">need info</Badge>}
                        {t.planned_on && t.planned_on < date && t.status !== "done" && (
                          <Badge tone="muted">arrastrada</Badge>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
          </div>
        )}
      </Card>

      <Card title="Log del día">
        <ListEditor
          label="Lo que pasó"
          items={form.log}
          onChange={(log) => update({ log })}
          placeholder="Ej. Terminé el reporte del cliente"
          emptyHint="Bitácora breve en pasado. Lo automático va abajo y no se duplica."
        />
        <Derived view={view} tz={tz} />
      </Card>

      <Card title="Personas" actions={<Link to="/personas">Personas →</Link>}>
        {view.people.length === 0 ? (
          <p className="text-sm text-muted">
            Sin interacciones registradas este día. Regístralas en la ficha de cada persona.
          </p>
        ) : (
          <ul className="space-y-1 text-sm">
            {view.people.map((p) => (
              <li key={p.id}>
                <Link to={`/personas/${p.person_id}`} className="font-medium">
                  {p.person_name}
                </Link>
                {p.topic && <span className="text-muted">: {p.topic}</span>}
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card title="Reflexión">
        <p className="mb-3 text-sm text-muted">
          Respecto a tus objetivos del mes y de la semana. Solo lo que tú escribas.
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <ListEditor
            label="Lo que hice mal hoy"
            items={form.went_badly}
            onChange={(went_badly) => update({ went_badly })}
          />
          <ListEditor
            label="Lo que hice bien hoy"
            items={form.went_well}
            onChange={(went_well) => update({ went_well })}
          />
        </div>
        <TextArea
          className="mt-4"
          label="Notas (opcional)"
          rows={3}
          value={form.notes_md}
          onChange={(e) => update({ notes_md: e.target.value })}
        />
      </Card>

      <Card title="Cerrar">
        <div className="flex flex-wrap gap-2">
          <Button
            variant={form.morning_done ? "secondary" : "primary"}
            onClick={() => void save({ morning_done: !form.morning_done })}
            loading={saving}
          >
            {form.morning_done ? "Mañana marcada ✓ (deshacer)" : "Guardar y marcar la mañana"}
          </Button>
          <Button
            variant={form.evening_done ? "secondary" : "primary"}
            onClick={() => void save({ evening_done: !form.evening_done })}
            loading={saving}
          >
            {form.evening_done ? "Día cerrado ✓ (reabrir)" : "Guardar y cerrar el día"}
          </Button>
        </div>
        <p className="mt-2 text-xs text-muted">
          Cerrar el día cancela el recordatorio nocturno. No necesitas llenar todo: las secciones vacías
          quedan "sin dato".
        </p>
      </Card>

      {conflictDialog}
    </div>
  );
}

function HabitsSection({
  title,
  items,
  form,
  onChange,
  goals,
  empty,
}: {
  title: string;
  items: HabitDay[];
  form: CheckinForm;
  onChange: (id: string, m: HabitMark) => void;
  goals?: Schemas["GoalBrief"][];
  empty?: React.ReactNode;
}) {
  const done = items.filter((h) => form.habits[h.habit.id]?.status === "done").length;
  return (
    <Card
      title={title}
      actions={
        items.length > 0 && <span className="text-sm text-muted">{`${done}/${items.length} hoy`}</span>
      }
    >
      {goals && goals.length > 0 && (
        <p className="mb-2 text-sm text-muted">
          Objetivos:{" "}
          {goals.map((g, i) => (
            <span key={g.id}>
              {i > 0 && " · "}
              <Link to={`/objetivos/${g.id}`}>{g.title}</Link>
            </span>
          ))}
        </p>
      )}
      {items.length === 0 ? (
        <p className="text-sm text-muted">{empty}</p>
      ) : (
        <div className="grid gap-2 md:grid-cols-2">
          {items.map((h) => {
            const p = asToday(h.progress);
            const bits = [
              h.goal_title ? `· ${h.goal_title}` : null,
              p.week_target ? `${p.week_done ?? 0}/${p.week_target} esta semana` : null,
              !p.required_today && !p.week_target ? "No exigible hoy" : null,
              h.target_quantity ? `meta ${Number(h.target_quantity)} ${h.habit.unit ?? ""}` : null,
            ].filter(Boolean);
            return (
              <HabitStatusControl
                key={h.habit.id}
                name={h.habit.name}
                detail={bits.join(" · ") || undefined}
                unit={h.habit.unit}
                mark={form.habits[h.habit.id] ?? { status: null, reason: "", quantity: "" }}
                onChange={(m) => onChange(h.habit.id, m)}
              />
            );
          })}
        </div>
      )}
    </Card>
  );
}

function DevotionalCard({ view, date }: { view: DayView; date: string }) {
  const d = view.devotional;
  const label = view.devotional_mode === "meditation" ? "Meditación" : "Devocional";
  const hasContent =
    !!d && [d.passage, d.learned, d.teaching, d.prayer, d.song].some((x) => x.trim().length > 0);
  return (
    <Card title={label} actions={<Link to={`/devocionales/${date}`}>{d ? "Abrir" : "Escribir"} →</Link>}>
      {d ? (
        <div className="text-sm">
          {d.passage && (
            <p>
              <span className="text-muted">Pasaje:</span> {d.passage}
            </p>
          )}
          {d.learned && <p className="mt-1 line-clamp-3">{d.learned}</p>}
          {!hasContent && <p className="text-muted">Creado pero sin contenido: no marca el hábito.</p>}
        </div>
      ) : (
        <p className="text-sm text-muted">
          Sin escribir todavía. Un {label.toLowerCase()} con contenido real marca el hábito.
        </p>
      )}
    </Card>
  );
}

function SpendCard({
  date,
  form,
  update,
  currency,
  spent,
  spentCount,
}: {
  date: string;
  form: CheckinForm;
  update: (p: Partial<CheckinForm>) => void;
  currency: string;
  spent: number;
  spentCount: number;
}) {
  const cents = parsePesosToCents(form.planned_spend_amount);
  const amount = cents.ok && cents.value !== null && !form.planned_spend_none ? cents.value : null;
  const suggestion = useQuery({
    queryKey: ["card-suggestion", date, amount],
    queryFn: async () =>
      unwrap(
        await api.GET("/finance/card-suggestion", {
          params: { query: amount !== null ? { date, amount_cents: amount } : { date } },
        }),
      ),
    enabled: !form.planned_spend_none,
  });
  const [showAll, setShowAll] = useState(false);
  const s = suggestion.data;
  return (
    <Card title="Gasto planeado" actions={<Link to="/finanzas/movimientos">Registrar gasto →</Link>}>
      <div className="grid gap-3 sm:grid-cols-[1fr_10rem]">
        <TextField
          label="¿Qué planeas gastar?"
          value={form.planned_spend_text}
          disabled={form.planned_spend_none}
          onChange={(e) => update({ planned_spend_text: e.target.value })}
          placeholder="Ej. comida con Ana"
        />
        <TextField
          label="Monto (MXN)"
          inputMode="decimal"
          value={form.planned_spend_amount}
          disabled={form.planned_spend_none}
          onChange={(e) => update({ planned_spend_amount: e.target.value })}
          error={cents.ok ? undefined : cents.error}
          placeholder="Opcional"
        />
      </div>
      <label className="mt-2 flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={form.planned_spend_none}
          onChange={(e) => update({ planned_spend_none: e.target.checked })}
          className="h-5 w-5"
        />
        No planeo gastar nada (distinto de no responder)
      </label>
      {spentCount > 0 && (
        <p className="mt-2 text-sm text-muted">
          Gastado este día: {formatMoney(spent, currency)} en {spentCount} movimiento(s) confirmados.
        </p>
      )}
      {!form.planned_spend_none && (
        <div className="mt-3 rounded-xl bg-surface-2 p-3 text-sm" aria-live="polite">
          {suggestion.isPending && <p>Calculando tarjeta sugerida…</p>}
          {suggestion.isError && <p className="text-danger">No se pudo calcular la tarjeta sugerida.</p>}
          {s && (
            <>
              <p className="font-semibold">
                Tarjeta sugerida: {s.card_name ?? s.debit_account_name ?? "Ninguna disponible"}
                {s.provisional && <Badge tone="warn">provisional</Badge>}
              </p>
              <p className="mt-1">{s.explanation}</p>
              {s.provisional && <p className="mt-1 text-muted">Sin monto la sugerencia es provisional.</p>}
              {s.cards.length > 0 && (
                <button
                  type="button"
                  className="mt-2 text-link underline"
                  onClick={() => setShowAll((v) => !v)}
                >
                  {showAll ? "Ocultar" : "Ver"} razones por tarjeta ({s.cards.length})
                </button>
              )}
              {showAll && (
                <ul className="mt-2 space-y-1">
                  {s.cards.map((c) => (
                    <li key={c.account_id}>
                      <span className={c.eligible ? "text-ok" : "text-muted"}>{c.eligible ? "✓" : "✕"}</span>{" "}
                      <strong>{c.name}</strong>
                      {c.days_to_cut !== null &&
                        c.days_to_cut !== undefined &&
                        ` · ${c.days_to_cut} días al corte`}
                      {c.usage_after_pct !== null &&
                        c.usage_after_pct !== undefined &&
                        ` · uso ${c.usage_after_pct}%`}
                      {c.reasons.length > 0 && <span className="text-muted"> — {c.reasons.join("; ")}</span>}
                    </li>
                  ))}
                </ul>
              )}
              <p className="mt-2 text-xs text-muted">
                Regla personal (uso ≤ {s.max_usage_pct}%); no consulta bancos ni ejecuta pagos.
              </p>
            </>
          )}
        </div>
      )}
    </Card>
  );
}

function Derived({ view, tz }: { view: DayView; tz: string }) {
  const d = view.derived;
  const any = d.closed_tasks.length || d.past_events.length || d.devlog_sessions.length || d.expenses_count;
  if (!any) return null;
  return (
    <div className="mt-4 rounded-xl bg-surface-2 p-3 text-sm">
      <p className="font-medium">Del día (automático; no se guarda en tu log)</p>
      <ul className="mt-1 list-disc space-y-0.5 pl-5">
        {d.closed_tasks.map((t) => (
          <li key={t.id}>Cerré: {t.title}</li>
        ))}
        {d.past_events.map((e) => (
          <li key={e.id}>
            {e.all_day ? "" : `${timeIn(tz, e.starts_at)} `}
            {e.title}
          </li>
        ))}
        {d.devlog_sessions.map((s) => (
          <li key={s.id}>
            Devlog: <Link to={`/dev?date=${s.on_date}`}>{s.repo || "sesión"}</Link>
            {s.branch && ` (${s.branch})`}
          </li>
        ))}
        {d.expenses_count > 0 && (
          <li>
            Gastos: {formatMoney(d.expenses_total_cents, view.currency)} ({d.expenses_count})
          </li>
        )}
      </ul>
    </div>
  );
}
