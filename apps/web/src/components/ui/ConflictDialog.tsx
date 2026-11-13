// Resolución explícita de conflictos (409): muestra mi versión y la del servidor campo por campo y permite elegir
// la mía, la del servidor o una mezcla. Nunca se sobrescribe en silencio.
import { useMemo, useState } from "react";
import { Button } from "./Button";
import { Dialog } from "./Dialog";

const IGNORED = new Set(["version", "updated_at", "created_at", "id", "user_id"]);

function show(v: unknown): string {
  if (v === null || v === undefined || v === "") return "—";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

export function diffFields(mine: Record<string, unknown>, theirs: Record<string, unknown>): string[] {
  const keys = new Set([...Object.keys(mine), ...Object.keys(theirs)]);
  return [...keys].filter(
    (k) => !IGNORED.has(k) && k in mine && JSON.stringify(mine[k]) !== JSON.stringify(theirs[k]),
  );
}

export function ConflictDialog({
  open,
  mine,
  theirs,
  labels = {},
  onResolve,
  onClose,
}: {
  open: boolean;
  mine: Record<string, unknown>;
  theirs: Record<string, unknown>;
  labels?: Record<string, string>;
  onResolve: (
    r: { choice: "mine" } | { choice: "theirs" } | { choice: "merged"; body: Record<string, unknown> },
  ) => void;
  onClose: () => void;
}) {
  const fields = useMemo(() => diffFields(mine, theirs), [mine, theirs]);
  const [pick, setPick] = useState<Record<string, "mine" | "theirs">>({});
  const merged = () => {
    const out: Record<string, unknown> = { ...mine };
    for (const f of fields) if ((pick[f] ?? "mine") === "theirs") out[f] = theirs[f];
    return out;
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      size="lg"
      title="Este registro cambió en otro dispositivo"
      footer={
        <>
          <Button variant="secondary" onClick={() => onResolve({ choice: "theirs" })}>
            Usar la del servidor
          </Button>
          <Button variant="secondary" onClick={() => onResolve({ choice: "mine" })}>
            Conservar la mía
          </Button>
          <Button onClick={() => onResolve({ choice: "merged", body: merged() })}>Guardar mezcla</Button>
        </>
      }
    >
      <p className="mb-3 text-sm text-muted">Elige por campo qué valor conservar. Nada se guardó todavía.</p>
      {fields.length === 0 ? (
        <p>No hay diferencias en los campos editables; puedes conservar tu versión.</p>
      ) : (
        <ul className="space-y-3">
          {fields.map((f) => (
            <li key={f} className="rounded-xl border border-line p-3">
              <p className="mb-2 font-medium">{labels[f] ?? f}</p>
              <fieldset className="grid gap-2 sm:grid-cols-2">
                <legend className="sr-only">{labels[f] ?? f}</legend>
                {(["mine", "theirs"] as const).map((side) => (
                  <label
                    key={side}
                    className="flex min-w-0 cursor-pointer items-start gap-2 rounded-lg bg-surface-2 p-2"
                  >
                    <input
                      type="radio"
                      name={`conflict-${f}`}
                      checked={(pick[f] ?? "mine") === side}
                      onChange={() => setPick((p) => ({ ...p, [f]: side }))}
                      className="mt-1"
                    />
                    <span className="min-w-0">
                      <span className="block text-xs text-muted">
                        {side === "mine" ? "Mi versión" : "Servidor"}
                      </span>
                      <span className="block break-words">{show(side === "mine" ? mine[f] : theirs[f])}</span>
                    </span>
                  </label>
                ))}
              </fieldset>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
