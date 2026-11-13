// Cerrar sesión con aviso de cambios sin sincronizar y limpieza de datos privados locales.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { defaultSender } from "../../lib/offline/hooks";
import { flushQueue } from "../../lib/offline/store";
import { useAuth } from "./AuthProvider";

export function LogoutControls() {
  const { logout, countPending, userId, isTelegram } = useAuth();
  const toast = useToast();
  const [confirm, setConfirm] = useState<{ everywhere: boolean; pending: number } | null>(null);
  const [busy, setBusy] = useState(false);

  const start = async (everywhere: boolean) => {
    const pending = await countPending();
    setConfirm({ everywhere, pending });
  };
  const run = async () => {
    if (!confirm) return;
    setBusy(true);
    try {
      await logout(confirm.everywhere);
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      <Button variant="secondary" onClick={() => void start(false)}>
        Cerrar sesión
      </Button>
      <Button variant="ghost" onClick={() => void start(true)}>
        Cerrar en todos los dispositivos
      </Button>
      {confirm && (
        <ConfirmDialog
          open
          danger={confirm.pending > 0}
          loading={busy}
          title={confirm.everywhere ? "Cerrar sesión en todos los dispositivos" : "Cerrar sesión"}
          confirmLabel={confirm.pending > 0 ? "Salir y descartar cambios" : "Cerrar sesión"}
          onClose={() => setConfirm(null)}
          onConfirm={() => void run()}
          message={
            <div className="space-y-3">
              {confirm.pending > 0 ? (
                <>
                  <p>
                    Tienes <strong>{confirm.pending}</strong> cambio(s) guardados solo en este dispositivo. Si
                    sales ahora se borrarán.
                  </p>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={async () => {
                      if (!userId) return;
                      const r = await flushQueue(userId, defaultSender);
                      setConfirm({ ...confirm, pending: r.remaining });
                      toast.show(
                        r.remaining ? `Quedan ${r.remaining} sin sincronizar` : "Todo sincronizado",
                        { tone: r.remaining ? "error" : "ok" },
                      );
                    }}
                  >
                    Sincronizar antes de salir
                  </Button>
                </>
              ) : (
                <p>Se borrarán de este dispositivo tus borradores y datos en caché.</p>
              )}
              {isTelegram && (
                <p className="text-sm text-muted">
                  En Telegram, al volver a abrir la app se iniciará sesión de nuevo con tu cuenta.
                </p>
              )}
            </div>
          }
        />
      )}
    </div>
  );
}
