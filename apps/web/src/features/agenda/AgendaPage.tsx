import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { DatePicker } from "../../components/ui/DatePicker";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog";
import { Badge, PageHeader } from "../../components/ui/Display";
import { CheckboxField, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { errorMessage, unwrap } from "../../lib/errors";
import { todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";

type CalendarEvent = Schemas["CalendarEvent"];

/** "HH:MM" de un instante en la zona del usuario. */
export function localTime(iso: string | null | undefined, timeZone?: string): string {
  if (!iso) return "";
  return new Intl.DateTimeFormat("es-MX", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  }).format(new Date(iso));
}

/** "10:00–11:00 Título" como en la agenda del vault. */
export function eventLine(e: CalendarEvent, tz?: string): string {
  if (e.all_day) return `Todo el día · ${e.title}`;
  const s = localTime(e.starts_at, tz);
  const en = localTime(e.ends_at, tz);
  return `${s}${en ? `–${en}` : ""} ${e.title}`.trim();
}

function EventDialog({
  date,
  event,
  onClose,
}: {
  date: string;
  event: CalendarEvent | null;
  onClose: () => void;
}) {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? undefined;
  const qc = useQueryClient();
  const [f, setF] = useState({
    title: event?.title ?? "",
    on_date: event?.on_date ?? date,
    start_time: localTime(event?.starts_at, tz),
    end_time: localTime(event?.ends_at, tz),
    all_day: event?.all_day ?? false,
    location: event?.location ?? "",
  });
  const [err, setErr] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: async () => {
      const body = {
        title: f.title.trim(),
        on_date: f.on_date,
        start_time: f.all_day || !f.start_time ? null : f.start_time,
        end_time: f.all_day || !f.end_time ? null : f.end_time,
        all_day: f.all_day,
        location: f.location || null,
      };
      return event
        ? unwrap(await api.PUT("/agenda/{id}", { params: { path: { id: event.id } }, body }))
        : unwrap(await api.POST("/agenda", { body }));
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["agenda"] });
      onClose();
    },
    onError: (e) => setErr(errorMessage(e)),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={event ? "Editar evento" : "Nuevo evento"}
      footer={
        <Button
          loading={save.isPending}
          onClick={() => {
            if (!f.title.trim()) return setErr("El título es obligatorio");
            if (!f.all_day && f.start_time && f.end_time && f.end_time < f.start_time)
              return setErr("La hora de fin es antes del inicio");
            save.mutate();
          }}
        >
          Guardar
        </Button>
      }
    >
      <div className="space-y-3">
        {err && (
          <p role="alert" className="text-sm text-danger">
            {err}
          </p>
        )}
        <TextField label="Título" value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} />
        <TextField
          label="Fecha"
          type="date"
          value={f.on_date}
          onChange={(e) => setF({ ...f, on_date: e.target.value })}
        />
        <CheckboxField
          label="Todo el día"
          checked={f.all_day}
          onChange={(e) => setF({ ...f, all_day: e.target.checked })}
        />
        {!f.all_day && (
          <div className="grid grid-cols-2 gap-3">
            <TextField
              label="Inicio"
              type="time"
              value={f.start_time}
              onChange={(e) => setF({ ...f, start_time: e.target.value })}
            />
            <TextField
              label="Fin"
              type="time"
              value={f.end_time}
              onChange={(e) => setF({ ...f, end_time: e.target.value })}
            />
          </div>
        )}
        <TextField
          label="Lugar"
          value={f.location}
          onChange={(e) => setF({ ...f, location: e.target.value })}
        />
        <p className="text-sm text-muted">Horas en tu zona ({tz ?? "local"}).</p>
      </div>
    </Dialog>
  );
}

export function AgendaPage() {
  const { me } = useAuth();
  const tz = me?.settings.timezone ?? undefined;
  const [params, setParams] = useSearchParams();
  const date = params.get("date") ?? todayIso(tz);
  const day = useQuery({
    queryKey: ["agenda", date],
    queryFn: async () => unwrap(await api.GET("/agenda/day/{date}", { params: { path: { date } } })),
  });
  const qc = useQueryClient();
  const toast = useToast();
  const [editing, setEditing] = useState<CalendarEvent | "new" | null>(null);
  const [deleting, setDeleting] = useState<CalendarEvent | null>(null);
  const del = useMutation({
    mutationFn: async (id: string) => unwrap(await api.DELETE("/agenda/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      setDeleting(null);
      void qc.invalidateQueries({ queryKey: ["agenda"] });
    },
    onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
  });
  return (
    <div className="space-y-4">
      <PageHeader
        title="Agenda"
        subtitle="Eventos del día: manuales y de Google Calendar (solo lectura)"
        actions={<Button onClick={() => setEditing("new")}>Nuevo evento</Button>}
      />
      <DatePicker value={date} onChange={(d) => setParams({ date: d }, { replace: true })} />
      <QueryState
        query={day}
        isEmpty={(d) => d.length === 0}
        empty={<EmptyState title="Sin eventos este día" />}
      >
        {(events) => (
          <ul className="divide-y divide-line rounded-xl border border-line bg-surface">
            {events.map((e) => (
              <li key={e.id} className="flex flex-wrap items-center justify-between gap-2 p-3">
                <div className="min-w-0">
                  <p className="font-medium">{eventLine(e, tz)}</p>
                  {e.location && <p className="text-sm text-muted">{e.location}</p>}
                </div>
                <div className="flex items-center gap-2">
                  {e.source === "google" ? (
                    <>
                      <Badge tone="accent">Google</Badge>
                      {e.html_link && (
                        <a href={e.html_link} target="_blank" rel="noopener noreferrer" className="text-sm">
                          Abrir
                        </a>
                      )}
                    </>
                  ) : (
                    <>
                      <Button size="sm" variant="secondary" onClick={() => setEditing(e)}>
                        Editar
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => setDeleting(e)}>
                        Eliminar
                      </Button>
                    </>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </QueryState>
      <p className="text-sm text-muted">
        Un evento en la agenda no confirma que asististe ni con quién hablaste: eso se registra en el día o en
        Personas.
      </p>
      {editing && (
        <EventDialog
          date={date}
          event={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
        />
      )}
      <ConfirmDialog
        open={!!deleting}
        title="Eliminar evento"
        message={`¿Eliminar «${deleting?.title ?? ""}»?`}
        danger
        confirmLabel="Eliminar"
        loading={del.isPending}
        onConfirm={() => deleting && del.mutate(deleting.id)}
        onClose={() => setDeleting(null)}
      />
    </div>
  );
}
