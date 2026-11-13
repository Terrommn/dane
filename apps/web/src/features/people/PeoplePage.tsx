import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import { emptyPerson, PersonForm } from "./PersonForm";
import {
  birthdayLabel,
  type PersonView,
  useBirthdays,
  usePeopleByArea,
  usePeopleMutations,
  usePeopleSearch,
  useSuggestions,
} from "./peopleApi";

function PersonCard({ p }: { p: PersonView }) {
  return (
    <li>
      <Link
        to={`/personas/${p.id}`}
        className="block rounded-xl border border-line bg-surface p-3 hover:bg-surface-2"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="font-semibold text-fg">
            {p.name}
            {p.aliases.length > 0 && (
              <span className="font-normal text-muted"> · {p.aliases.join(", ")}</span>
            )}
          </span>
          <span className="flex flex-wrap items-center gap-1">
            {p.relationship && <Badge>{p.relationship}</Badge>}
            {p.archived_at && <Badge tone="warn">Archivada</Badge>}
          </span>
        </div>
        <p className="mt-1 text-sm text-muted">
          {[p.role, p.org].filter(Boolean).join(" · ")}
          {(p.role || p.org) && " · "}
          {p.days_since_contact != null
            ? `${p.days_since_contact} días sin hablar`
            : "Sin contacto registrado"}
          {p.days_to_birthday != null && p.days_to_birthday <= 30 && ` · 🎂 en ${p.days_to_birthday} días`}
        </p>
      </Link>
    </li>
  );
}

function SuggestionsCard() {
  const s = useSuggestions();
  const people = usePeopleSearch("", false);
  const { acceptSuggestion, dismissSuggestion } = usePeopleMutations();
  const toast = useToast();
  const [linkTo, setLinkTo] = useState<Record<string, string>>({});
  if (!s.data?.length) return null;
  const accept = (sg: Schemas["PersonSuggestion"]) => {
    const person_id = linkTo[sg.id];
    acceptSuggestion.mutate(
      {
        id: sg.id,
        body: person_id
          ? { person_id, record_interaction: !!sg.on_date }
          : {
              person: { name: sg.name, email: sg.email ?? null, aliases: [], context_md: sg.context },
              record_interaction: !!sg.on_date,
            },
      },
      {
        onSuccess: (r) =>
          toast.show(r.created ? `Persona creada: ${r.person.name}` : `Vinculado a ${r.person.name}`, {
            tone: "ok",
          }),
        onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
      },
    );
  };
  return (
    <Card title={`Sugerencias (${s.data.length})`}>
      <p className="mb-2 text-sm text-muted">
        Detectadas en agenda, correo o dictado. No se crean hasta que lo confirmes.
      </p>
      <ul className="divide-y divide-line">
        {s.data.map((sg) => (
          <li key={sg.id} className="space-y-2 py-2">
            <p>
              <strong>{sg.name}</strong>
              {sg.email && <span className="text-muted"> · {sg.email}</span>}
              <span className="text-sm text-muted">
                {" "}
                · {sg.source}
                {sg.on_date && ` · ${formatShortDate(sg.on_date)}`}
              </span>
            </p>
            {sg.context && <p className="text-sm text-muted">{sg.context}</p>}
            <div className="grid gap-2 sm:grid-cols-[1fr_auto_auto] sm:items-end">
              <SelectField
                label="Acción"
                value={linkTo[sg.id] ?? ""}
                onChange={(e) => setLinkTo({ ...linkTo, [sg.id]: e.target.value })}
              >
                <option value="">Crear persona nueva</option>
                {(people.data ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    Es {p.name}
                  </option>
                ))}
              </SelectField>
              <Button size="sm" onClick={() => accept(sg)} loading={acceptSuggestion.isPending}>
                Aceptar
              </Button>
              <Button size="sm" variant="ghost" onClick={() => dismissSuggestion.mutate(sg.id)}>
                Descartar
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function PeoplePage() {
  const [q, setQ] = useState("");
  const [archived, setArchived] = useState(false);
  const [creating, setCreating] = useState(false);
  const byArea = usePeopleByArea();
  const search = usePeopleSearch(q.trim(), archived);
  const birthdays = useBirthdays(30);
  const { create } = usePeopleMutations();
  const navigate = useNavigate();
  const [err, setErr] = useState<string | null>(null);
  const searching = q.trim().length > 0 || archived;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Personas"
        subtitle="Último contacto, cumpleaños e interacciones"
        actions={<Button onClick={() => setCreating(true)}>Nueva persona</Button>}
      />
      <SuggestionsCard />
      {birthdays.data && birthdays.data.length > 0 && (
        <Card title="Próximos cumpleaños">
          <ul className="space-y-1">
            {birthdays.data.map((p) => (
              <li key={p.id} className="flex flex-wrap justify-between gap-2">
                <Link to={`/personas/${p.id}`}>{p.name}</Link>
                <span className="text-sm text-muted">
                  🎂 {birthdayLabel(p)} · {p.days_to_birthday === 0 ? "hoy" : `en ${p.days_to_birthday} días`}
                  {p.turning != null && ` · cumple ${p.turning}`}
                </span>
              </li>
            ))}
          </ul>
        </Card>
      )}
      <div className="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-end">
        <TextField label="Buscar por nombre o alias" value={q} onChange={(e) => setQ(e.target.value)} />
        <CheckboxField
          label="Archivadas"
          checked={archived}
          onChange={(e) => setArchived(e.target.checked)}
        />
      </div>
      {searching ? (
        <QueryState
          query={search}
          isEmpty={(d) => d.length === 0}
          empty={<EmptyState title="Nadie coincide" />}
        >
          {(d) => (
            <ul className="space-y-2">
              {d.map((p) => (
                <PersonCard key={p.id} p={p} />
              ))}
            </ul>
          )}
        </QueryState>
      ) : (
        <QueryState
          query={byArea}
          isEmpty={(d) => d.every((g) => g.people.length === 0)}
          empty={
            <EmptyState
              title="Aún no hay personas"
              action={<Button onClick={() => setCreating(true)}>Agregar</Button>}
            />
          }
        >
          {(groups) =>
            groups
              .filter((g) => g.people.length > 0)
              .map((g) => (
                <section key={g.area_id ?? "none"} className="space-y-2">
                  <h2 className="text-lg font-semibold">{g.area_name}</h2>
                  <ul className="space-y-2">
                    {g.people.map((p) => (
                      <PersonCard key={p.id} p={p} />
                    ))}
                  </ul>
                </section>
              ))
          }
        </QueryState>
      )}
      <Dialog open={creating} onClose={() => setCreating(false)} title="Nueva persona" size="lg">
        <PersonForm
          initial={emptyPerson()}
          submitLabel="Crear"
          loading={create.isPending}
          serverError={err}
          onSubmit={(v) =>
            create.mutate(v, {
              onSuccess: (p) => {
                setCreating(false);
                navigate(`/personas/${p.id}`);
              },
              onError: (e) => setErr(errorMessage(e)),
            })
          }
        />
      </Dialog>
    </div>
  );
}
