// Aviso de nueva versión del service worker. No recarga sola: el usuario decide. Los borradores viven en IndexedDB y
// sobreviven a la recarga; antes de activar se da un instante para que terminen las escrituras pendientes.
import { useRegisterSW } from "virtual:pwa-register/react";
import { Button } from "../components/ui/Button";

export function UpdatePrompt() {
  const {
    needRefresh: [needRefresh, setNeedRefresh],
    updateServiceWorker,
  } = useRegisterSW({
    onRegisterError(e) {
      console.warn("No se pudo registrar el service worker", e);
    },
  });
  if (!needRefresh) return null;
  return (
    <div
      role="status"
      className="fixed inset-x-0 z-50 mx-auto flex max-w-md items-center gap-3 rounded-xl border border-line bg-surface p-3 shadow-lg"
      style={{ bottom: "calc(5.5rem + var(--safe-bottom))" }}
    >
      <p className="min-w-0 flex-1 text-sm">Hay una nueva versión disponible. Tus borradores se conservan.</p>
      <Button variant="secondary" size="sm" onClick={() => setNeedRefresh(false)}>
        Después
      </Button>
      <Button
        size="sm"
        onClick={async () => {
          // Deja que terminen los guardados con retardo (useDraft escribe a los ~400 ms).
          await new Promise((r) => setTimeout(r, 600));
          await updateServiceWorker(true);
        }}
      >
        Actualizar
      </Button>
    </div>
  );
}
