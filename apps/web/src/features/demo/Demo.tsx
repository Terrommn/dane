// Datos demo opt-in: un usuario aparte (`users.is_demo`) con datos ficticios marcados [DEMO]. Nunca se mezclan con
// los de tu cuenta. El servidor solo lo permite fuera de producción (DEMO_ENABLED); ante 404 se oculta la opción.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { api } from "../../lib/api";
import { errorMessage, unwrap } from "../../lib/errors";
import { useAuth } from "../auth/AuthProvider";

/** Botón en /login para entrar al espacio demo aislado. */
export function DemoEntry({ redirectTo }: { redirectTo: string }) {
  const { completeDevLogin } = useAuth();
  const [unavailable, setUnavailable] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (unavailable) return null;
  const enter = async (reset: boolean) => {
    setErr(null);
    setBusy(true);
    try {
      const r = await api.POST("/demo/session", { body: { reset, transport: "cookie" } });
      if (r.response.status === 404) {
        setUnavailable(true);
        return;
      }
      const s = unwrap(r);
      await completeDevLogin(s.user_id);
      window.history.replaceState(null, "", redirectTo);
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="space-y-2">
      <p className="text-sm text-muted">
        ¿Solo quieres ver cómo funciona? Entra a un espacio separado con datos ficticios marcados [DEMO]. No
        toca tu cuenta.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" loading={busy} onClick={() => void enter(false)}>
          Explorar con datos demo
        </Button>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void enter(true)}>
          Reiniciar demo
        </Button>
      </div>
      {err && (
        <p role="alert" className="text-sm text-danger">
          {err}
        </p>
      )}
    </div>
  );
}

/** Banner fijo mientras la sesión es del usuario demo. */
export function DemoBanner() {
  const { me, logout } = useAuth();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  if (!me?.user.is_demo) return null;
  return (
    <>
      <div
        role="status"
        className="sticky top-0 z-40 flex flex-wrap items-center justify-center gap-3 bg-warn px-4 py-2 text-center text-sm font-semibold text-black"
      >
        <span>Modo demo — datos ficticios</span>
        <button
          type="button"
          className="rounded-md border border-black/40 px-2 py-0.5 underline-offset-2 hover:underline"
          onClick={() => setConfirm(true)}
        >
          Salir del demo
        </button>
      </div>
      <ConfirmDialog
        open={confirm}
        title="¿Salir del modo demo?"
        confirmLabel="Salir"
        loading={busy}
        onClose={() => setConfirm(false)}
        onConfirm={async () => {
          setBusy(true);
          try {
            await logout();
          } finally {
            setBusy(false);
            setConfirm(false);
          }
        }}
        message="Se cierra la sesión demo y se borran sus datos locales de este dispositivo. Tu cuenta real no se ve afectada."
      />
    </>
  );
}
