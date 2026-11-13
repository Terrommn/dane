// Secciones de preferencias reutilizadas por Ajustes y Onboarding. Cada una guarda solo sus campos.
import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import { Card } from "../../components/ui/Display";
import { CheckboxField, RadioGroup, SelectField, TextField } from "../../components/ui/Field";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { fromTimeInput, toTimeInput } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import { MODULES, type Settings, type SettingsPatch, timeZones, WEEKDAYS } from "./settingsApi";

export interface SectionProps {
  settings: Settings;
  save: (p: SettingsPatch) => Promise<unknown>;
  saving: boolean;
  /** En onboarding el botón dice "Guardar y continuar". */
  submitLabel?: string;
  onSaved?: () => void;
}

function useSubmit(save: SectionProps["save"], onSaved?: () => void) {
  const toast = useToast();
  return async (patch: SettingsPatch) => {
    try {
      await save(patch);
      toast.show("Preferencias guardadas", { tone: "ok" });
      onSaved?.();
    } catch (e) {
      toast.show(errorMessage(e), { tone: "error" });
    }
  };
}

export function RegionSection({ settings, save, saving, submitLabel = "Guardar", onSaved }: SectionProps) {
  const [tz, setTz] = useState(settings.timezone);
  const [currency, setCurrency] = useState(settings.currency);
  const [locale, setLocale] = useState(settings.locale);
  const [weekStart, setWeekStart] = useState(settings.week_start);
  const submit = useSubmit(save, onSaved);
  useEffect(() => setTz(settings.timezone), [settings.timezone]);
  return (
    <Card title="Región">
      <form
        className="grid gap-3 sm:grid-cols-2"
        onSubmit={(e) => {
          e.preventDefault();
          void submit({ timezone: tz, currency: currency.toUpperCase(), locale, week_start: weekStart });
        }}
      >
        <SelectField
          label="Zona horaria"
          value={tz}
          onChange={(e) => setTz(e.target.value)}
          hint="Cambiarla afecta el calendario futuro; tu historial no se reclasifica."
        >
          {timeZones().map((z) => (
            <option key={z} value={z}>
              {z}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Moneda (ISO)"
          value={currency}
          maxLength={3}
          onChange={(e) => setCurrency(e.target.value)}
          hint="Ejemplo: MXN"
        />
        <SelectField label="Idioma y formato" value={locale} onChange={(e) => setLocale(e.target.value)}>
          <option value="es-MX">Español (México)</option>
          <option value="es-ES">Español (España)</option>
          <option value="es-US">Español (EE. UU.)</option>
        </SelectField>
        <SelectField
          label="Primer día de la semana (visual)"
          value={weekStart}
          onChange={(e) => setWeekStart(Number(e.target.value))}
          hint="Los periodos semanales usan semana ISO lunes–domingo."
        >
          {WEEKDAYS.slice(1).map((d, i) => (
            <option key={d} value={i + 1}>
              {d}
            </option>
          ))}
        </SelectField>
        <div className="sm:col-span-2">
          <Button type="submit" loading={saving}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </Card>
  );
}

type DevotionalChoice = "devotional" | "meditation" | "off";

export function ModulesSection({ settings, save, saving, submitLabel = "Guardar", onSaved }: SectionProps) {
  const initial = (settings.modules ?? {}) as Record<string, boolean>;
  const [mods, setMods] = useState<Record<string, boolean>>(initial);
  const [dev, setDev] = useState<DevotionalChoice>(
    initial.devotional === false ? "off" : (settings.devotional_mode as DevotionalChoice),
  );
  const submit = useSubmit(save, onSaved);
  return (
    <Card title="Módulos">
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submit({
            modules: { ...mods, devotional: dev !== "off" } as never,
            devotional_mode: dev === "off" ? settings.devotional_mode : dev,
          });
        }}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          {MODULES.map((m) => (
            <CheckboxField
              key={m.key}
              label={m.label}
              hint={m.hint}
              checked={mods[m.key] !== false}
              onChange={(e) => setMods((x) => ({ ...x, [m.key]: e.target.checked }))}
            />
          ))}
        </div>
        <RadioGroup<DevotionalChoice>
          legend="Reflexión diaria"
          name="devotional"
          value={dev}
          onChange={setDev}
          options={[
            {
              value: "devotional",
              label: "Devocional",
              hint: "Pasaje, versículo/acción, aprendizaje, oración y canción",
            },
            { value: "meditation", label: "Meditación", hint: "Misma sección con enfoque de meditación" },
            { value: "off", label: "Desactivado", hint: "Oculta la sección y su hábito" },
          ]}
        />
        <Button type="submit" loading={saving}>
          {submitLabel}
        </Button>
      </form>
    </Card>
  );
}

export function RemindersSection({ settings, save, saving, submitLabel = "Guardar", onSaved }: SectionProps) {
  const [s, setS] = useState({
    daily_enabled: settings.daily_enabled,
    daily_time: toTimeInput(settings.daily_time),
    weekly_enabled: settings.weekly_enabled,
    weekly_weekday: settings.weekly_weekday,
    weekly_time: toTimeInput(settings.weekly_time),
    monthly_enabled: settings.monthly_enabled,
    monthly_time: toTimeInput(settings.monthly_time),
    reminders_paused: settings.reminders_paused,
    paused_until: settings.paused_until ? settings.paused_until.slice(0, 10) : "",
  });
  const submit = useSubmit(save, onSaved);
  const set = <K extends keyof typeof s>(k: K, v: (typeof s)[K]) => setS((x) => ({ ...x, [k]: v }));
  return (
    <Card title="Recordatorios en Telegram">
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submit({
            daily_enabled: s.daily_enabled,
            daily_time: fromTimeInput(s.daily_time),
            weekly_enabled: s.weekly_enabled,
            weekly_weekday: s.weekly_weekday,
            weekly_time: fromTimeInput(s.weekly_time),
            monthly_enabled: s.monthly_enabled,
            monthly_time: fromTimeInput(s.monthly_time),
            reminders_paused: s.reminders_paused,
            paused_until:
              s.reminders_paused && s.paused_until
                ? new Date(`${s.paused_until}T23:59:59`).toISOString()
                : null,
          });
        }}
      >
        <div className="grid gap-3 sm:grid-cols-[auto_1fr] sm:items-end">
          <CheckboxField
            label="Check-in diario"
            checked={s.daily_enabled}
            onChange={(e) => set("daily_enabled", e.target.checked)}
          />
          <TextField
            label="Hora del recordatorio diario"
            type="time"
            value={s.daily_time}
            disabled={!s.daily_enabled}
            onChange={(e) => set("daily_time", e.target.value)}
          />
          <CheckboxField
            label="Revisión semanal"
            checked={s.weekly_enabled}
            onChange={(e) => set("weekly_enabled", e.target.checked)}
          />
          <div className="grid grid-cols-2 gap-2">
            <SelectField
              label="Día"
              value={s.weekly_weekday}
              disabled={!s.weekly_enabled}
              onChange={(e) => set("weekly_weekday", Number(e.target.value))}
            >
              {WEEKDAYS.slice(1).map((d, i) => (
                <option key={d} value={i + 1}>
                  {d}
                </option>
              ))}
            </SelectField>
            <TextField
              label="Hora"
              type="time"
              value={s.weekly_time}
              disabled={!s.weekly_enabled}
              onChange={(e) => set("weekly_time", e.target.value)}
            />
          </div>
          <CheckboxField
            label="Objetivos del mes (día 1)"
            checked={s.monthly_enabled}
            onChange={(e) => set("monthly_enabled", e.target.checked)}
          />
          <TextField
            label="Hora del recordatorio mensual"
            type="time"
            value={s.monthly_time}
            disabled={!s.monthly_enabled}
            onChange={(e) => set("monthly_time", e.target.value)}
          />
        </div>
        <div className="rounded-xl bg-surface-2 p-3">
          <CheckboxField
            label="Pausar todos los recordatorios"
            checked={s.reminders_paused}
            onChange={(e) => set("reminders_paused", e.target.checked)}
          />
          {s.reminders_paused && (
            <TextField
              className="mt-2"
              label="Pausar hasta (opcional)"
              type="date"
              value={s.paused_until}
              onChange={(e) => set("paused_until", e.target.value)}
              hint="Vacío = hasta que los reanudes (/resume en el bot)."
            />
          )}
        </div>
        <Button type="submit" loading={saving}>
          {submitLabel}
        </Button>
      </form>
    </Card>
  );
}

/**
 * Metas opcionales. En onboarding se proponen EJEMPLOS del vault (no son datos del usuario): aparecen desmarcados y
 * solo se guardan si el usuario los marca y confirma.
 */
export function GoalsSection({
  settings,
  save,
  saving,
  submitLabel = "Guardar",
  onSaved,
  asExamples,
}: SectionProps & { asExamples?: boolean }) {
  const [useWeight, setUseWeight] = useState(!asExamples && settings.weight_goal_kg != null);
  const [weight, setWeight] = useState(settings.weight_goal_kg ?? (asExamples ? "70" : ""));
  const [direction, setDirection] = useState(settings.weight_direction ?? "decrease");
  const [useFailures, setUseFailures] = useState(!asExamples && settings.max_devotional_failures != null);
  const [failures, setFailures] = useState(String(settings.max_devotional_failures ?? (asExamples ? 5 : "")));
  const [err, setErr] = useState<string | null>(null);
  const submit = useSubmit(save, onSaved);
  return (
    <Card title="Metas opcionales">
      {asExamples && (
        <p className="mb-3 text-sm text-muted">Son ejemplos editables. No se guarda nada que no marques.</p>
      )}
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          setErr(null);
          const w = Number(weight);
          const f = Number(failures);
          if (useWeight && !(w > 0 && w < 500)) return setErr("El peso meta debe estar entre 0 y 500 kg");
          if (useFailures && !(Number.isInteger(f) && f >= 0))
            return setErr("Las fallas permitidas deben ser un entero ≥ 0");
          void submit({
            weight_goal_kg: useWeight ? String(w) : null,
            weight_direction: useWeight ? direction : null,
            max_devotional_failures: useFailures ? f : null,
          });
        }}
      >
        <div className="rounded-xl border border-line p-3">
          <CheckboxField
            label="Meta de peso"
            checked={useWeight}
            onChange={(e) => setUseWeight(e.target.checked)}
          />
          {useWeight && (
            <div className="mt-2 grid gap-2 sm:grid-cols-2">
              <TextField
                label="Peso meta (kg)"
                inputMode="decimal"
                value={weight}
                onChange={(e) => setWeight(e.target.value)}
              />
              <SelectField label="Dirección" value={direction} onChange={(e) => setDirection(e.target.value)}>
                <option value="decrease">Bajar</option>
                <option value="increase">Subir</option>
                <option value="maintain">Mantener</option>
              </SelectField>
            </div>
          )}
        </div>
        <div className="rounded-xl border border-line p-3">
          <CheckboxField
            label="Máximo de fallas devocionales al mes"
            checked={useFailures}
            onChange={(e) => setUseFailures(e.target.checked)}
          />
          {useFailures && (
            <TextField
              className="mt-2"
              label="Fallas permitidas por mes"
              inputMode="numeric"
              value={failures}
              onChange={(e) => setFailures(e.target.value)}
            />
          )}
        </div>
        <p className="text-sm text-muted">
          La meta de ejercicio (N veces por semana) es la frecuencia de su hábito fijo: se configura en
          Hábitos.
        </p>
        {err && (
          <p role="alert" className="text-sm text-danger">
            {err}
          </p>
        )}
        <Button type="submit" loading={saving}>
          {submitLabel}
        </Button>
      </form>
    </Card>
  );
}

export function TelegramStatusCard() {
  const { me, refreshMe, isTelegram } = useAuth();
  const t = me?.telegram;
  if (!t) return null;
  return (
    <Card
      title="Conexión con Telegram"
      actions={
        <Button variant="ghost" size="sm" onClick={() => void refreshMe()}>
          Actualizar
        </Button>
      }
    >
      {t.chat_known && t.can_message ? (
        <p className="text-ok">✓ El bot puede enviarte recordatorios.</p>
      ) : (
        <div className="space-y-2 text-sm">
          <p className="font-medium text-warn">
            {t.chat_known ? "El bot no puede enviarte mensajes." : "El bot todavía no conoce tu chat."}
          </p>
          {t.last_error && <p className="text-muted">Último error: {t.last_error}</p>}
          <ol className="list-decimal space-y-1 pl-5">
            <li>
              Abre{" "}
              {t.bot_username ? (
                <a href={`https://t.me/${t.bot_username}`} target="_blank" rel="noreferrer">
                  @{t.bot_username}
                </a>
              ) : (
                "el bot"
              )}{" "}
              en Telegram.
            </li>
            <li>Envía /start (si lo bloqueaste, desbloquéalo primero).</li>
            <li>Vuelve aquí y toca «Actualizar».</li>
          </ol>
          {!isTelegram && (
            <p className="text-muted">
              Los recordatorios llegan a tu chat privado con el bot; los botones abren el check-in.
            </p>
          )}
        </div>
      )}
    </Card>
  );
}
