// Instrucciones de instalación por plataforma. Distingue el acceso desde Telegram (Mini App, sin instalar) de la PWA
// instalada desde el navegador.
import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import { Card, PageHeader } from "../../components/ui/Display";
import { useAuth } from "../auth/AuthProvider";

interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

export function detectPlatform(ua: string): "ios" | "android" | "desktop" {
  if (/iPhone|iPad|iPod/i.test(ua) || (/Macintosh/.test(ua) && /Mobile/.test(ua))) return "ios";
  if (/Android/i.test(ua)) return "android";
  return "desktop";
}

export function InstallPage() {
  const { isTelegram } = useAuth();
  const [deferred, setDeferred] = useState<BeforeInstallPromptEvent | null>(null);
  const standalone =
    typeof window !== "undefined" && window.matchMedia?.("(display-mode: standalone)").matches;
  const platform = detectPlatform(navigator.userAgent);

  useEffect(() => {
    const h = (e: Event) => {
      e.preventDefault();
      setDeferred(e as BeforeInstallPromptEvent);
    };
    window.addEventListener("beforeinstallprompt", h);
    return () => window.removeEventListener("beforeinstallprompt", h);
  }, []);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Instalar la app"
        subtitle="Una sola app para Telegram, navegador y pantalla de inicio."
      />
      {standalone && (
        <p role="status" className="rounded-xl border border-ok bg-surface p-3 text-sm">
          Ya estás usando la app instalada.
        </p>
      )}
      <Card title="Telegram (acceso directo)">
        <p className="text-sm">
          {isTelegram ? "Ahora mismo estás dentro de Telegram. " : ""}
          Desde el bot abres la app con el botón del check-in o el menú. No se instala nada: la sesión se
          valida con Telegram cada vez que la abres. Ideal para el check-in rápido desde los recordatorios.
        </p>
      </Card>
      <Card title="App instalada (PWA)">
        <p className="mb-3 text-sm">
          Instalada desde el navegador funciona a pantalla completa, abre más rápido y guarda borradores del
          check-in y del Inbox sin conexión. Inicias sesión con Telegram Login (es una sesión distinta a la de
          Telegram).
        </p>
        {isTelegram && (
          <p className="mb-3 text-sm text-warn">
            Para instalarla, abre esta dirección en tu navegador (Safari o Chrome), no dentro de Telegram.
          </p>
        )}
        {deferred && (
          <Button
            onClick={async () => {
              await deferred.prompt();
              await deferred.userChoice;
              setDeferred(null);
            }}
          >
            Instalar ahora
          </Button>
        )}
        <div className="mt-3 space-y-3 text-sm">
          <details open={platform === "ios"}>
            <summary className="cursor-pointer font-semibold">iPhone / iPad (Safari)</summary>
            <ol className="mt-2 list-decimal space-y-1 pl-5">
              <li>Abre la app en Safari.</li>
              <li>Toca Compartir (cuadro con flecha).</li>
              <li>Elige «Agregar a pantalla de inicio» y confirma.</li>
            </ol>
          </details>
          <details open={platform === "android"}>
            <summary className="cursor-pointer font-semibold">Android (Chrome)</summary>
            <ol className="mt-2 list-decimal space-y-1 pl-5">
              <li>Abre la app en Chrome.</li>
              <li>Toca «Instalar app» en el aviso o en el menú ⋮.</li>
            </ol>
          </details>
          <details open={platform === "desktop"}>
            <summary className="cursor-pointer font-semibold">Computadora (Chrome, Edge)</summary>
            <ol className="mt-2 list-decimal space-y-1 pl-5">
              <li>Abre la app en el navegador.</li>
              <li>
                Haz clic en el ícono de instalar en la barra de direcciones o en el menú → «Instalar Vida».
              </li>
            </ol>
          </details>
        </div>
      </Card>
    </div>
  );
}
