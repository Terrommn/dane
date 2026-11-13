// Editor de la propuesta tipada. Es también el formulario completo del modo manual (sin IA): muestra solo las
// secciones que el intent permite y nunca rellena valores por su cuenta.
import { Fragment, type ReactNode, useId } from "react";
import { Button } from "../../components/ui/Button";
import { CheckboxField, SelectField, TextArea, TextField } from "../../components/ui/Field";
import type {
  CaptureQuestion,
  CheckinItem,
  DevlogItem,
  ExpenseItem,
  GoalItem,
  IncomeItem,
  InteractionItem,
  Proposal,
  ProposalField,
  TaskItem,
} from "./proposal";
import { isValidAmount, parseQuestionField } from "./proposal";

export interface Catalogs {
  expenseCategories: string[];
  incomeCategories: string[];
  accounts: string[];
  areas: string[];
  people: string[];
}

interface EditorProps {
  value: Proposal;
  onChange: (p: Proposal) => void;
  allowed: readonly string[];
  catalogs: Catalogs;
  questions: CaptureQuestion[];
  readOnly?: boolean;
}

function Section({ title, children, actions }: { title: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <fieldset className="min-w-0 rounded-xl border border-line p-3">
      <legend className="px-1 text-base font-semibold">{title}</legend>
      <div className="space-y-3">{children}</div>
      {actions && <div className="mt-3 flex flex-wrap gap-2">{actions}</div>}
    </fieldset>
  );
}

function Row({
  children,
  onRemove,
  label,
  readOnly,
}: {
  children: ReactNode;
  onRemove: () => void;
  label: string;
  readOnly?: boolean | undefined;
}) {
  return (
    <div className="rounded-lg border border-line bg-surface-2/40 p-3">
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">{children}</div>
      {!readOnly && (
        <div className="mt-2 flex justify-end">
          <Button variant="ghost" size="sm" onClick={onRemove} aria-label={`Quitar ${label}`}>
            Quitar
          </Button>
        </div>
      )}
    </div>
  );
}

/** Mensaje de pregunta del servidor para un campo concreto (`expenses[0].amount`). */
function questionFor(
  questions: CaptureQuestion[],
  section: string,
  index: number,
  field: string,
): string | undefined {
  return questions.find((q) => {
    const f = parseQuestionField(q.field);
    return f.section === section && f.index === index && f.field === field;
  })?.message;
}

const nn = (s: string): string | null => (s === "" ? null : s);

function DataList({ id, items }: { id: string; items: string[] }) {
  return (
    <datalist id={id}>
      {items.map((x) => (
        <option key={x} value={x} />
      ))}
    </datalist>
  );
}

function StringList({
  title,
  items,
  onChange,
  addLabel,
  placeholder,
  readOnly,
}: {
  title: string;
  items: string[];
  onChange: (xs: string[]) => void;
  addLabel: string;
  placeholder?: string;
  readOnly?: boolean | undefined;
}) {
  return (
    <Section
      title={title}
      actions={
        readOnly ? undefined : (
          <Button variant="secondary" size="sm" onClick={() => onChange([...items, ""])}>
            {addLabel}
          </Button>
        )
      }
    >
      {items.length === 0 && <p className="text-sm text-muted">Nada por ahora.</p>}
      {items.map((x, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <div key={i} className="flex items-end gap-2">
          <TextField
            className="min-w-0 flex-1"
            label={`${title} ${i + 1}`}
            value={x}
            placeholder={placeholder}
            readOnly={readOnly}
            onChange={(e) => onChange(items.map((y, j) => (j === i ? e.target.value : y)))}
          />
          {!readOnly && (
            <Button
              variant="ghost"
              size="sm"
              aria-label={`Quitar ${title.toLowerCase()} ${i + 1}`}
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            >
              ✕
            </Button>
          )}
        </div>
      ))}
    </Section>
  );
}

function ExpensesEditor({
  items,
  onChange,
  cats,
  accounts,
  questions,
  readOnly,
}: {
  items: ExpenseItem[];
  onChange: (x: ExpenseItem[]) => void;
  cats: string[];
  accounts: string[];
  questions: CaptureQuestion[];
  readOnly?: boolean | undefined;
}) {
  const listId = useId();
  const set = (i: number, patch: Partial<ExpenseItem>) =>
    onChange(items.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <Section
      title="Gastos"
      actions={
        readOnly ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              onChange([...items, { date: null, concept: "", amount: null, category: null, payment: null }])
            }
          >
            Agregar gasto
          </Button>
        )
      }
    >
      <DataList id={listId} items={accounts} />
      {items.length === 0 && <p className="text-sm text-muted">Sin gastos.</p>}
      {items.map((e, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <Fragment key={i}>
          <Row
            label={`gasto ${i + 1}`}
            onRemove={() => onChange(items.filter((_, j) => j !== i))}
            readOnly={readOnly}
          >
            <TextField
              label="Concepto"
              value={e.concept}
              readOnly={readOnly}
              onChange={(ev) => set(i, { concept: ev.target.value })}
            />
            <TextField
              label="Monto"
              inputMode="decimal"
              placeholder="180.50"
              value={e.amount ?? ""}
              readOnly={readOnly}
              error={
                questionFor(questions, "expenses", i, "amount") ??
                (e.amount && !isValidAmount(e.amount) ? "Monto inválido (ej. 180.50)" : undefined)
              }
              onChange={(ev) => set(i, { amount: nn(ev.target.value) })}
            />
            <TextField
              label="Fecha del gasto"
              type="date"
              value={e.date ?? ""}
              readOnly={readOnly}
              error={questionFor(questions, "expenses", i, "date")}
              onChange={(ev) => set(i, { date: nn(ev.target.value) })}
            />
            <SelectField
              label="Categoría"
              value={e.category ?? ""}
              disabled={readOnly}
              error={questionFor(questions, "expenses", i, "category")}
              onChange={(ev) => set(i, { category: nn(ev.target.value) })}
            >
              <option value="">Sin elegir</option>
              {e.category && !cats.includes(e.category) && <option value={e.category}>{e.category}</option>}
              {cats.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Pago (cuenta o tarjeta)"
              list={listId}
              value={e.payment ?? ""}
              readOnly={readOnly}
              hint="Opcional. Escribe el nombre de la cuenta, tarjeta o «Efectivo»."
              onChange={(ev) => set(i, { payment: nn(ev.target.value) })}
            />
          </Row>
        </Fragment>
      ))}
    </Section>
  );
}

function IncomesEditor({
  items,
  onChange,
  cats,
  accounts,
  questions,
  readOnly,
}: {
  items: IncomeItem[];
  onChange: (x: IncomeItem[]) => void;
  cats: string[];
  accounts: string[];
  questions: CaptureQuestion[];
  readOnly?: boolean | undefined;
}) {
  const listId = useId();
  const set = (i: number, patch: Partial<IncomeItem>) =>
    onChange(items.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <Section
      title="Ingresos"
      actions={
        readOnly ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              onChange([...items, { date: null, concept: "", amount: null, category: null, account: null }])
            }
          >
            Agregar ingreso
          </Button>
        )
      }
    >
      <DataList id={listId} items={accounts} />
      {items.length === 0 && <p className="text-sm text-muted">Sin ingresos.</p>}
      {items.map((e, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <Fragment key={i}>
          <Row
            label={`ingreso ${i + 1}`}
            onRemove={() => onChange(items.filter((_, j) => j !== i))}
            readOnly={readOnly}
          >
            <TextField
              label="Concepto"
              value={e.concept}
              readOnly={readOnly}
              onChange={(ev) => set(i, { concept: ev.target.value })}
            />
            <TextField
              label="Monto"
              inputMode="decimal"
              value={e.amount ?? ""}
              readOnly={readOnly}
              error={
                questionFor(questions, "incomes", i, "amount") ??
                (e.amount && !isValidAmount(e.amount) ? "Monto inválido (ej. 15000)" : undefined)
              }
              onChange={(ev) => set(i, { amount: nn(ev.target.value) })}
            />
            <TextField
              label="Fecha en que entró"
              type="date"
              value={e.date ?? ""}
              readOnly={readOnly}
              hint="Obligatoria: no se supone que fue hoy."
              error={questionFor(questions, "incomes", i, "date")}
              onChange={(ev) => set(i, { date: nn(ev.target.value) })}
            />
            <SelectField
              label="Categoría"
              value={e.category ?? ""}
              disabled={readOnly}
              error={questionFor(questions, "incomes", i, "category")}
              onChange={(ev) => set(i, { category: nn(ev.target.value) })}
            >
              <option value="">Sin elegir</option>
              {e.category && !cats.includes(e.category) && <option value={e.category}>{e.category}</option>}
              {cats.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Cuenta receptora"
              list={listId}
              value={e.account ?? ""}
              readOnly={readOnly}
              onChange={(ev) => set(i, { account: nn(ev.target.value) })}
            />
          </Row>
        </Fragment>
      ))}
    </Section>
  );
}

function TasksEditor({
  items,
  onChange,
  areas,
  readOnly,
}: {
  items: TaskItem[];
  onChange: (x: TaskItem[]) => void;
  areas: string[];
  readOnly?: boolean | undefined;
}) {
  const set = (i: number, patch: Partial<TaskItem>) =>
    onChange(items.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <Section
      title="Pendientes"
      actions={
        readOnly ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              onChange([...items, { title: "", area: null, due: null, people: [], clickup_candidate: false }])
            }
          >
            Agregar pendiente
          </Button>
        )
      }
    >
      {items.length === 0 && <p className="text-sm text-muted">Sin pendientes.</p>}
      {items.map((t, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <Fragment key={i}>
          <Row
            label={`pendiente ${i + 1}`}
            onRemove={() => onChange(items.filter((_, j) => j !== i))}
            readOnly={readOnly}
          >
            <TextField
              label="Pendiente"
              value={t.title}
              readOnly={readOnly}
              onChange={(ev) => set(i, { title: ev.target.value })}
            />
            <SelectField
              label="Área"
              value={t.area ?? ""}
              disabled={readOnly}
              onChange={(ev) => set(i, { area: nn(ev.target.value) })}
            >
              <option value="">Sin área</option>
              {t.area && !areas.includes(t.area) && <option value={t.area}>{t.area}</option>}
              {areas.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Fecha límite"
              type="date"
              value={t.due ?? ""}
              readOnly={readOnly}
              onChange={(ev) => set(i, { due: nn(ev.target.value) })}
            />
            <TextField
              label="Personas"
              hint="Separadas por coma"
              value={t.people.join(", ")}
              readOnly={readOnly}
              onChange={(ev) => set(i, { people: ev.target.value.split(",").map((x) => x.trimStart()) })}
            />
            <CheckboxField
              className="sm:col-span-2"
              label="Parece trabajo de equipo (#clickup)"
              hint="Solo es una marca para moverlo después; la app no crea nada en ClickUp."
              checked={t.clickup_candidate}
              disabled={readOnly}
              onChange={(ev) => set(i, { clickup_candidate: ev.target.checked })}
            />
          </Row>
        </Fragment>
      ))}
    </Section>
  );
}

function InteractionsEditor({
  items,
  onChange,
  people,
  readOnly,
}: {
  items: InteractionItem[];
  onChange: (x: InteractionItem[]) => void;
  people: string[];
  readOnly?: boolean | undefined;
}) {
  const listId = useId();
  const set = (i: number, patch: Partial<InteractionItem>) =>
    onChange(items.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <Section
      title="Personas con las que hablé"
      actions={
        readOnly ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => onChange([...items, { person: "", topic: "" }])}
          >
            Agregar persona
          </Button>
        )
      }
    >
      <DataList id={listId} items={people} />
      <p className="text-sm text-muted">
        Si la persona no existe todavía, queda como sugerencia: no se crea sin tu decisión.
      </p>
      {items.map((x, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <Fragment key={i}>
          <Row
            label={`persona ${i + 1}`}
            onRemove={() => onChange(items.filter((_, j) => j !== i))}
            readOnly={readOnly}
          >
            <TextField
              label="Persona (nombre o alias)"
              list={listId}
              value={x.person}
              readOnly={readOnly}
              onChange={(ev) => set(i, { person: ev.target.value })}
            />
            <TextField
              label="Tema"
              value={x.topic}
              readOnly={readOnly}
              onChange={(ev) => set(i, { topic: ev.target.value })}
            />
          </Row>
        </Fragment>
      ))}
    </Section>
  );
}

const TRI: { value: string; label: string }[] = [
  { value: "", label: "Sin dato" },
  { value: "yes", label: "Sí" },
  { value: "no", label: "No" },
];
const triValue = (b: boolean | null) => (b === null ? "" : b ? "yes" : "no");
const fromTri = (v: string): boolean | null => (v === "" ? null : v === "yes");

function CheckinEditor({
  value,
  onChange,
  readOnly,
}: {
  value: CheckinItem | null;
  onChange: (c: CheckinItem | null) => void;
  readOnly?: boolean | undefined;
}) {
  const c = value ?? { weight_kg: null, energy: null, exercise: null, reading: null, planned_spend: null };
  const set = (patch: Partial<CheckinItem>) => onChange({ ...c, ...patch });
  return (
    <Section title="Check-in del día">
      <p className="text-sm text-muted">Solo lo que dictaste. «Sin dato» no cuenta como cero ni como «no».</p>
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <TextField
          label="Peso (kg)"
          inputMode="decimal"
          value={c.weight_kg ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ weight_kg: nn(e.target.value) })}
        />
        <SelectField
          label="Energía (1–5)"
          value={c.energy === null ? "" : String(c.energy)}
          disabled={readOnly}
          onChange={(e) => set({ energy: e.target.value === "" ? null : Number(e.target.value) })}
        >
          <option value="">Sin dato</option>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </SelectField>
        <SelectField
          label="Ejercicio"
          value={triValue(c.exercise)}
          disabled={readOnly}
          onChange={(e) => set({ exercise: fromTri(e.target.value) })}
        >
          {TRI.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </SelectField>
        <SelectField
          label="Lectura"
          value={triValue(c.reading)}
          disabled={readOnly}
          onChange={(e) => set({ reading: fromTri(e.target.value) })}
        >
          {TRI.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </SelectField>
        <TextField
          className="sm:col-span-2"
          label="Planeo gastar"
          value={c.planned_spend ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ planned_spend: nn(e.target.value) })}
        />
      </div>
    </Section>
  );
}

function GoalsEditor({
  items,
  onChange,
  areas,
  readOnly,
}: {
  items: GoalItem[];
  onChange: (x: GoalItem[]) => void;
  areas: string[];
  readOnly?: boolean | undefined;
}) {
  const set = (i: number, patch: Partial<GoalItem>) =>
    onChange(items.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <Section
      title="Objetivos"
      actions={
        readOnly ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() =>
              onChange([
                ...items,
                { title: "", kind: "week", area: null, motivation: null, success_criteria: null, habits: [] },
              ])
            }
          >
            Agregar objetivo
          </Button>
        )
      }
    >
      <p className="text-sm text-muted">
        Los hábitos son acciones diarias marcables; ninguno se crea sin tu confirmación.
      </p>
      {items.map((g, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: filas editables sin ID propio
        <Fragment key={i}>
          <Row
            label={`objetivo ${i + 1}`}
            onRemove={() => onChange(items.filter((_, j) => j !== i))}
            readOnly={readOnly}
          >
            <TextField
              label="Objetivo"
              value={g.title}
              readOnly={readOnly}
              onChange={(ev) => set(i, { title: ev.target.value })}
            />
            <SelectField
              label="Periodo"
              value={g.kind}
              disabled={readOnly}
              onChange={(ev) => set(i, { kind: ev.target.value === "month" ? "month" : "week" })}
            >
              <option value="week">Semana</option>
              <option value="month">Mes</option>
            </SelectField>
            <SelectField
              label="Área"
              value={g.area ?? ""}
              disabled={readOnly}
              onChange={(ev) => set(i, { area: nn(ev.target.value) })}
            >
              <option value="">Sin área</option>
              {g.area && !areas.includes(g.area) && <option value={g.area}>{g.area}</option>}
              {areas.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Por qué importa"
              value={g.motivation ?? ""}
              readOnly={readOnly}
              onChange={(ev) => set(i, { motivation: nn(ev.target.value) })}
            />
            <TextField
              className="sm:col-span-2"
              label="Cómo sé que lo logré"
              value={g.success_criteria ?? ""}
              readOnly={readOnly}
              onChange={(ev) => set(i, { success_criteria: nn(ev.target.value) })}
            />
            <TextArea
              className="sm:col-span-2"
              label="Hábitos diarios (uno por línea, 1–3)"
              rows={3}
              value={g.habits.join("\n")}
              readOnly={readOnly}
              onChange={(ev) => set(i, { habits: ev.target.value.split("\n") })}
            />
          </Row>
        </Fragment>
      ))}
    </Section>
  );
}

function DevlogEditor({
  value,
  onChange,
  readOnly,
}: {
  value: DevlogItem | null;
  onChange: (d: DevlogItem | null) => void;
  readOnly?: boolean | undefined;
}) {
  const d = value ?? { repo: "", branch: null, changes: "", bugs: null, decisions: null, next_steps: null };
  const set = (patch: Partial<DevlogItem>) => onChange({ ...d, ...patch });
  return (
    <Section title="Sesión de código">
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <TextField
          label="Repositorio"
          value={d.repo}
          readOnly={readOnly}
          onChange={(e) => set({ repo: e.target.value })}
        />
        <TextField
          label="Rama"
          value={d.branch ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ branch: nn(e.target.value) })}
        />
        <TextArea
          className="sm:col-span-2"
          label="Qué hice"
          rows={3}
          value={d.changes}
          readOnly={readOnly}
          onChange={(e) => set({ changes: e.target.value })}
        />
        <TextArea
          label="Bloqueos / bugs"
          rows={2}
          value={d.bugs ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ bugs: nn(e.target.value) })}
        />
        <TextArea
          label="Decisiones"
          rows={2}
          value={d.decisions ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ decisions: nn(e.target.value) })}
        />
        <TextArea
          className="sm:col-span-2"
          label="Siguiente"
          rows={2}
          value={d.next_steps ?? ""}
          readOnly={readOnly}
          onChange={(e) => set({ next_steps: nn(e.target.value) })}
        />
      </div>
    </Section>
  );
}

export function ProposalEditor({ value, onChange, allowed, catalogs, questions, readOnly }: EditorProps) {
  const has = (f: ProposalField) => allowed.includes(f);
  const patch = (p: Partial<Proposal>) => onChange({ ...value, ...p });
  return (
    <div className="space-y-4">
      {has("checkin") && (
        <CheckinEditor value={value.checkin} onChange={(checkin) => patch({ checkin })} readOnly={readOnly} />
      )}
      {has("expenses") && (
        <ExpensesEditor
          items={value.expenses}
          onChange={(expenses) => patch({ expenses })}
          cats={catalogs.expenseCategories}
          accounts={catalogs.accounts}
          questions={questions}
          readOnly={readOnly}
        />
      )}
      {has("incomes") && (
        <IncomesEditor
          items={value.incomes}
          onChange={(incomes) => patch({ incomes })}
          cats={catalogs.incomeCategories}
          accounts={catalogs.accounts}
          questions={questions}
          readOnly={readOnly}
        />
      )}
      {has("tasks") && (
        <TasksEditor
          items={value.tasks}
          onChange={(tasks) => patch({ tasks })}
          areas={catalogs.areas}
          readOnly={readOnly}
        />
      )}
      {has("inbox") && (
        <StringList
          title="Inbox"
          addLabel="Agregar al Inbox"
          items={value.inbox}
          onChange={(inbox) => patch({ inbox })}
          readOnly={readOnly}
        />
      )}
      {has("log") && (
        <StringList
          title="Log"
          addLabel="Agregar línea"
          items={value.log}
          onChange={(log) => patch({ log })}
          readOnly={readOnly}
        />
      )}
      {has("reflections_good") && (
        <StringList
          title="Lo que hice bien hoy"
          addLabel="Agregar"
          items={value.reflections_good}
          onChange={(reflections_good) => patch({ reflections_good })}
          readOnly={readOnly}
        />
      )}
      {has("reflections_bad") && (
        <StringList
          title="Lo que hice mal hoy"
          addLabel="Agregar"
          items={value.reflections_bad}
          onChange={(reflections_bad) => patch({ reflections_bad })}
          readOnly={readOnly}
        />
      )}
      {has("interactions") && (
        <InteractionsEditor
          items={value.interactions}
          onChange={(interactions) => patch({ interactions })}
          people={catalogs.people}
          readOnly={readOnly}
        />
      )}
      {has("goals") && (
        <GoalsEditor
          items={value.goals}
          onChange={(goals) => patch({ goals })}
          areas={catalogs.areas}
          readOnly={readOnly}
        />
      )}
      {has("devlog") && (
        <DevlogEditor value={value.devlog} onChange={(devlog) => patch({ devlog })} readOnly={readOnly} />
      )}
      {has("summary") && (
        <Section title="Resumen de la semana">
          <TextArea
            label="Resumen (editable; solo se guarda al confirmar)"
            rows={6}
            value={value.summary ?? ""}
            readOnly={readOnly}
            onChange={(e) => patch({ summary: nn(e.target.value) })}
          />
        </Section>
      )}
    </div>
  );
}
