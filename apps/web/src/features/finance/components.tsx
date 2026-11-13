// Piezas compartidas del módulo de finanzas.
import { forwardRef, type InputHTMLAttributes, type ReactNode, useState } from "react";
import { NavLink } from "react-router";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { SelectField, TextField } from "../../components/ui/Field";
import { ApiError } from "../../lib/errors";
import type { AccountView, Category } from "./financeApi";
import { ACCOUNT_KIND_LABEL, monthLabel, shiftMonth } from "./labels";
import { formatCents } from "./money";

const TABS = [
  { to: "/finanzas", label: "Resumen", end: true },
  { to: "/finanzas/movimientos", label: "Movimientos" },
  { to: "/finanzas/cuentas", label: "Cuentas y tarjetas" },
  { to: "/finanzas/presupuesto", label: "Presupuesto" },
  { to: "/finanzas/recurrentes", label: "Recurrentes" },
  { to: "/finanzas/deudas", label: "Deudas" },
  { to: "/finanzas/plan", label: "Plan de crédito" },
  { to: "/finanzas/categorias", label: "Categorías" },
];

export function FinanceNav() {
  return (
    <nav aria-label="Secciones de finanzas" className="mb-4 -mx-1 overflow-x-auto">
      <ul className="flex w-max gap-1 px-1">
        {TABS.map((t) => (
          <li key={t.to}>
            <NavLink
              to={t.to}
              end={t.end}
              className={({ isActive }) =>
                `inline-flex min-h-10 items-center rounded-full border px-3 text-sm no-underline ${
                  isActive
                    ? "border-accent bg-accent text-accent-fg"
                    : "border-line bg-surface text-fg hover:bg-surface-2"
                }`
              }
            >
              {t.label}
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}

export function MonthPicker({ value, onChange }: { value: string; onChange: (m: string) => void }) {
  return (
    <div className="flex items-center gap-2">
      <Button
        variant="secondary"
        size="sm"
        aria-label="Mes anterior"
        onClick={() => onChange(shiftMonth(value, -1))}
      >
        ‹
      </Button>
      <label className="sr-only" htmlFor="finance-month">
        Mes
      </label>
      <input
        id="finance-month"
        type="month"
        value={value}
        onChange={(e) => e.target.value && onChange(e.target.value)}
        className="rounded-lg border border-line bg-surface px-3 py-1.5"
        aria-describedby="finance-month-label"
      />
      <span id="finance-month-label" className="sr-only">
        {monthLabel(value)}
      </span>
      <Button
        variant="secondary"
        size="sm"
        aria-label="Mes siguiente"
        onClick={() => onChange(shiftMonth(value, 1))}
      >
        ›
      </Button>
    </div>
  );
}

/** Monto formateado desde centavos; `signed` colorea positivo/negativo. */
export function Money({
  cents,
  currency = "MXN",
  signed,
  className = "",
}: {
  cents: number | null | undefined;
  currency?: string;
  signed?: boolean;
  className?: string;
}) {
  const tone = signed && cents != null ? (cents < 0 ? "text-danger" : cents > 0 ? "text-ok" : "") : "";
  return <span className={`tabular-nums ${tone} ${className}`}>{formatCents(cents, currency)}</span>;
}

/** Campo de monto: texto con teclado decimal; el valor se convierte a centavos con `parseMoney`. */
export const MoneyField = forwardRef<
  HTMLInputElement,
  {
    label: ReactNode;
    hint?: ReactNode;
    error?: string | undefined;
    className?: string;
  } & InputHTMLAttributes<HTMLInputElement>
>(function MoneyField(props, ref) {
  return <TextField ref={ref} inputMode="decimal" autoComplete="off" placeholder="0.00" {...props} />;
});

export function AccountSelect({
  label,
  value,
  onChange,
  accounts,
  filter,
  error,
  emptyLabel = "Sin cuenta",
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  accounts: AccountView[];
  filter?: (a: AccountView) => boolean;
  error?: string | undefined;
  emptyLabel?: string;
  hint?: ReactNode;
}) {
  const list = accounts.filter((a) => (filter ? filter(a) : true));
  return (
    <SelectField
      label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      error={error}
      hint={hint}
    >
      <option value="">{emptyLabel}</option>
      {list.map((a) => (
        <option key={a.account.id} value={a.account.id}>
          {a.account.name} · {ACCOUNT_KIND_LABEL[a.account.kind] ?? a.account.kind}
          {a.account.status !== "active" ? " (inactiva)" : ""}
        </option>
      ))}
    </SelectField>
  );
}

export function CategorySelect({
  label = "Categoría",
  value,
  onChange,
  categories,
  kind,
  error,
  emptyLabel = "Elige una categoría",
}: {
  label?: string;
  value: string;
  onChange: (v: string) => void;
  categories: Category[];
  kind: "expense" | "income";
  error?: string | undefined;
  emptyLabel?: string;
}) {
  const list = categories.filter((c) => c.kind === kind && (!c.archived_at || c.id === value));
  return (
    <SelectField label={label} value={value} onChange={(e) => onChange(e.target.value)} error={error}>
      <option value="">{emptyLabel}</option>
      {list.map((c) => (
        <option key={c.id} value={c.id}>
          {c.name}
        </option>
      ))}
    </SelectField>
  );
}

export function SimpleSelect({
  label,
  value,
  onChange,
  options,
  emptyLabel,
  error,
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { id: string; name: string }[];
  emptyLabel: string;
  error?: string | undefined;
  hint?: ReactNode;
}) {
  return (
    <SelectField
      label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      error={error}
      hint={hint}
    >
      <option value="">{emptyLabel}</option>
      {options.map((o) => (
        <option key={o.id} value={o.id}>
          {o.name}
        </option>
      ))}
    </SelectField>
  );
}

/**
 * Guardado con control de versión: si el servidor responde 409 muestra mi versión frente a la del servidor y
 * permite elegir. Nunca sobrescribe en silencio.
 */
export function useVersionedSave<B extends { version?: number | null }>(opts: {
  save: (body: B) => Promise<unknown>;
  labels?: Record<string, string>;
  /** Convierte el `current` del 409 a un cuerpo comparable (mismos campos que B). */
  toBody: (current: unknown) => B;
  onSaved?: () => void;
  onError?: (e: unknown) => void;
}) {
  const [conflict, setConflict] = useState<{ mine: B; theirs: B } | null>(null);
  const run = async (body: B): Promise<"saved" | "conflict"> => {
    try {
      await opts.save(body);
      opts.onSaved?.();
      return "saved";
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) {
        setConflict({ mine: body, theirs: opts.toBody(e.current) });
        return "conflict";
      }
      throw e;
    }
  };
  const dialog = conflict ? (
    <ConflictDialog
      open
      mine={conflict.mine as unknown as Record<string, unknown>}
      theirs={conflict.theirs as unknown as Record<string, unknown>}
      labels={opts.labels ?? {}}
      onClose={() => setConflict(null)}
      onResolve={async (r) => {
        const version = conflict.theirs.version;
        setConflict(null);
        if (r.choice === "theirs") {
          opts.onSaved?.();
          return;
        }
        const body = (r.choice === "mine" ? conflict.mine : r.body) as B;
        try {
          await run({ ...body, version });
        } catch (e) {
          opts.onError?.(e);
        }
      }}
    />
  ) : null;
  return { run, dialog };
}

export function Stat({ label, children, hint }: { label: string; children: ReactNode; hint?: ReactNode }) {
  return (
    <div className="flex min-w-0 justify-between gap-3 border-b border-line py-1.5 last:border-b-0">
      <dt className="text-muted">{label}</dt>
      <dd className="text-right font-medium tabular-nums">
        {children}
        {hint && <span className="block text-xs font-normal text-muted">{hint}</span>}
      </dd>
    </div>
  );
}

/** Mes actual "YYYY-MM" (local del navegador; el servidor usa la zona del usuario por defecto). */
export function currentMonth(now = new Date()): string {
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}
