// Detalle de un hábito: identidad estable, versiones de frecuencia con vigencia (el pasado conserva la anterior),
// pausas (fuera del denominador), registro de cualquier día y archivado.
import { useState } from "react";
import { Link, useParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { DatePicker } from "../../components/ui/DatePicker";
import { Badge, Card, KpiCard, PageHeader } from "../../components/ui/Display";
import { TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import type { HabitMark, HabitStatus } from "../today/checkin";
import { cleanFrequency, FrequencyFields, validateFrequency } from "./FrequencyFields";
import { HabitStatusControl } from "./HabitStatusControl";
import { complianceText, streakText } from "./HabitsPage";
import {
  asCompliance,
  asStreaks,
  describeFrequency,
  type FrequencyBody,
  type HabitDetail,
  useHabit,
  useHabitMutations,
} from "./habitsApi";

export function HabitDetailPage() {
  const { id = "" } = useParams();
  const habit = useHabit(id);
  return (
    <div className="space-y-4">
      <p>
        <Link to="/habitos">← Hábitos</Link>
      </p>
      <QueryState query={habit}>{(h) => <Detail h={h} />}</QueryState>
    </div>
  );
}

function Detail({ h }: { h: HabitDetail }) {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? "America/Monterrey";
  const today = todayIso(tz);
  const m = useHabitMutations();
  const toast = useToast();
  const [name, setName] = useState(h.habit.name);
  const [unit, setUnit] = useState(h.habit.unit ?? "");
  const [description, setDescription] = useState(h.habit.description);
  const [freq, setFreq] = useState<FrequencyBody>({ frequency: "daily" });
  const [validFrom, setValidFrom] = useState(today);
  const [pause, setPause] = useState({ starts_on: today, ends_on: "", reason: "" });
  const [entryDate, setEntryDate] = useState(today);
  const month = asCompliance(h.this_month);
  const last30 = asCompliance(h.last_30_days);
  const streaks = asStreaks(h.streaks);
  const entry = h.entries.find((e) => e.on_date === entryDate);
  const mark: HabitMark = {
    status: (entry?.status as HabitStatus | undefined) ?? null,
    reason: entry?.reason ?? "",
    quantity: entry?.quantity ?? "",
  };

  const run = async (p: Promise<unknown>, ok: string) => {
    try {
      await p;
      toast.show(ok, { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict)
        toast.show("Otro dispositivo cambió este hábito; se recargó la versión actual", { tone: "error" });
      else toast.show(errorMessage(e), { tone: "error" });
    }
  };

  const saveBasics = (archived: boolean) =>
    run(
      m.update.mutateAsync({
        id: h.habit.id,
        body: {
          name: name.trim(),
          unit: unit.trim() || null,
          description,
          area_id: h.habit.area_id ?? null,
          position: h.habit.position,
          archived,
          version: h.habit.version,
        },
      }),
      archived ? "Hábito archivado" : h.habit.archived_at ? "Hábito recuperado" : "Guardado",
    );

  return (
    <>
      <PageHeader
        title={h.habit.name}
        subtitle={
          <>
            {describeFrequency(h.current_version)}
            {h.goal_title && (
              <>
                {" · "}
                <Link to={`/objetivos/${h.habit.goal_id}`}>{h.goal_title}</Link>
              </>
            )}
          </>
        }
        actions={h.habit.archived_at ? <Badge>archivado</Badge> : undefined}
      />
      <div className="grid gap-3 sm:grid-cols-3">
        <KpiCard
          label="Este mes (cerrado)"
          value={complianceText(month).split(" · ")[0]}
          sub={complianceText(month)}
        />
        <KpiCard
          label="Últimos 30 días"
          value={complianceText(last30).split(" · ")[0]}
          sub={complianceText(last30)}
        />
        <KpiCard label="Rachas" value={streaks.current} sub={streakText(streaks)} />
      </div>

      <Card title="Registrar un día">
        <div className="mb-3">
          <DatePicker value={entryDate} max={today} onChange={setEntryDate} label="Día a registrar" />
        </div>
        <HabitStatusControl
          name={`${h.habit.name} · ${formatShortDate(entryDate)}`}
          unit={h.habit.unit}
          mark={mark}
          onChange={(next) => {
            if (next.status === null)
              void run(
                m.clearEntry.mutateAsync({ id: h.habit.id, date: entryDate }),
                "Vuelve a «sin registrar»",
              );
            else if (next.status === "not_applicable" && !next.reason.trim()) return;
            else
              void run(
                m.setEntry.mutateAsync({
                  id: h.habit.id,
                  date: entryDate,
                  body: {
                    status: next.status,
                    reason: next.reason.trim() || null,
                    quantity: next.quantity.trim() || null,
                    version: entry?.version ?? null,
                  },
                }),
                "Registrado",
              );
          }}
        />
        <p className="mt-2 text-xs text-muted">
          Para "no aplica" escribe el motivo y vuelve a elegirlo. Editar días pasados recalcula métricas.
        </p>
      </Card>

      <Card title="Datos">
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField label="Nombre" value={name} onChange={(e) => setName(e.target.value)} />
          <TextField label="Unidad (opcional)" value={unit} onChange={(e) => setUnit(e.target.value)} />
          <TextArea
            className="sm:col-span-2"
            label="Descripción"
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button onClick={() => void saveBasics(h.habit.archived_at != null)} loading={m.update.isPending}>
            Guardar
          </Button>
          <Button variant="secondary" onClick={() => void saveBasics(h.habit.archived_at == null)}>
            {h.habit.archived_at ? "Recuperar" : "Archivar"}
          </Button>
        </div>
      </Card>

      <Card title="Frecuencia e historial de versiones">
        <ul className="mb-4 space-y-1 text-sm">
          {h.versions.map((v) => (
            <li key={v.id}>
              <strong>{describeFrequency(v)}</strong> · desde {v.valid_from}
              {v.valid_to ? ` hasta ${v.valid_to}` : " (vigente)"}
              {v.target_quantity ? ` · meta ${Number(v.target_quantity)} ${h.habit.unit ?? ""}` : ""}
            </li>
          ))}
        </ul>
        <p className="mb-2 text-sm text-muted">
          Cambiar la frecuencia crea una versión nueva; el pasado no se recalcula con la regla nueva.
        </p>
        <FrequencyFields value={freq} onChange={setFreq} unit={h.habit.unit} />
        <TextField
          className="mt-3"
          label="Vigente desde"
          type="date"
          value={validFrom}
          onChange={(e) => setValidFrom(e.target.value)}
        />
        <Button
          className="mt-3"
          onClick={() => {
            const err = validateFrequency(freq);
            if (err) return toast.show(err, { tone: "error" });
            void run(
              m.addVersion.mutateAsync({
                id: h.habit.id,
                body: { ...cleanFrequency(freq), valid_from: validFrom },
              }),
              "Nueva frecuencia guardada",
            );
          }}
          loading={m.addVersion.isPending}
        >
          Cambiar frecuencia
        </Button>
      </Card>

      <Card title="Pausas">
        {h.pauses.length === 0 ? (
          <p className="mb-3 text-sm text-muted">
            Sin pausas. Los días en pausa no cuentan en el cumplimiento ni rompen rachas.
          </p>
        ) : (
          <ul className="mb-3 space-y-1 text-sm">
            {h.pauses.map((p) => (
              <li key={p.id} className="flex flex-wrap items-center gap-2">
                <span>
                  {p.starts_on} → {p.ends_on ?? "sin fin"}
                  {p.reason && ` · ${p.reason}`}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    void run(m.deletePause.mutateAsync({ id: h.habit.id, pauseId: p.id }), "Pausa eliminada")
                  }
                >
                  Quitar
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div className="grid gap-2 sm:grid-cols-3">
          <TextField
            label="Desde"
            type="date"
            value={pause.starts_on}
            onChange={(e) => setPause({ ...pause, starts_on: e.target.value })}
          />
          <TextField
            label="Hasta (opcional)"
            type="date"
            value={pause.ends_on}
            onChange={(e) => setPause({ ...pause, ends_on: e.target.value })}
          />
          <TextField
            label="Motivo"
            value={pause.reason}
            onChange={(e) => setPause({ ...pause, reason: e.target.value })}
          />
        </div>
        <Button
          className="mt-3"
          variant="secondary"
          onClick={() =>
            void run(
              m.addPause.mutateAsync({
                id: h.habit.id,
                body: { starts_on: pause.starts_on, ends_on: pause.ends_on || null, reason: pause.reason },
              }),
              "Pausa registrada",
            )
          }
        >
          Pausar
        </Button>
      </Card>
    </>
  );
}
