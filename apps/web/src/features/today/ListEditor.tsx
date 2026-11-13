// Lista editable de viñetas breves (log, "Lo que hice bien/mal hoy"). Solo lo que el usuario escribe.
import { useId, useState } from "react";
import { Button } from "../../components/ui/Button";

export function ListEditor({
  label,
  items,
  onChange,
  placeholder,
  emptyHint,
}: {
  label: string;
  items: string[];
  onChange: (items: string[]) => void;
  placeholder?: string;
  emptyHint?: string;
}) {
  const id = useId();
  const [draft, setDraft] = useState("");
  const add = () => {
    const t = draft.trim();
    if (!t) return;
    onChange([...items, t]);
    setDraft("");
  };
  return (
    <div>
      <p id={`${id}-label`} className="mb-1 text-sm font-medium">
        {label}
      </p>
      {items.length === 0 && emptyHint && <p className="mb-2 text-sm text-muted">{emptyHint}</p>}
      <ul aria-labelledby={`${id}-label`} className="space-y-1.5">
        {items.map((it, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: el orden es la identidad (lista simple de textos)
          <li key={i} className="flex items-start gap-2">
            <span aria-hidden className="mt-2 text-muted">
              •
            </span>
            <input
              aria-label={`${label} ${i + 1}`}
              value={it}
              onChange={(e) => onChange(items.map((x, j) => (j === i ? e.target.value : x)))}
              className="min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 py-1.5"
            />
            <Button
              variant="ghost"
              size="sm"
              aria-label={`Quitar ${it || `elemento ${i + 1}`}`}
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            >
              ✕
            </Button>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex gap-2">
        <input
          id={`${id}-new`}
          aria-label={`Agregar a ${label}`}
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          className="min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 py-1.5"
        />
        <Button variant="secondary" size="sm" onClick={add} disabled={!draft.trim()}>
          Agregar
        </Button>
      </div>
    </div>
  );
}
