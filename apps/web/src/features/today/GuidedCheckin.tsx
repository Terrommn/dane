// Check-in guiado: una pregunta por pantalla, con una línea que explica para qué sirve cada dato.
// Por qué existe además del formulario completo: el usuario no entendía qué debía llenar en la pantalla larga; en el
// teléfono (desde el mensaje del bot de la mañana o de la noche) se responde en 2 minutos sin pensar.
// Usa EXACTAMENTE el mismo estado y guardado que el formulario completo (borrador local, cola offline, conflictos), así
// que cambiar entre ambos nunca pierde datos.
import { useState } from "react";
import { Link } from "react-router";
import { Button } from "../../components/ui/Button";
import { TextArea, TextField } from "../../components/ui/Field";
import type { Schemas } from "../../lib/api";
import { formatMoney } from "../../lib/format";
import { HabitStatusControl } from "../habits/HabitStatusControl";
import { asToday } from "../habits/habitsApi";
import { type CheckinForm, ENERGY_LABEL, type HabitMark } from "./checkin";
import { ListEditor } from "./ListEditor";
import { parsePesosToCents } from "./numbers";

export type GuidedPart = "manana" | "noche";

type DayView = Schemas["DayView"];

interface Props {
  part: GuidedPart;
  date: string;
  view: DayView;
  form: CheckinForm;
  update: (p: Partial<CheckinForm>) => void;
  setHabit: (id: string, m: HabitMark) => void;
  /** Guarda (con extras como morning_done) y resuelve cuando terminó el intento. */
  save: (extra?: Partial<CheckinForm>) => Promise<void>;
  saving: boolean;
  finance: boolean;
  error: string | null;
  onDone: () => void;
  timeIn: (instant: string | null | undefined) => string;
}

interface Step {
  key: string;
  tag: string;
  question: string;
  why: string;
  body: React.ReactNode;
}

export function GuidedCheckin(p: Props) {
  const [i, setI] = useState(0);
  const steps = p.part === "manana" ? morningSteps(p) : eveningSteps(p);
  const step = steps[Math.min(i, steps.length - 1)];
  const last = i >= steps.length - 1;
  if (!step) return null;

  const finish = async () => {
    await p.save(p.part === "manana" ? { morning_done: true } : { evening_done: true });
    p.onDone();
  };

  return (
    <div className="mx-auto flex min-h-[calc(100dvh-12rem)] max-w-xl flex-col">
      <div className="flex items-center justify-between text-sm">
        <Link
          to={`/hoy?date=${p.date}`}
          className="text-muted no-underline"
          title="Ver el formulario completo"
        >
          Formulario completo
        </Link>
        <span className="font-medium">
          {p.part === "manana" ? "Check-in de la mañana" : "Cierre del día"}
        </span>
        <span className="tabular-nums text-muted">
          {i + 1}/{steps.length}
        </span>
      </div>
      <div className="mt-3 flex gap-1" aria-hidden>
        {steps.map((s, n) => (
          <span key={s.key} className={`h-1 flex-1 rounded-full ${n <= i ? "bg-accent" : "bg-surface-2"}`} />
        ))}
      </div>

      <section aria-labelledby={`q-${step.key}`} className="mt-6 flex-1">
        <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{step.tag}</p>
        <h1 id={`q-${step.key}`} className="mt-2 font-serif text-[28px] font-normal leading-tight">
          {step.question}
        </h1>
        <p className="mt-2 text-sm text-muted">{step.why}</p>
        <div className="mt-6">{step.body}</div>
      </section>

      {p.error && (
        <p role="alert" className="mt-4 rounded-xl border border-danger p-3 text-sm text-danger">
          {p.error}
        </p>
      )}

      {/* Botones fijos abajo en el teléfono: se alcanzan con el pulgar sin desplazarse. */}
      <div
        className="sticky bottom-0 mt-6 flex gap-3 bg-bg/95 py-4 backdrop-blur"
        style={{ paddingBottom: "max(1rem, var(--safe-bottom))" }}
      >
        <Button
          variant="secondary"
          className="flex-1"
          disabled={i === 0}
          onClick={() => setI((n) => Math.max(0, n - 1))}
        >
          Atrás
        </Button>
        {last ? (
          <Button className="flex-[2]" loading={p.saving} onClick={() => void finish()}>
            {p.part === "manana" ? "Guardar y empezar el día" : "Cerrar el día"}
          </Button>
        ) : (
          <Button className="flex-[2]" onClick={() => setI((n) => n + 1)}>
            Siguiente
          </Button>
        )}
      </div>
    </div>
  );
}

function EnergyScale({ form, update }: Pick<Props, "form" | "update">) {
  return (
    <fieldset>
      <legend className="sr-only">Energía del 1 al 5</legend>
      <div className="grid grid-cols-5 gap-2">
        {[1, 2, 3, 4, 5].map((n) => (
          <label key={n}>
            <input
              type="radio"
              name="guided-energy"
              className="peer sr-only"
              checked={form.energy === n}
              onChange={() => update({ energy: n })}
            />
            <span className="flex aspect-square cursor-pointer items-center justify-center rounded-2xl border border-line bg-surface text-xl peer-checked:border-accent peer-checked:bg-accent-soft peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent">
              {n}
            </span>
          </label>
        ))}
      </div>
      <div className="mt-2 flex justify-between text-xs text-muted">
        <span>Agotado</span>
        <span>{form.energy ? ENERGY_LABEL[form.energy] : "Sin responder"}</span>
        <span>A tope</span>
      </div>
    </fieldset>
  );
}

function morningSteps(p: Props): Step[] {
  const { form, update, view } = p;
  const cents = parsePesosToCents(form.planned_spend_amount);
  const openTasks = view.tasks_by_area.flatMap((a) => a.tasks).filter((t) => t.status !== "done");
  const habits = [
    ...view.habits.fixed,
    ...view.habits.month,
    ...view.habits.week,
    ...view.habits.other,
  ].filter((h) => asToday(h.progress).required_today);
  const steps: Step[] = [
    {
      key: "energia",
      tag: "Cómo estás",
      question: "¿Con cuánta energía despertaste?",
      why: "Del 1 (agotado) al 5 (a tope). Con el tiempo verás qué días y hábitos te dan más energía.",
      body: <EnergyScale form={form} update={update} />,
    },
    {
      key: "peso",
      tag: "Tu cuerpo",
      question: "¿Cuánto pesas hoy?",
      why: "Opcional. Si no te pesaste, déjalo vacío y sigue: vacío no cuenta como cero.",
      body: (
        <TextField
          label="Peso en kg"
          inputMode="decimal"
          placeholder="Ej. 78.4"
          value={form.weight_kg}
          onChange={(e) => update({ weight_kg: e.target.value })}
        />
      ),
    },
  ];
  if (p.finance)
    steps.push({
      key: "gasto",
      tag: "Tu dinero",
      question: "¿Planeas gastar algo hoy?",
      why: "Anotarlo antes ayuda a no gastar de más. Dane te sugiere con qué tarjeta pagar según tus cortes.",
      body: (
        <div className="space-y-3">
          <label className="flex min-h-12 cursor-pointer items-center gap-3 rounded-2xl border border-line bg-surface px-4 text-sm">
            <input
              type="checkbox"
              className="size-5 accent-[var(--c-accent)]"
              checked={form.planned_spend_none}
              onChange={(e) => update({ planned_spend_none: e.target.checked })}
            />
            No planeo gastar nada hoy
          </label>
          {!form.planned_spend_none && (
            <>
              <TextField
                label="¿En qué?"
                placeholder="Ej. comida con Ana"
                value={form.planned_spend_text}
                onChange={(e) => update({ planned_spend_text: e.target.value })}
              />
              <TextField
                label="¿Cuánto? (opcional)"
                inputMode="decimal"
                placeholder="Ej. 350"
                value={form.planned_spend_amount}
                onChange={(e) => update({ planned_spend_amount: e.target.value })}
                error={cents.ok ? undefined : cents.error}
              />
            </>
          )}
        </div>
      ),
    });
  steps.push({
    key: "dia",
    tag: "Tu día",
    question: "Esto es lo que te espera hoy",
    why: "Revísalo y guarda: así queda marcada la mañana y el bot no te lo vuelve a recordar.",
    body: (
      <div className="space-y-3 text-sm">
        <Summary
          title="Agenda"
          empty="Sin eventos hoy."
          items={view.agenda.map((e) => `${e.all_day ? "Todo el día" : p.timeIn(e.starts_at)} · ${e.title}`)}
        />
        <Summary
          title="Hábitos que tocan hoy"
          empty="Ninguno exigible hoy."
          items={habits.map((h) => h.habit.name)}
        />
        <Summary
          title="Tareas pendientes"
          empty="Sin tareas para hoy."
          items={openTasks.slice(0, 6).map((t) => t.title)}
          more={openTasks.length > 6 ? openTasks.length - 6 : 0}
        />
      </div>
    ),
  });
  return steps;
}

function eveningSteps(p: Props): Step[] {
  const { form, update, view } = p;
  const habits = [
    ...view.habits.fixed,
    ...view.habits.month,
    ...view.habits.week,
    ...view.habits.other,
  ].filter((h) => asToday(h.progress).shown !== false);
  const spent = view.derived.expenses_count
    ? ` Hoy registraste ${formatMoney(view.derived.expenses_total_cents, view.currency)} en gastos.`
    : "";
  return [
    {
      key: "habitos",
      tag: "Hábitos",
      question: "¿Qué hábitos cumpliste hoy?",
      why: 'Marca ✓ si lo hiciste, ✕ si no, y – si hoy no aplicaba (te pedirá el motivo). Lo que dejes sin marcar cuenta como "sin registrar".',
      body:
        habits.length === 0 ? (
          <p className="rounded-2xl border border-dashed border-line p-4 text-sm text-muted">
            No tienes hábitos para hoy. Puedes crearlos en <Link to="/objetivos/planificar">Planificar</Link>.
          </p>
        ) : (
          <div className="space-y-2">
            {habits.map((h) => (
              <HabitStatusControl
                key={h.habit.id}
                name={h.habit.name}
                detail={h.goal_title ?? undefined}
                unit={h.habit.unit}
                mark={form.habits[h.habit.id] ?? { status: null, reason: "", quantity: "" }}
                onChange={(m) => p.setHabit(h.habit.id, m)}
              />
            ))}
          </div>
        ),
    },
    {
      key: "log",
      tag: "Tu día",
      question: "¿Qué pasó hoy?",
      why: `Una línea por cosa, en pasado. Lo automático (tareas cerradas, eventos, gastos) ya se registra solo.${spent}`,
      body: (
        <ListEditor
          label="Lo que pasó"
          items={form.log}
          onChange={(log) => update({ log })}
          placeholder="Ej. Terminé el reporte del cliente"
        />
      ),
    },
    {
      key: "reflexion",
      tag: "Reflexión",
      question: "¿Qué hiciste bien y qué harías distinto?",
      why: "Pensando en tus objetivos del mes y de la semana. Basta con una cosa de cada lado.",
      body: (
        <div className="space-y-5">
          <ListEditor
            label="Lo que hice bien"
            items={form.went_well}
            onChange={(went_well) => update({ went_well })}
          />
          <ListEditor
            label="Lo que haría distinto"
            items={form.went_badly}
            onChange={(went_badly) => update({ went_badly })}
          />
        </div>
      ),
    },
    {
      key: "cierre",
      tag: "Cerrar",
      question: form.energy ? "¿Algo más antes de cerrar?" : "¿Cómo terminó tu energía hoy?",
      why: form.energy
        ? "Notas libres, opcionales. Al cerrar, el bot no te vuelve a escribir hoy."
        : "No la anotaste en la mañana. Del 1 al 5; luego, notas opcionales.",
      body: (
        <div className="space-y-5">
          {!form.energy && <EnergyScale form={form} update={update} />}
          <TextArea
            label="Notas (opcional)"
            rows={4}
            value={form.notes_md}
            onChange={(e) => update({ notes_md: e.target.value })}
          />
        </div>
      ),
    },
  ];
}

function Summary({
  title,
  items,
  empty,
  more = 0,
}: {
  title: string;
  items: string[];
  empty: string;
  more?: number;
}) {
  return (
    <div className="rounded-2xl border border-line bg-surface p-4">
      <p className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">{title}</p>
      {items.length === 0 ? (
        <p className="mt-1.5 text-muted">{empty}</p>
      ) : (
        <ul className="mt-1.5 space-y-1">
          {items.map((t) => (
            <li key={t} className="break-words">
              {t}
            </li>
          ))}
          {more > 0 && <li className="text-muted">y {more} más</li>}
        </ul>
      )}
    </div>
  );
}
