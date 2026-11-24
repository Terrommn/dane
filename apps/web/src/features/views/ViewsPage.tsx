// Equivalente de las 7 Bases del vault. Las vistas de módulos de otras pantallas enlazan a ellas; aquí se muestran
// "Proyectos y Dev" (Dev reciente + ADRs), "Personas" (agrupadas por área) y "Pipeline" (contenido por estado).
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { QueryState } from "../../components/ui/States";
import { api } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { daysToPublishLabel, STATUS_LABEL, useBoard } from "../content/contentApi";
import { ADR_STATUS, adrCode } from "../dev/devApi";
import { ENTITY_LABEL, entityHref } from "../notes/common";
import { usePeopleByArea } from "../people/peopleApi";

const OTHER_BASES = [
  { to: "/habitos", label: "Hábitos", hint: "Últimos 30 días de ejercicio, lectura y energía" },
  { to: "/objetivos", label: "Objetivos", hint: "Por tipo y periodo" },
  { to: "/lecturas", label: "Lecturas", hint: "Tarjetas por estado y tabla por rating" },
];

export function ViewsPage() {
  const projects = useQuery({
    queryKey: ["views", "projects"],
    queryFn: async () => unwrap(await api.GET("/views/projects")),
  });
  const people = usePeopleByArea();
  const board = useBoard();
  return (
    <div className="space-y-4">
      <PageHeader title="Vistas" subtitle="Las vistas tipo base de datos del vault" />
      <Card title="Otras vistas">
        <ul className="grid gap-2 sm:grid-cols-2">
          {OTHER_BASES.map((b) => (
            <li key={b.to}>
              <Link to={b.to} className="block rounded-lg border border-line p-3 hover:bg-surface-2">
                <span className="font-medium">{b.label}</span>
                <span className="block text-sm text-muted">{b.hint}</span>
              </Link>
            </li>
          ))}
        </ul>
      </Card>

      <Card title="Proyectos y Dev · Dev reciente">
        <QueryState query={projects}>
          {(v) => (
            <>
              {v.recent.length === 0 ? (
                <p className="text-sm text-muted">Sin actividad reciente.</p>
              ) : (
                <ul className="divide-y divide-line">
                  {v.recent.map((a) => (
                    <li
                      key={`${a.entity_type}${a.entity_id}`}
                      className="flex flex-wrap items-center justify-between gap-2 py-2"
                    >
                      <Link to={entityHref(a.entity_type, a.entity_id, a.on_date)}>{a.title}</Link>
                      <span className="flex items-center gap-2 text-sm text-muted">
                        <Badge>{ENTITY_LABEL[a.entity_type] ?? a.entity_type}</Badge>
                        {formatDateTime(a.updated_at)}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              <h3 className="mt-4 mb-2 font-semibold">ADRs</h3>
              {v.adrs.length === 0 ? (
                <p className="text-sm text-muted">Sin ADRs.</p>
              ) : (
                <ul className="divide-y divide-line">
                  {v.adrs.map((a) => (
                    <li key={a.id} className="flex flex-wrap items-center justify-between gap-2 py-2">
                      <Link to={`/dev/adr/${a.id}`}>
                        {adrCode(a.number)} {a.title}
                      </Link>
                      <span className="flex items-center gap-2 text-sm text-muted">
                        <Badge>{ADR_STATUS[a.status] ?? a.status}</Badge>
                        {formatShortDate(a.decided_on)}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </QueryState>
      </Card>

      <Card title="Personas (por área)">
        <QueryState query={people}>
          {(groups) => (
            <div className="space-y-3">
              {groups
                .filter((g) => g.people.length)
                .map((g) => (
                  <section key={g.area_id ?? "none"}>
                    <h3 className="font-semibold">{g.area_name}</h3>
                    <ul className="text-sm">
                      {g.people.map((p) => (
                        <li key={p.id} className="flex flex-wrap justify-between gap-2 py-1">
                          <Link to={`/personas/${p.id}`}>{p.name}</Link>
                          <span className="text-muted">
                            {[p.relationship, p.org, p.role].filter(Boolean).join(" · ")}
                            {p.days_since_contact != null && ` · ${p.days_since_contact} días`}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </section>
                ))}
              {groups.every((g) => !g.people.length) && <p className="text-sm text-muted">Sin personas.</p>}
            </div>
          )}
        </QueryState>
      </Card>

      <Card title="Pipeline de contenido" actions={<Link to="/contenido">Abrir contenido</Link>}>
        <QueryState query={board}>
          {(cols) => (
            <ul className="grid gap-2 sm:grid-cols-5">
              {cols.map((c) => (
                <li key={c.status} className="rounded-lg border border-line p-2">
                  <p className="font-semibold">
                    {STATUS_LABEL[c.status] ?? c.status} ({c.items.length})
                  </p>
                  <ul className="mt-1 space-y-1 text-sm">
                    {c.items.slice(0, 5).map((p) => (
                      <li key={p.id}>
                        <Link to={`/contenido/${p.id}`}>{p.title}</Link>
                        {p.publish_on && (
                          <span className="text-muted"> · {daysToPublishLabel(p.days_to_publish)}</span>
                        )}
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          )}
        </QueryState>
      </Card>
    </div>
  );
}
