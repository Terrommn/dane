import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";

type Tone = "info" | "ok" | "error";
interface ToastItem {
  id: number;
  message: ReactNode;
  tone: Tone;
  action?: { label: string; onClick: () => void } | undefined;
  sticky?: boolean | undefined;
}
interface ToastApi {
  show: (
    message: ReactNode,
    opts?: { tone?: Tone; action?: ToastItem["action"]; sticky?: boolean },
  ) => number;
  dismiss: (id: number) => void;
}

const Ctx = createContext<ToastApi | null>(null);
let seq = 0;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const dismiss = useCallback((id: number) => setItems((xs) => xs.filter((x) => x.id !== id)), []);
  const show = useCallback<ToastApi["show"]>(
    (message, opts = {}) => {
      const id = ++seq;
      setItems((xs) => [
        ...xs,
        { id, message, tone: opts.tone ?? "info", action: opts.action, sticky: opts.sticky },
      ]);
      if (!opts.sticky) setTimeout(() => dismiss(id), opts.tone === "error" ? 8000 : 4000);
      return id;
    },
    [dismiss],
  );
  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);
  return (
    <Ctx.Provider value={api}>
      {children}
      <div
        aria-live="polite"
        className="pointer-events-none fixed inset-x-0 z-50 flex flex-col items-center gap-2 px-4"
        style={{ bottom: "calc(5.5rem + var(--safe-bottom))" }}
      >
        {items.map((t) => (
          <div
            key={t.id}
            role={t.tone === "error" ? "alert" : "status"}
            className={`pointer-events-auto flex w-full max-w-md items-center gap-3 rounded-xl border px-4 py-3 shadow-lg ${
              t.tone === "error"
                ? "border-danger bg-surface text-fg"
                : t.tone === "ok"
                  ? "border-ok bg-surface"
                  : "border-line bg-surface"
            }`}
          >
            <div className="min-w-0 flex-1 text-sm">{t.message}</div>
            {t.action && (
              <button
                type="button"
                className="font-semibold text-link"
                onClick={() => {
                  t.action?.onClick();
                  dismiss(t.id);
                }}
              >
                {t.action.label}
              </button>
            )}
            <button
              type="button"
              aria-label="Cerrar aviso"
              className="text-muted"
              onClick={() => dismiss(t.id)}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export function useToast(): ToastApi {
  const c = useContext(Ctx);
  if (!c) throw new Error("useToast fuera de ToastProvider");
  return c;
}
