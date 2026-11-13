import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { type Column, DataTable } from "../../components/ui/DataTable";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { errorMessage } from "../../lib/errors";
import { formatShortDate } from "../../lib/format";
import {
  type ContentView,
  daysToPublishLabel,
  STATUS_LABEL,
  STATUSES,
  useBoard,
  useContentList,
  useContentMutations,
  usePublished,
} from "./contentApi";

type Tab = "tabla" | "tablero" | "publicados";

function TableView() {
  const [status, setStatus] = useState("");
  const list = useContentList(status || undefined);
  const navigate = useNavigate();
  const columns: Column<ContentView>[] = [
    { key: "title", header: "Pieza", render: (p) => p.title, sortValue: (p) => p.title },
    {
      key: "status",
      header: "Estado",
      render: (p) => <Badge>{STATUS_LABEL[p.status] ?? p.status}</Badge>,
      sortValue: (p) => STATUSES.indexOf(p.status as never),
    },
    { key: "format", header: "Formato", render: (p) => p.format, hideOnCard: true },
    {
      key: "platforms",
      header: "Plataformas",
      render: (p) => p.platforms.join(", ") || "—",
      hideOnCard: true,
    },
    { key: "hook", header: "Hook", render: (p) => p.chosen_hook_text ?? "—" },
    {
      key: "pub",
      header: "Publicación",
      render: (p) => (p.publish_on ? formatShortDate(p.publish_on) : "—"),
      sortValue: (p) => p.publish_on,
    },
    {
      key: "days",
      header: "Días",
      render: (p) => daysToPublishLabel(p.days_to_publish),
      sortValue: (p) => p.days_to_publish,
    },
  ];
  return (
    <div className="space-y-3">
      <SelectField label="Estado" value={status} onChange={(e) => setStatus(e.target.value)}>
        <option value="">Todos</option>
        {STATUSES.map((s) => (
          <option key={s} value={s}>
            {STATUS_LABEL[s]}
          </option>
        ))}
      </SelectField>
      <QueryState query={list}>
        {(rows) => (
          <DataTable
            caption="Pipeline de contenido"
            rows={rows}
            columns={columns}
            rowKey={(p) => p.id}
            searchText={(p) => `${p.title} ${p.idea} ${p.pillar ?? ""}`}
            onRowClick={(p) => navigate(`/contenido/${p.id}`)}
            initialSort={{ key: "pub", dir: "asc" }}
            emptyTitle="Sin piezas"
          />
        )}
      </QueryState>
    </div>
  );
}

function BoardView() {
  const board = useBoard();
  return (
    <QueryState query={board}>
      {(cols) => (
        <div className="grid gap-3 md:grid-cols-5">
          {cols.map((c) => (
            <section
              key={c.status}
              className="min-w-0 rounded-xl border border-line bg-surface-2 p-2"
              aria-label={STATUS_LABEL[c.status]}
            >
              <h2 className="mb-2 px-1 font-semibold">
                {STATUS_LABEL[c.status] ?? c.status} <span className="text-muted">({c.items.length})</span>
              </h2>
              <ul className="space-y-2">
                {c.items.map((p) => (
                  <li key={p.id}>
                    <Link
                      to={`/contenido/${p.id}`}
                      className="block rounded-lg border border-line bg-surface p-2 text-sm hover:bg-surface-2"
                    >
                      <span className="font-medium text-fg">{p.title}</span>
                      {p.chosen_hook_text && (
                        <span className="mt-1 block text-muted">“{p.chosen_hook_text}”</span>
                      )}
                      {p.publish_on && (
                        <span className="mt-1 block text-muted">
                          {formatShortDate(p.publish_on)} · {daysToPublishLabel(p.days_to_publish)}
                        </span>
                      )}
                    </Link>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </QueryState>
  );
}

function PublishedView() {
  const [sort, setSort] = useState<"views" | "retention">("views");
  const pub = usePublished(sort);
  const navigate = useNavigate();
  const columns: Column<ContentView>[] = [
    { key: "title", header: "Pieza", render: (p) => p.title },
    {
      key: "date",
      header: "Publicada",
      render: (p) => (p.published_on ? formatShortDate(p.published_on) : "—"),
    },
    {
      key: "platforms",
      header: "Plataformas",
      render: (p) => p.platforms.join(", ") || "—",
      hideOnCard: true,
    },
    { key: "views", header: "Views", render: (p) => (p.views ?? "—").toLocaleString("es-MX") },
    {
      key: "ret",
      header: "Retención",
      render: (p) => (p.retention_pct != null ? `${p.retention_pct}%` : "—"),
    },
  ];
  return (
    <div className="space-y-3">
      <SelectField
        label="Ordenar por"
        value={sort}
        onChange={(e) => setSort(e.target.value as "views" | "retention")}
      >
        <option value="views">Views</option>
        <option value="retention">Retención</option>
      </SelectField>
      <QueryState query={pub}>
        {(rows) => (
          <DataTable
            caption="Publicados y métricas"
            rows={rows as ContentView[]}
            columns={columns}
            rowKey={(p) => p.id}
            onRowClick={(p) => navigate(`/contenido/${p.id}`)}
            emptyTitle="Aún no hay piezas publicadas"
          />
        )}
      </QueryState>
    </div>
  );
}

export function ContentPage() {
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") as Tab) || "tabla";
  const { create } = useContentMutations();
  const navigate = useNavigate();
  const toast = useToast();
  const newPiece = () =>
    create.mutate(
      { title: "Nueva pieza", status: "idea", format: "reel", platforms: ["instagram", "tiktok"] },
      {
        onSuccess: (p) => navigate(`/contenido/${p.id}`),
        onError: (e) => toast.show(errorMessage(e), { tone: "error" }),
      },
    );
  return (
    <div className="space-y-4">
      <PageHeader
        title="Contenido"
        subtitle="idea → guion → grabado → editado → publicado"
        actions={
          <Button onClick={newPiece} loading={create.isPending}>
            Nueva pieza
          </Button>
        }
      />
      <div role="tablist" aria-label="Vista" className="flex flex-wrap gap-2">
        {(
          [
            ["tabla", "Tabla"],
            ["tablero", "Tablero"],
            ["publicados", "Publicados"],
          ] as [Tab, string][]
        ).map(([k, l]) => (
          <Button
            key={k}
            role="tab"
            size="sm"
            aria-selected={tab === k}
            variant={tab === k ? "primary" : "secondary"}
            onClick={() => setParams({ tab: k }, { replace: true })}
          >
            {l}
          </Button>
        ))}
      </div>
      {tab === "tabla" && <TableView />}
      {tab === "tablero" && <BoardView />}
      {tab === "publicados" && <PublishedView />}
      <Card>
        <p className="text-sm text-muted">
          Videos y renders viven fuera de la app (p. ej. Google Drive): en cada pieza solo se guardan los
          enlaces. Las referencias de otros creadores están en <Link to="/swipe">Swipe</Link>.
        </p>
      </Card>
    </div>
  );
}
