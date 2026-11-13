import { useState } from "react";
import { useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, KpiCard, PageHeader } from "../../components/ui/Display";
import { TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { formatShortDate, todayIso } from "../../lib/format";
import { useAuth } from "../auth/AuthProvider";
import {
  ConflictPrompt,
  FavoriteButton,
  LinksPanel,
  SafeMarkdown,
  useArchiveMutation,
} from "../notes/common";
import { PersonForm, type PersonFormValue } from "./PersonForm";
import {
  birthdayLabel,
  type Person,
  type PersonView,
  personBody,
  usePeopleMutations,
  usePerson,
} from "./peopleApi";

function DatedEntryForm({
  label,
  onAdd,
  loading,
}: {
  label: string;
  onAdd: (text: string, date: string) => void;
  loading?: boolean;
}) {
  const { me } = useAuth();
  const [text, setText] = useState("");
  const [date, setDate] = useState(todayIso(me?.settings.timezone ?? undefined));
  return (
    <form
      className="grid gap-2 sm:grid-cols-[1fr_10rem_auto] sm:items-end"
      onSubmit={(e) => {
        e.preventDefault();
        if (!text.trim()) return;
        onAdd(text.trim(), date);
        setText("");
      }}
    >
      <TextField label={label} value={text} onChange={(e) => setText(e.target.value)} />
      <TextField label="Fecha" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
      <Button type="submit" loading={loading} disabled={!text.trim()}>
        Agregar
      </Button>
    </form>
  );
}

function PersonDetail({
  person,
  notes,
  interactions,
}: {
  person: PersonView;
  notes: { id: string; on_date: string; text: string }[];
  interactions: { id: string; on_date: string; topic: string; source: string }[];
}) {
  const m = usePeopleMutations();
  const archive = useArchiveMutation();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [conflict, setConflict] = useState<Person | null>(null);
  const [pending, setPending] = useState<PersonFormValue | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const save = (v: PersonFormValue, version: number) =>
    m.update.mutate(
      { id: person.id, body: { ...v, archived: person.archived_at != null, version } },
      {
        onSuccess: () => {
          setEditing(false);
          toast.show("Guardado", { tone: "ok" });
        },
        onError: (e) => {
          if (e instanceof ApiError && e.isConflict && e.current) {
            setPending(v);
            setConflict(e.current as Person);
          } else setErr(errorMessage(e));
        },
      },
    );
  const bday = birthdayLabel(person);
  return (
    <div className="space-y-4">
      <PageHeader
        title={person.name}
        subtitle={[person.role, person.org, person.relationship].filter(Boolean).join(" · ") || undefined}
        actions={
          <>
            <FavoriteButton type="person" id={person.id} />
            <Button variant="secondary" onClick={() => setEditing((x) => !x)}>
              {editing ? "Cancelar" : "Editar"}
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                archive.mutate({ type: "person", id: person.id, restore: person.archived_at != null })
              }
            >
              {person.archived_at ? "Recuperar" : "Archivar"}
            </Button>
            <ButtonLink to="/personas">Volver</ButtonLink>
          </>
        }
      />
      <div className="grid gap-3 sm:grid-cols-3">
        <KpiCard
          label="Último contacto"
          value={person.last_contact_on ? formatShortDate(person.last_contact_on) : "—"}
          sub={
            person.days_since_contact != null
              ? `${person.days_since_contact} días sin hablar`
              : "Sin interacciones"
          }
          tone={person.days_since_contact != null && person.days_since_contact > 60 ? "warn" : undefined}
        />
        <KpiCard
          label="Cumpleaños"
          value={bday ?? "—"}
          sub={
            person.days_to_birthday != null
              ? `en ${person.days_to_birthday} días${person.turning != null ? ` · cumple ${person.turning}` : ""}`
              : "Sin fecha"
          }
        />
        <KpiCard
          label="Contacto"
          value={<span className="text-base">{person.contact || person.email || "—"}</span>}
          sub={person.aliases.length ? `Alias: ${person.aliases.join(", ")}` : undefined}
        />
      </div>
      {editing ? (
        <Card title="Editar">
          <PersonForm
            initial={personBody(person)}
            submitLabel="Guardar"
            loading={m.update.isPending}
            serverError={err}
            onSubmit={(v) => save(v, person.version)}
          />
        </Card>
      ) : (
        person.context_md && (
          <Card title="Contexto">
            <SafeMarkdown>{person.context_md}</SafeMarkdown>
          </Card>
        )
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        <Card title="Interacciones">
          <DatedEntryForm
            label="¿De qué hablaron?"
            loading={m.addInteraction.isPending}
            onAdd={(topic, on_date) =>
              m.addInteraction.mutate(
                { id: person.id, topic, on_date },
                { onError: (e) => toast.show(errorMessage(e), { tone: "error" }) },
              )
            }
          />
          <ul className="mt-3 divide-y divide-line">
            {interactions.length === 0 && (
              <li className="py-2 text-sm text-muted">Sin interacciones registradas.</li>
            )}
            {interactions.map((i) => (
              <li key={i.id} className="flex items-start justify-between gap-2 py-2">
                <span>
                  <span className="text-sm text-muted">{formatShortDate(i.on_date)}</span>{" "}
                  {i.topic || "(sin tema)"}
                  {i.source !== "manual" && <Badge>{i.source}</Badge>}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label="Eliminar interacción"
                  onClick={() => m.deleteInteraction.mutate({ id: person.id, interactionId: i.id })}
                >
                  ✕
                </Button>
              </li>
            ))}
          </ul>
        </Card>
        <Card title="Notas">
          <DatedEntryForm
            label="Nota"
            loading={m.addNote.isPending}
            onAdd={(text, on_date) =>
              m.addNote.mutate(
                { id: person.id, text, on_date },
                { onError: (e) => toast.show(errorMessage(e), { tone: "error" }) },
              )
            }
          />
          <ul className="mt-3 divide-y divide-line">
            {notes.length === 0 && <li className="py-2 text-sm text-muted">Sin notas.</li>}
            {notes.map((n) => (
              <li key={n.id} className="flex items-start justify-between gap-2 py-2">
                <span>
                  <span className="text-sm text-muted">{formatShortDate(n.on_date)}</span> {n.text}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label="Eliminar nota"
                  onClick={() => m.deleteNote.mutate({ id: person.id, noteId: n.id })}
                >
                  ✕
                </Button>
              </li>
            ))}
          </ul>
        </Card>
      </div>
      <Card>
        <LinksPanel type="person" id={person.id} />
      </Card>
      <ConflictPrompt
        conflict={conflict}
        mine={(pending ?? {}) as Record<string, unknown>}
        onClose={() => setConflict(null)}
        onTakeServer={() => setEditing(false)}
        onSave={(data, v) => save(data as unknown as PersonFormValue, v)}
      />
    </div>
  );
}

export function PersonDetailPage() {
  const { id = "" } = useParams();
  const q = usePerson(id);
  return (
    <QueryState query={q}>
      {(d) => (
        <PersonDetail
          key={`${d.person.id}:${d.person.version}`}
          person={d.person}
          notes={d.notes}
          interactions={d.interactions}
        />
      )}
    </QueryState>
  );
}
