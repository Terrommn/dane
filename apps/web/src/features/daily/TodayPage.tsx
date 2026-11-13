// Hoy: selecciona la fecha objetivo (?date= o start_param del bot) y la muestra claramente. El formulario del
// check-in (secciones de la daily) se integra en la fase 2 usando `useDraft("checkin", fecha)`.
import { Link, useSearchParams } from "react-router";
import { DatePicker } from "../../components/ui/DatePicker";
import { Card, PageHeader } from "../../components/ui/Display";
import { formatLongDate, todayIso } from "../../lib/format";
import { isValidIsoDate } from "../../lib/telegram";
import { useAuth } from "../auth/AuthProvider";
import { ModulePending } from "../more/ModulePending";

export function TodayPage() {
  const { me } = useAuth();
  const [params, setParams] = useSearchParams();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const requested = params.get("date");
  const date = isValidIsoDate(requested) ? requested : today;
  const fromOldMessage = isValidIsoDate(requested) && requested !== today;

  return (
    <div className="space-y-4">
      <PageHeader
        title="Hoy"
        subtitle={<span className="capitalize">{formatLongDate(date)}</span>}
        actions={<DatePicker value={date} onChange={(d) => setParams(d === today ? {} : { date: d })} />}
      />
      {fromOldMessage && (
        <p role="status" className="rounded-xl border border-warn bg-surface p-3 text-sm">
          Estás editando el check-in del <strong>{formatLongDate(date)}</strong>, no el de hoy.{" "}
          <button type="button" className="font-semibold text-link underline" onClick={() => setParams({})}>
            Ir a hoy
          </button>
        </p>
      )}
      {me && !me.settings.onboarding_completed_at && (
        <Card>
          <p className="font-semibold">Termina de configurar tu app</p>
          <p className="mt-1 text-sm text-muted">
            Zona, áreas, módulos, recordatorios y metas opcionales. Puedes hacerlo después.
          </p>
          <Link to="/onboarding" className="mt-2 inline-block font-semibold">
            Configurar ahora →
          </Link>
        </Card>
      )}
      <ModulePending
        title="Check-in diario"
        includes={[
          "Devocional/reflexión, agenda, gasto planeado y tarjeta sugerida",
          "Hábitos fijos, del mes y de la semana",
          "Log, personas, tareas por área",
          "Lo que hice mal / bien hoy; peso, energía 1–5",
          "Guardado parcial, borrador sin conexión y resolución de conflictos",
        ]}
      />
    </div>
  );
}
