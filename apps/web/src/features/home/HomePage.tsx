// Inicio: el dashboard principal (rediseño "Ámbar"). Muestra poco y bien: saludo, cuatro lecturas rápidas, el botón del
// check-in y "Objetivos de hoy" con interruptores. Las métricas completas siguen en /dashboard (menú Objetivos →
// Estadísticas) para no saturar la pantalla que abres cada mañana.
import { useState } from "react";
import { Link } from "react-router";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { formatLongDate, formatMoney, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { useDashboard } from "../dashboard/DashboardPage";
import { asToday, useHabitMutations } from "../habits/habitsApi";
import { formatNumber } from "../today/numbers";
import { useDayView } from "../today/TodayPage";
import { HeroScene, PILLARS, type PillarKey } from "./HeroScene";

type Dashboard = Schemas["Dashboard"];
type DayView = Schemas["DayView"];
type HabitDay = Schemas["HabitDay"];

/** Hora local del usuario (no la del navegador): el saludo y el check-in sugerido dependen de su zona. */
export function localHour(tz: string, now: Date = new Date()): number {
  return Number(
    new Intl.DateTimeFormat("en-US", { hour: "numeric", hourCycle: "h23", timeZone: tz }).format(now),
  );
}

export function greeting(hour: number): string {
  if (hour < 12) return "Buenos días";
  if (hour < 19) return "Buenas tardes";
  return "Buenas noches";
}

/** Días seguidos (sin contar hoy, que sigue abierto) con algún registro: hábitos hechos o energía anotada. */
export function recordStreak(days: Schemas["HeatDay"][], today: string): number {
  let n = 0;
  for (const d of [...days].filter((x) => x.date < today).sort((a, b) => b.date.localeCompare(a.date))) {
    if (d.done > 0 || (d.energy !== null && d.energy !== undefined)) n++;
    else break;
  }
  return n;
}

/** Hábitos que tocan hoy (los de los cuatro grupos, sin repetir) y cuántos están hechos. */
export function todaysHabits(view: DayView): HabitDay[] {
  const all = [...view.habits.fixed, ...view.habits.month, ...view.habits.week, ...view.habits.other];
  const seen = new Set<string>();
  return all.filter((h) => {
    if (seen.has(h.habit.id)) return false;
    seen.add(h.habit.id);
    return asToday(h.progress).shown !== false;
  });
}

export function HomePage() {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const dash = useDashboard(today, "same_span");
  const day = useDayView(today);
  const hour = localHour(tz);
  const first = me?.user.display_name?.trim().split(/\s+/)[0];

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="font-serif text-[15px] text-muted first-letter:uppercase">{formatLongDate(today)}</p>
          <h1 className="mt-1 font-serif text-3xl font-normal tracking-tight md:text-[40px]">
            {greeting(hour)}
            {first ? `, ${first}` : ""}
          </h1>
        </div>
      </div>

      {(dash.isPending || day.isPending) && <LoadingState rows={5} label="Preparando tu día…" />}
      {dash.isError && <ErrorState error={dash.error} onRetry={() => dash.refetch()} />}
      {dash.data && day.data && <Body d={dash.data} view={day.data} today={today} hour={hour} />}
    </div>
  );
}

function Body({ d, view, today, hour }: { d: Dashboard; view: DayView; today: string; hour: number }) {
  const { me } = useAuth();
  const currency = me?.settings.currency ?? "MXN";
  const habits = todaysHabits(view);
  const required = habits.filter((h) => asToday(h.progress).required_today);
  const doneReq = required.filter((h) => h.entry?.status === "done").length;
  const last7 = d.energy.slice(-7);
  const energyAvg = last7.length ? last7.reduce((a, p) => a + p.energy, 0) / last7.length : null;
  const streak = recordStreak(d.last_30_days, today);

  return (
    <div className="grid gap-6 lg:grid-cols-[minmax(0,380px)_minmax(0,1fr)]">
      {/* En el teléfono lo primero es la acción del día (check-in y objetivos); las lecturas van después. */}
      <div className="order-2 space-y-3 lg:order-1">
        <EnergyGauge avg={energyAvg} days={last7.length} />
        <div className="grid grid-cols-2 gap-3">
          <Panel
            title="Hábitos de hoy"
            sub={required.length ? `${doneReq} de ${required.length} exigibles` : "Nada exigible hoy"}
          >
            <Ring done={doneReq} total={required.length} />
          </Panel>
          <Panel title="Racha" sub="Días seguidos con registro">
            <p className="mt-4 text-center text-[40px] font-light leading-none tabular-nums">{streak}</p>
            <p className="mt-1 text-center text-xs text-muted">{streak === 1 ? "día" : "días"}</p>
          </Panel>
        </div>
        <Last7 days={d.last_30_days.filter((x) => x.date <= today).slice(-7)} />
        <CheckinCta view={view} today={today} hour={hour} className="hidden lg:flex" />
      </div>

      <div className="order-1 space-y-6 lg:order-2">
        <CheckinCta view={view} today={today} hour={hour} className="lg:hidden" />
        <Hero d={d} energyAvg={energyAvg} currency={currency} />
        <TodayGoals habits={habits} today={today} />
      </div>
    </div>
  );
}

function Panel({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) {
  return (
    <section className="rounded-[20px] border border-line bg-linear-to-b from-white/[0.035] to-white/[0.01] p-4">
      <h2 className="text-[13px] font-medium">{title}</h2>
      {sub && <p className="mt-0.5 text-[11px] text-muted">{sub}</p>}
      {children}
    </section>
  );
}

/** Medidor semicircular de energía (escala 1–5), como el control de clima de la referencia. */
function EnergyGauge({ avg, days }: { avg: number | null; days: number }) {
  const t = avg === null ? 0 : (avg - 1) / 4;
  const angle = Math.PI * (1 - t);
  const x = 150 + 120 * Math.cos(angle);
  const y = 130 - 120 * Math.sin(angle);
  return (
    <Panel title="Energía" sub={days ? `Promedio de tus últimos ${days} registros` : "Aún sin registros"}>
      <svg
        viewBox="0 0 300 145"
        className="mx-auto mt-1 block w-full max-w-[300px]"
        role="img"
        aria-label={avg === null ? "Sin datos de energía" : `Energía promedio ${formatNumber(avg, 1)} de 5`}
      >
        <path d="M30 130 A120 120 0 0 1 270 130" fill="none" stroke="var(--c-border)" strokeWidth="2" />
        {avg !== null && (
          <>
            <path
              d={`M30 130 A120 120 0 0 1 ${x.toFixed(1)} ${y.toFixed(1)}`}
              fill="none"
              stroke="var(--c-accent)"
              strokeWidth="3"
              strokeLinecap="round"
            />
            <circle cx={x} cy={y} r="8" fill="var(--c-accent)" />
          </>
        )}
        <text x="150" y="108" textAnchor="middle" fill="var(--c-text)" fontSize="34" fontWeight="300">
          {avg === null ? "—" : formatNumber(avg, 1)}
        </text>
        <text x="150" y="128" textAnchor="middle" fill="var(--c-muted)" fontSize="11">
          de 5
        </text>
      </svg>
    </Panel>
  );
}

function Ring({ done, total }: { done: number; total: number }) {
  const c = 2 * Math.PI * 40;
  const frac = total ? done / total : 0;
  return (
    <svg
      viewBox="0 0 100 100"
      className="mx-auto mt-2 block size-24"
      role="img"
      aria-label={`${done} de ${total} hechos`}
    >
      <circle cx="50" cy="50" r="40" fill="none" stroke="var(--c-border)" strokeWidth="6" />
      {total > 0 && done > 0 && (
        <circle
          cx="50"
          cy="50"
          r="40"
          fill="none"
          stroke="var(--c-accent)"
          strokeWidth="6"
          strokeLinecap="round"
          strokeDasharray={`${(c * frac).toFixed(1)} ${c.toFixed(1)}`}
          transform="rotate(-90 50 50)"
        />
      )}
      <text x="50" y="57" textAnchor="middle" fill="var(--c-text)" fontSize="20" fontWeight="300">
        {total ? `${done}/${total}` : "—"}
      </text>
    </svg>
  );
}

const WEEKDAY = ["D", "L", "M", "M", "J", "V", "S"];

/** Barras de cumplimiento de los últimos 7 días (la barra de hoy se resalta). */
function Last7({ days }: { days: Schemas["HeatDay"][] }) {
  return (
    <Panel title="Últimos 7 días" sub="Hábitos cumplidos de los exigibles">
      <div
        className="mt-4 flex h-24 items-end gap-2"
        role="img"
        aria-label={days.map((d) => `${d.date}: ${d.done}/${d.required}`).join(", ")}
      >
        {days.map((d, i) => {
          const ratio = d.required ? d.done / d.required : 0;
          const isLast = i === days.length - 1;
          return (
            <div key={d.date} className="flex h-full flex-1 flex-col items-center justify-end gap-1.5">
              <div
                className={`w-full rounded-t-lg rounded-b ${isLast ? "bg-accent" : "bg-surface-2"}`}
                style={{ height: `${Math.max(8, ratio * 100)}%` }}
              />
              <span className="text-[10px] text-muted">
                {WEEKDAY[new Date(`${d.date}T12:00:00`).getDay()]}
              </span>
            </div>
          );
        })}
      </div>
    </Panel>
  );
}

/** Botón principal: sugiere la parte del check-in según la hora y avisa si ya está hecha. */
function CheckinCta({
  view,
  today,
  hour,
  className = "",
}: {
  view: DayView;
  today: string;
  hour: number;
  className?: string;
}) {
  const morningDone = !!view.checkin?.morning_done_at;
  const eveningDone = !!view.checkin?.evening_done_at;
  const evening = hour >= 15 || morningDone;
  if (morningDone && eveningDone)
    return (
      <div
        className={`flex items-center justify-between gap-3 rounded-[18px] border border-line bg-surface p-4 ${className}`}
      >
        <span className="text-sm">Check-in de hoy completo ✓</span>
        <Link to={`/hoy?date=${today}`} className="text-sm">
          Ver →
        </Link>
      </div>
    );
  return (
    <Link
      to={`/hoy?date=${today}&guiado=${evening ? "noche" : "manana"}`}
      className={`flex items-center justify-between gap-3 rounded-[18px] bg-accent px-5 py-4 font-semibold text-accent-fg no-underline shadow-glow hover:brightness-105 ${className}`}
    >
      <span>{evening ? "Cerrar el día" : "Empezar check-in de la mañana"}</span>
      <span className="text-right text-xs font-normal opacity-80">4 pasos · 2 min →</span>
    </Link>
  );
}

function Hero({ d, energyAvg, currency }: { d: Dashboard; energyAvg: number | null; currency: string }) {
  const [pillar, setPillar] = useState<PillarKey>("cuerpo");
  const w = d.weight;
  const dev = d.devotional;
  const lastPeace = d.peace.at(-1);
  const habits30 = d.last_30_days.reduce((a, x) => ({ done: a.done + x.done, req: a.req + x.required }), {
    done: 0,
    req: 0,
  });
  const chips: Record<PillarKey, string[]> = {
    cuerpo: [
      w.last ? `⚖ ${formatNumber(w.last.kg, 1)} kg` : "⚖ Sin peso",
      w.change_week !== null && w.change_week !== undefined
        ? `Semana ${w.change_week > 0 ? "+" : ""}${formatNumber(w.change_week, 1)} kg`
        : null,
      energyAvg !== null ? `⚡ Energía ${formatNumber(energyAvg, 1)}/5` : null,
    ].filter((x): x is string => !!x),
    mente: [
      `${d.month_goals.length} objetivos del mes`,
      `${d.week_goals.length} de la semana`,
      habits30.req
        ? `Hábitos 30 días ${Math.round((habits30.done / habits30.req) * 100)}%`
        : "Sin hábitos exigibles",
    ],
    dinero: [
      `Semana ${formatMoney(d.finance.week.consumption_cents, currency)}`,
      `Mes ${formatMoney(d.finance.month.consumption_cents, currency)}`,
      `Ingreso ${formatMoney(d.finance.month.earned_income_cents, currency)}`,
    ],
    espiritu: [
      dev
        ? `Devocional: ${dev.failures_total}${dev.max_allowed !== null && dev.max_allowed !== undefined ? `/${dev.max_allowed}` : ""} fallas`
        : "Sin hábito devocional",
      lastPeace ? `Paz ${lastPeace.peace}/10` : "Paz sin registrar",
    ],
  };
  // PILLARS es una lista fija con las cuatro claves, así que siempre hay coincidencia; el respaldo calma a TS.
  const p = PILLARS.find((x) => x.key === pillar) ?? { label: "", sub: "", route: "/" };
  return (
    <HeroScene pillar={pillar}>
      <div className="flex flex-wrap gap-1.5" role="tablist" aria-label="Áreas de tu vida">
        {PILLARS.map((x) => (
          <button
            key={x.key}
            type="button"
            role="tab"
            aria-selected={x.key === pillar}
            onClick={() => setPillar(x.key)}
            className={`cursor-pointer rounded-full border px-3.5 py-1.5 text-xs backdrop-blur-md ${x.key === pillar ? "border-accent bg-black/50 text-fg" : "border-line bg-black/40 text-muted hover:text-fg"}`}
          >
            {x.label}
          </button>
        ))}
      </div>
      <div className="mt-auto flex flex-wrap items-end justify-between gap-4" role="tabpanel">
        <div>
          <h2 className="text-[28px] font-normal leading-tight md:text-[32px]">{p.label}</h2>
          <p className="mt-1 text-sm text-muted">{p.sub}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {chips[pillar].map((c) => (
            <span
              key={c}
              className="rounded-full border border-line bg-black/55 px-3 py-1.5 text-xs backdrop-blur-md"
            >
              {c}
            </span>
          ))}
          <Link
            to={p.route}
            className="rounded-full border border-accent/60 bg-black/55 px-3 py-1.5 text-xs text-link no-underline backdrop-blur-md"
          >
            Abrir →
          </Link>
        </div>
      </div>
    </HeroScene>
  );
}

/** Objetivos de hoy: los hábitos que tocan hoy, con un interruptor grande que guarda al instante. */
function TodayGoals({ habits, today }: { habits: HabitDay[]; today: string }) {
  const { setEntry, clearEntry } = useHabitMutations();
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const done = habits.filter((h) => h.entry?.status === "done").length;

  const toggle = async (h: HabitDay) => {
    setBusy(h.habit.id);
    try {
      if (h.entry?.status === "done") await clearEntry.mutateAsync({ id: h.habit.id, date: today });
      else
        await setEntry.mutateAsync({
          id: h.habit.id,
          date: today,
          body: { status: "done", version: h.entry?.version ?? null },
        });
    } catch {
      toast.show("No se pudo guardar. Revisa tu conexión e inténtalo de nuevo.", { tone: "error" });
    } finally {
      setBusy(null);
    }
  };

  return (
    <section aria-labelledby="hoy-objetivos">
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
        <h2 id="hoy-objetivos" className="text-lg font-normal">
          Objetivos de hoy{" "}
          {habits.length > 0 && (
            <span className="text-muted">
              / {done} de {habits.length} hechos
            </span>
          )}
        </h2>
        <Link to="/objetivos" className="text-xs text-muted">
          Ver todos los objetivos →
        </Link>
      </div>
      {habits.length === 0 ? (
        <div className="rounded-[18px] border border-dashed border-line p-5 text-sm text-muted">
          Aún no tienes hábitos para hoy. <Link to="/objetivos/planificar">Planifica tus objetivos</Link> y
          aquí aparecerán con un interruptor para marcarlos.
        </div>
      ) : (
        <ul className="divide-y divide-line rounded-[18px] border border-line bg-surface/60">
          {habits.map((h) => {
            const t = asToday(h.progress);
            const isDone = h.entry?.status === "done";
            const detail = [
              h.goal_title ?? (h.habit.kind === "fixed" ? "Hábito fijo" : null),
              t.week_target ? `${t.week_done ?? 0}/${t.week_target} esta semana` : null,
              !t.required_today ? "opcional hoy" : null,
            ].filter(Boolean);
            return (
              <li key={h.habit.id} className="flex items-center gap-4 px-4 py-3.5">
                <div className="min-w-0 flex-1">
                  <p className={`text-[15px] ${isDone ? "text-muted line-through decoration-1" : ""}`}>
                    {h.habit.name}
                  </p>
                  {detail.length > 0 && <p className="mt-0.5 text-xs text-muted">{detail.join(" · ")}</p>}
                </div>
                <button
                  type="button"
                  role="switch"
                  aria-checked={isDone}
                  aria-label={`${h.habit.name}: ${isDone ? "hecho" : "pendiente"}`}
                  disabled={busy === h.habit.id}
                  onClick={() => void toggle(h)}
                  className={`relative h-8 w-14 shrink-0 cursor-pointer rounded-full border transition-colors disabled:opacity-60 ${isDone ? "border-accent bg-accent" : "border-line bg-surface-2"}`}
                >
                  <span
                    className={`absolute top-1 size-[22px] rounded-full transition-all ${isDone ? "left-[29px] bg-white" : "left-1 bg-muted"}`}
                  />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
