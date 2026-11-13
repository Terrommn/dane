// Campos de formulario con label visible, ayuda y error asociados por aria-describedby.
import {
  forwardRef,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
  useId,
} from "react";

const control =
  "w-full min-w-0 rounded-lg border border-line bg-surface px-3 py-2 text-base text-fg placeholder:text-muted aria-[invalid=true]:border-danger";

interface FieldBase {
  label: ReactNode;
  hint?: ReactNode;
  error?: string | undefined;
  className?: string;
}

function useDescribedBy(hint: unknown, error: unknown) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errId = error ? `${id}-err` : undefined;
  return { id, hintId, errId, describedBy: [hintId, errId].filter(Boolean).join(" ") || undefined };
}

function Meta({
  hintId,
  errId,
  hint,
  error,
}: {
  hintId?: string | undefined;
  errId?: string | undefined;
  hint?: ReactNode;
  error?: string | undefined;
}) {
  return (
    <>
      {hint && (
        <p id={hintId} className="mt-1 text-sm text-muted">
          {hint}
        </p>
      )}
      {error && (
        <p id={errId} role="alert" className="mt-1 text-sm font-medium text-danger">
          {error}
        </p>
      )}
    </>
  );
}

export const TextField = forwardRef<HTMLInputElement, FieldBase & InputHTMLAttributes<HTMLInputElement>>(
  function TextField({ label, hint, error, className = "", ...rest }, ref) {
    const d = useDescribedBy(hint, error);
    return (
      <div className={className}>
        <label htmlFor={rest.id ?? d.id} className="mb-1 block text-sm font-medium">
          {label}
        </label>
        <input
          ref={ref}
          id={rest.id ?? d.id}
          className={control}
          aria-invalid={error ? true : undefined}
          aria-describedby={d.describedBy}
          {...rest}
        />
        <Meta hintId={d.hintId} errId={d.errId} hint={hint} error={error} />
      </div>
    );
  },
);

export const TextArea = forwardRef<
  HTMLTextAreaElement,
  FieldBase & TextareaHTMLAttributes<HTMLTextAreaElement>
>(function TextArea({ label, hint, error, className = "", rows = 4, ...rest }, ref) {
  const d = useDescribedBy(hint, error);
  return (
    <div className={className}>
      <label htmlFor={rest.id ?? d.id} className="mb-1 block text-sm font-medium">
        {label}
      </label>
      <textarea
        ref={ref}
        rows={rows}
        id={rest.id ?? d.id}
        className={control}
        aria-invalid={error ? true : undefined}
        aria-describedby={d.describedBy}
        {...rest}
      />
      <Meta hintId={d.hintId} errId={d.errId} hint={hint} error={error} />
    </div>
  );
});

export const SelectField = forwardRef<
  HTMLSelectElement,
  FieldBase & SelectHTMLAttributes<HTMLSelectElement> & { children: ReactNode }
>(function SelectField({ label, hint, error, className = "", children, ...rest }, ref) {
  const d = useDescribedBy(hint, error);
  return (
    <div className={className}>
      <label htmlFor={rest.id ?? d.id} className="mb-1 block text-sm font-medium">
        {label}
      </label>
      <select
        ref={ref}
        id={rest.id ?? d.id}
        className={control}
        aria-invalid={error ? true : undefined}
        aria-describedby={d.describedBy}
        {...rest}
      >
        {children}
      </select>
      <Meta hintId={d.hintId} errId={d.errId} hint={hint} error={error} />
    </div>
  );
});

export const CheckboxField = forwardRef<
  HTMLInputElement,
  Omit<FieldBase, "error"> & { error?: string } & InputHTMLAttributes<HTMLInputElement>
>(function CheckboxField({ label, hint, error, className = "", ...rest }, ref) {
  const d = useDescribedBy(hint, error);
  return (
    <div className={`flex items-start gap-3 ${className}`}>
      <input
        ref={ref}
        type="checkbox"
        id={rest.id ?? d.id}
        className="mt-1 h-5 w-5 shrink-0 accent-[var(--c-accent)]"
        aria-describedby={d.describedBy}
        {...rest}
      />
      <div className="min-w-0">
        <label htmlFor={rest.id ?? d.id} className="font-medium">
          {label}
        </label>
        <Meta hintId={d.hintId} errId={d.errId} hint={hint} error={error} />
      </div>
    </div>
  );
});

/** Grupo de opciones (radio) con leyenda. */
export function RadioGroup<T extends string>({
  legend,
  name,
  value,
  options,
  onChange,
}: {
  legend: ReactNode;
  name: string;
  value: T;
  options: { value: T; label: ReactNode; hint?: ReactNode }[];
  onChange: (v: T) => void;
}) {
  return (
    <fieldset>
      <legend className="mb-2 text-sm font-medium">{legend}</legend>
      <div className="flex flex-col gap-2">
        {options.map((o) => (
          <label
            key={o.value}
            className="flex cursor-pointer items-start gap-3 rounded-lg border border-line bg-surface p-3"
          >
            <input
              type="radio"
              name={name}
              value={o.value}
              checked={value === o.value}
              onChange={() => onChange(o.value)}
              className="mt-1 h-4 w-4 accent-[var(--c-accent)]"
            />
            <span>
              <span className="font-medium">{o.label}</span>
              {o.hint && <span className="block text-sm text-muted">{o.hint}</span>}
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
