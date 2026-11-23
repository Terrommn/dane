// Inicio: el dashboard principal (rediseño "Ámbar", versión "planos"). Escritorio: planos de un edificio animados de
// fondo, saludo grande con el resumen del día en una frase, cuatro tarjetas y "Objetivos de hoy". Teléfono: solo lo de
// hoy (saludo, botón del check-in, casillas y el objetivo del mes), sin fondo animado.
// Las métricas completas siguen en /dashboard (Objetivos → Estadísticas) para no saturar la pantalla de cada mañana.
import { useState } from "react";
import { Link } from "react-router";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { formatLongDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { useDashboard } from "../dashboard/DashboardPage";
import { asProgress, describeProgress } from "../goals/goalsApi";
import { asToday, useHabitMutations } from "../habits/habitsApi";
import { formatNumber } from "../today/numbers";
import { useDayView } from "../today/TodayPage";
import { Blueprint } from "./Blueprint";

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

/** Hábitos que tocan hoy (los de los cuatro grupos, sin repetir). */
export function todaysHabits(view: DayView): HabitDay[] {
  const all = [...view.habits.fixed, ...view.habits.month, ...view.habits.week, ...view.habits.other];
  const seen = new Set<string>();
  return all.filter((h) => {
    if (seen.has(h.habit.id)) return false;
    seen.add(h.habit.id);
    return asToday(h.progress).shown !== false;
  });
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * El día en una frase ("Día 23 de «Volver a mi prime». Hoy tienes 3 hábitos, 2 tareas y Junta a las 11:00.").
 * Por qué una frase y no más tarjetas: el usuario quería que Inicio se sintiera menos "panel de control".
 */
export function dayBrief(opts: {
  goalTitle?: string;
  dayOfMonth: number;
  habits: number;
  tasks: number;
  nextEvent?: { title: string; time: string };
  events: number;
}): string {
  const lead = opts.goalTitle ? `Día ${opts.dayOfMonth} de «${opts.goalTitle}». ` : "";
  const bits = [
    opts.habits ? plural(opts.habits, "hábito", "hábitos") : null,
    opts.tasks ? plural(opts.tasks, "tarea", "tareas") : null,
    opts.nextEvent
      ? `${opts.nextEvent.title} a las ${opts.nextEvent.time}`
      : opts.events
        ? plural(opts.events, "evento", "eventos")
        : null,
  ].filter((b): b is string => !!b);
  if (!bits.length) return `${lead}Hoy no tienes nada agendado: buen día para avanzar en lo importante.`;
  const list = bits.length > 1 ? `${bits.slice(0, -1).join(", ")} y ${bits.at(-1)}` : bits[0];
  return `${lead}Hoy tienes ${list}.`;
}

/** Brillo ámbar al pasar el mouse (el usuario lo pidió para todas las tarjetas de Inicio). */
// Entra casi al instante (75 ms) y sale despacio (500 ms): así un paso rápido del mouse sí deja ver el brillo.
const HOVER =
  "transition duration-500 hover:duration-75 hover:-translate-y-0.5 hover:border-accent/55 hover:shadow-[0_0_0_1px_rgb(240_122_46/0.15),0_0_40px_rgb(240_122_46/0.18)]";
// Sin backdrop-blur a propósito: desenfocar el fondo animado en cada cuadro trababa el dashboard. Un fondo casi opaco
// se ve igual de premium y no cuesta nada.
const CARD = `rounded-[20px] border border-line bg-surface/90 p-4 ${HOVER}`;

export function HomePage() {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const dash = useDashboard(today, "same_span");
  const day = useDayView(today);
  const hour = localHour(tz);
  const first = me?.user.display_name?.trim().split(/\s+/)[0];

  return (
    <>
      <Blueprint />
      <div className="relative z-10 space-y-8 md:pt-10">
        <div className="max-w-xl">
          <p className="font-serif text-[15px] text-muted first-letter:uppercase">{formatLongDate(today)}</p>
          <h1 className="mt-2 font-serif text-[34px] font-normal leading-[1.05] tracking-tight md:text-[56px]">
            {greeting(hour)}
            {first ? `, ${first}` : ""}
          </h1>
          {dash.data && day.data && <Brief d={dash.data} view={day.data} today={today} tz={tz} />}
          {day.data && <CheckinCta view={day.data} today={today} hour={hour} />}
        </div>

        {(dash.isPending || day.isPending) && <LoadingState rows={4} label="Preparando tu día…" />}
        {dash.isError && <ErrorState error={dash.error} onRetry={() => dash.refetch()} />}
        {dash.data && day.data && <Body d={dash.data} view={day.data} today={today} />}
      </div>
    </>
  );
}

function Brief({ d, view, today, tz }: { d: Dashboard; view: DayView; today: string; tz: string }) {
  const goal = d.month_goals.find((g) => g.goal.status === "active")?.goal.title;
  const habits = todaysHabits(view).filter((h) => asToday(h.progress).required_today).length;
  const tasks = view.tasks_by_area.flatMap((a) => a.tasks).filter((t) => t.status !== "done").length;
  const now = Date.now();
  const timed = view.agenda.filter((e) => !e.all_day && e.starts_at);
  const next = timed.find((e) => e.starts_at && new Date(e.starts_at).getTime() > now);
  const text = dayBrief({
    goalTitle: goal,
    dayOfMonth: Number(today.slice(8, 10)),
    habits,
    tasks,
    events: view.agenda.length,
    nextEvent: next?.starts_at
      ? {
          title: next.title,
          time: new Intl.DateTimeFormat("es-MX", { hour: "2-digit", minute: "2-digit", timeZone: tz }).format(
            new Date(next.starts_at),
          ),
        }
      : undefined,
  });
  return <p className="mt-4 text-[15px] leading-relaxed text-[#d6cdc5]">{text}</p>;
}

/** Botón principal: sugiere la parte del check-in según la hora y avisa si ya está hecha. */
function CheckinCta({ view, today, hour }: { view: DayView; today: string; hour: number }) {
  const morningDone = !!view.checkin?.morning_done_at;
  const eveningDone = !!view.checkin?.evening_done_at;
  const evening = hour >= 15 || morningDone;
  if (morningDone && eveningDone)
    return (
      <p className="mt-6 text-sm text-muted">
        Check-in de hoy completo ✓ <Link to={`/hoy?date=${today}`}>Ver</Link>
      </p>
    );
  return (
    <Link
      to={`/hoy?date=${today}&guiado=${evening ? "noche" : "manana"}`}
      className="mt-6 flex w-full items-center justify-between gap-4 rounded-[18px] bg-accent px-5 py-4 font-semibold text-accent-fg no-underline shadow-glow hover:brightness-105 md:inline-flex md:w-auto md:rounded-full md:px-6 md:py-3.5"
    >
      <span>{evening ? "Cerrar el día" : "Empezar el día"}</span>
      <span className="text-[13px] font-normal opacity-75">2 min →</span>
    </Link>
  );
}

function Body({ d, view, today }: { d: Dashboard; view: DayView; today: string }) {
  const habits = todaysHabits(view);
  const required = habits.filter((h) => asToday(h.progress).required_today);
  const doneReq = required.filter((h) => h.entry?.status === "done").length;
  const streak = recordStreak(d.last_30_days, today);
  return (
    <div className="space-y-8">
      {/* En el teléfono solo va el objetivo del mes; las otras tres lecturas son de escritorio. */}
      <div className="grid max-w-[860px] gap-3 md:grid-cols-4">
        <GoalGauge d={d} />
        <Panel
          title="Hábitos de hoy"
          sub={required.length ? `${doneReq} de ${required.length} exigibles` : "Nada exigible hoy"}
          desktop
        >
          <Ring done={doneReq} total={required.length} />
        </Panel>
        <Panel title="Racha" sub="Días seguidos con registro" desktop>
          <p className="mt-5 text-center text-[44px] font-light leading-none tabular-nums">{streak}</p>
          <p className="mt-1.5 text-center text-xs text-muted">{streak === 1 ? "día" : "días"}</p>
        </Panel>
        <WeightCard w={d.weight} />
      </div>
      <div className="max-w-[760px] md:order-none">
        <TodayGoals habits={habits} today={today} />
      </div>
      <Link to="/dashboard" className="inline-block text-sm text-muted md:hidden">
        Ver estadísticas completas →
      </Link>
      {/* Aire al final: la página siempre se puede desplazar y el fondo de planos se luce al bajar. */}
      <div aria-hidden className="hidden h-[45vh] md:block" />
    </div>
  );
}

function Panel({
  title,
  sub,
  desktop,
  children,
}: {
  title: string;
  sub?: string;
  desktop?: boolean;
  children: React.ReactNode;
}) {
  return (
    <section data-card className={`${CARD} ${desktop ? "hidden md:block" : ""}`}>
      <h2 className="text-[13px] font-medium">{title}</h2>
      {sub && <p className="mt-0.5 truncate text-[11px] text-muted">{sub}</p>}
      {children}
    </section>
  );
}

/** Medidor semicircular (como el control de clima de la referencia) con el avance del objetivo del mes. */
function GoalGauge({ d }: { d: Dashboard }) {
  const view = d.month_goals.find((g) => g.goal.status === "active") ?? d.month_goals[0];
  if (!view)
    return (
      <Panel title="Objetivo del mes" sub="Aún no defines uno">
        <p className="mt-6 text-sm text-muted">
          ¿Qué quieres lograr este mes? <Link to="/objetivos/planificar">Defínelo</Link> y aquí verás tu
          avance.
        </p>
      </Panel>
    );
  const p = asProgress(view.progress);
  const measured = view.goal.progress_type !== "none";
  const ratio = Math.min(1, Math.max(0, p.ratio ?? (p.achieved ? 1 : 0)));
  const angle = Math.PI * (1 - ratio);
  const x = 100 + 80 * Math.cos(angle);
  const y = 92 - 80 * Math.sin(angle);
  return (
    <Panel title="Objetivo del mes" sub={view.goal.title}>
      <Link
        to={`/objetivos/${view.goal.id}`}
        className="block no-underline"
        aria-label={`Ver ${view.goal.title}`}
      >
        <svg
          viewBox="0 0 200 104"
          className="mx-auto mt-2 block w-full max-w-[190px]"
          role="img"
          aria-label={measured ? `Avance ${Math.round(ratio * 100)}%` : "Sin medición"}
        >
          <path d="M20 92 A80 80 0 0 1 180 92" fill="none" stroke="var(--c-border)" strokeWidth="2" />
          {measured && ratio > 0 && (
            <>
              <path
                d={`M20 92 A80 80 0 0 1 ${x.toFixed(1)} ${y.toFixed(1)}`}
                fill="none"
                stroke="var(--c-accent)"
                strokeWidth="3"
                strokeLinecap="round"
              />
              <circle cx={x} cy={y} r="6" fill="var(--c-accent)" />
            </>
          )}
          <text x="100" y="80" textAnchor="middle" fill="var(--c-text)" fontSize="26" fontWeight="300">
            {measured ? `${Math.round(ratio * 100)}%` : "—"}
          </text>
        </svg>
        <p className="mt-1 truncate text-center text-[11px] text-muted">
          {measured ? describeProgress(view.goal, p) : "Objetivo sin medición"}
        </p>
      </Link>
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

/** Peso: último valor grande y las últimas mediciones como barras (en vez del gasto, que ya está en Finanzas). */
function WeightCard({ w }: { w: Dashboard["weight"] }) {
  const pts = w.points.slice(-8);
  // Siempre 8 ranuras: con pocas mediciones las vacías quedan tenues en vez de estirar una sola barra a todo lo ancho.
  const empty = Math.max(0, 8 - pts.length);
  const kgs = pts.map((p) => p.kg);
  const min = Math.min(...kgs);
  const max = Math.max(...kgs);
  const change = w.change_week;
  return (
    <Panel
      title="Peso"
      sub={
        change !== null && change !== undefined
          ? `${change > 0 ? "+" : ""}${formatNumber(change, 1)} kg esta semana`
          : w.goal_kg
            ? `Meta ${formatNumber(w.goal_kg, 1)} kg`
            : "Anótalo en tu check-in"
      }
      desktop
    >
      <p className="mt-3 text-[30px] font-light leading-none tabular-nums">
        {w.last ? formatNumber(w.last.kg, 1) : "—"}
        <span className="ml-1 text-sm text-muted">kg</span>
      </p>
      <div className="mt-3 flex h-10 items-end gap-1.5" aria-hidden>
        {Array.from({ length: empty }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: ranuras vacías sin identidad propia
          <div key={`e${i}`} className="h-[12%] flex-1 rounded-t bg-surface-2/60" />
        ))}
        {pts.map((p, i) => (
          <div
            key={p.date}
            className={`flex-1 rounded-t ${i === pts.length - 1 ? "bg-accent" : "bg-surface-2"}`}
            style={{ height: `${max > min ? 25 + ((p.kg - min) / (max - min)) * 75 : 60}%` }}
          />
        ))}
      </div>
    </Panel>
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
        <div className="rounded-[20px] border border-dashed border-line bg-surface/80 p-5 text-sm text-muted">
          Aún no tienes hábitos para hoy. <Link to="/objetivos/planificar">Planifica tus objetivos</Link> y
          aquí aparecerán con un interruptor para marcarlos.
        </div>
      ) : (
        <ul
          data-card
          className={`divide-y divide-line rounded-[20px] border border-line bg-surface/90 ${HOVER}`}
        >
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
