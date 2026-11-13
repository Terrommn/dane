import { Link, Navigate, useSearchParams } from "react-router";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Card, PageHeader } from "../../components/ui/Display";
import { QueryState } from "../../components/ui/States";
import { useAuth } from "../auth/AuthProvider";
import { LogoutControls } from "../auth/LogoutControls";
import { AreasEditor } from "../projects/AreasEditor";
import {
  GoalsSection,
  ModulesSection,
  RegionSection,
  RemindersSection,
  TelegramStatusCard,
} from "./sections";
import { useSaveSettings, useSettings } from "./settingsApi";

export function SettingsPage() {
  const settings = useSettings();
  const { save, saving, conflict, resolve, dismissConflict } = useSaveSettings();
  const { me } = useAuth();
  const [params] = useSearchParams();
  // El callback de Google vuelve a /ajustes?google=… cuando no hay otra ruta: se muestra en Integraciones.
  const google = params.get("google");
  if (google) return <Navigate to={`/ajustes/integraciones?google=${encodeURIComponent(google)}`} replace />;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Ajustes"
        subtitle={me?.user.display_name ? `Cuenta de ${me.user.display_name}` : undefined}
        actions={<Link to="/onboarding">Asistente de configuración</Link>}
      />
      <QueryState query={settings} loadingRows={4}>
        {(s) => (
          <div className="grid gap-4 lg:grid-cols-2">
            <RegionSection key={`r${s.version}`} settings={s} save={save} saving={saving} />
            <ModulesSection key={`m${s.version}`} settings={s} save={save} saving={saving} />
            <RemindersSection key={`n${s.version}`} settings={s} save={save} saving={saving} />
            <GoalsSection key={`g${s.version}`} settings={s} save={save} saving={saving} />
            <Card title="Áreas">
              <AreasEditor />
              <p className="mt-2 text-sm">
                <Link to="/proyectos">Gestionar proyectos →</Link>
              </p>
            </Card>
            <TelegramStatusCard />
            <Card title="Integraciones y datos">
              <ul className="space-y-1 text-sm">
                <li>
                  <Link to="/ajustes/integraciones">Google Calendar, Gmail, ClickUp e IA →</Link>
                </li>
                <li>
                  <Link to="/captura/nueva">Captura libre (como en el bot) →</Link>
                </li>
                <li>
                  <Link to="/importar">Importar el vault de Obsidian →</Link>
                </li>
                <li>
                  <Link to="/exportar">Exportar mis datos →</Link>
                </li>
              </ul>
            </Card>
            <Card title="Sesión y datos de este dispositivo">
              <p className="mb-3 text-sm text-muted">
                Sesión {me?.session_transport === "bearer" ? "de Telegram (Mini App)" : "del navegador"}. Al
                cerrar sesión se borran los borradores y datos guardados en este dispositivo.
              </p>
              <LogoutControls />
              <p className="mt-3 text-sm">
                <Link to="/sincronizacion">Ver cambios pendientes de sincronizar</Link> ·{" "}
                <Link to="/instalar">Instalar la app</Link>
              </p>
            </Card>
          </div>
        )}
      </QueryState>
      {conflict && (
        <ConflictDialog
          open
          mine={conflict.patch as Record<string, unknown>}
          theirs={conflict.current as unknown as Record<string, unknown>}
          onClose={dismissConflict}
          onResolve={(r) => void resolve(r)}
        />
      )}
    </div>
  );
}
