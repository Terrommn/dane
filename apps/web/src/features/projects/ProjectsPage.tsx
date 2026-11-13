import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import { Button } from "../../components/ui/Button";
import { DataTable } from "../../components/ui/DataTable";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import { ActivityList } from "./ActivityList";
import { AreasEditor } from "./AreasEditor";
import { PROJECT_STATUS, type Project, useAreas, useProjectMutations, useProjects } from "./projectsApi";

function NewProjectDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const areas = useAreas();
  const { create } = useProjectMutations();
  const toast = useToast();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [areaId, setAreaId] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Nuevo proyecto"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            loading={create.isPending}
            onClick={async () => {
              if (!name.trim()) return setError("El nombre es obligatorio");
              try {
                const p = await create.mutateAsync({
                  name: name.trim(),
                  area_id: areaId || null,
                  description,
                });
                toast.show("Proyecto creado", { tone: "ok" });
                onClose();
                navigate(`/proyectos/${p.id}`);
              } catch (e) {
                setError(errorMessage(e));
              }
            }}
          >
            Crear
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <TextField
          label="Nombre"
          value={name}
          onChange={(e) => setName(e.target.value)}
          error={error ?? undefined}
        />
        <SelectField label="Área" value={areaId} onChange={(e) => setAreaId(e.target.value)}>
          <option value="">Sin área</option>
          {areas.data?.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        <TextArea
          label="Descripción"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
        />
      </div>
    </Dialog>
  );
}

export function ProjectsPage() {
  const [showArchived, setShowArchived] = useState(false);
  const [status, setStatus] = useState("");
  const [areaFilter, setAreaFilter] = useState("");
  const [creating, setCreating] = useState(false);
  const projects = useProjects(showArchived);
  const areas = useAreas(true);
  const navigate = useNavigate();
  const areaName = useMemo(() => new Map((areas.data ?? []).map((a) => [a.id, a.name])), [areas.data]);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Áreas y proyectos"
        actions={<Button onClick={() => setCreating(true)}>Nuevo proyecto</Button>}
      />
      <div className="grid gap-4 lg:grid-cols-[2fr_1fr]">
        <Card title="Proyectos">
          <QueryState query={projects}>
            {(list: Project[]) => (
              <DataTable
                caption="Proyectos"
                rows={list.filter(
                  (p) => (!status || p.status === status) && (!areaFilter || p.area_id === areaFilter),
                )}
                rowKey={(p) => p.id}
                searchText={(p) => `${p.name} ${p.description} ${p.repo_url ?? ""}`}
                onRowClick={(p) => navigate(`/proyectos/${p.id}`)}
                initialSort={{ key: "updated", dir: "desc" }}
                emptyTitle="Sin proyectos con esos filtros"
                filters={
                  <>
                    <SelectField label="Estado" value={status} onChange={(e) => setStatus(e.target.value)}>
                      <option value="">Todos</option>
                      {Object.entries(PROJECT_STATUS).map(([k, v]) => (
                        <option key={k} value={k}>
                          {v}
                        </option>
                      ))}
                    </SelectField>
                    <SelectField
                      label="Área"
                      value={areaFilter}
                      onChange={(e) => setAreaFilter(e.target.value)}
                    >
                      <option value="">Todas</option>
                      {areas.data?.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.name}
                        </option>
                      ))}
                    </SelectField>
                    <CheckboxField
                      label="Archivados"
                      checked={showArchived}
                      onChange={(e) => setShowArchived(e.target.checked)}
                    />
                  </>
                }
                columns={[
                  {
                    key: "name",
                    header: "Proyecto",
                    sortValue: (p) => p.name,
                    render: (p) => (
                      <Link
                        to={`/proyectos/${p.id}`}
                        onClick={(e) => e.stopPropagation()}
                        className="font-medium"
                      >
                        {p.name}
                      </Link>
                    ),
                  },
                  {
                    key: "area",
                    header: "Área",
                    sortValue: (p) => areaName.get(p.area_id ?? "") ?? "",
                    render: (p) => areaName.get(p.area_id ?? "") ?? "—",
                  },
                  {
                    key: "status",
                    header: "Estado",
                    sortValue: (p) => p.status,
                    render: (p) => (
                      <span className="inline-flex gap-1">
                        <Badge tone={p.status === "active" ? "ok" : "muted"}>
                          {PROJECT_STATUS[p.status] ?? p.status}
                        </Badge>
                        {p.archived_at && <Badge>Archivado</Badge>}
                      </span>
                    ),
                  },
                  {
                    key: "updated",
                    header: "Actualizado",
                    sortValue: (p) => p.updated_at,
                    render: (p) => formatDateTime(p.updated_at),
                  },
                ]}
              />
            )}
          </QueryState>
        </Card>
        <div className="space-y-4">
          <Card title="Áreas">
            <AreasEditor />
          </Card>
          <Card title="Actividad reciente">
            <ActivityList projectId={null} />
          </Card>
        </div>
      </div>
      <NewProjectDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}
