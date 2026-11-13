import { EmptyState, QueryState } from "../../components/ui/States";
import { formatDateTime, formatShortDate } from "../../lib/format";
import { useProjectActivity } from "./projectsApi";

const TYPE_LABEL: Record<string, string> = {
  task: "Tarea",
  note: "Nota",
  adr: "ADR",
  devlog_session: "Devlog",
  content: "Contenido",
};

export function ActivityList({ projectId }: { projectId: string | null }) {
  const q = useProjectActivity(projectId);
  return (
    <QueryState
      query={q}
      isEmpty={(d) => d.length === 0}
      empty={
        <EmptyState title="Sin actividad reciente">
          Tareas, notas, ADRs, devlog y contenido relacionados aparecerán aquí.
        </EmptyState>
      }
    >
      {(items) => (
        <ul className="divide-y divide-line">
          {items.map((i) => (
            <li key={`${i.entity_type}-${i.entity_id}`} className="py-2">
              <p className="break-words font-medium">{i.title}</p>
              <p className="text-sm text-muted">
                {TYPE_LABEL[i.entity_type] ?? i.entity_type}
                {i.status ? ` · ${i.status}` : ""}
                {i.on_date ? ` · ${formatShortDate(i.on_date)}` : ""} · {formatDateTime(i.updated_at)}
              </p>
            </li>
          ))}
        </ul>
      )}
    </QueryState>
  );
}
