// Centro de sincronización: operaciones pendientes, con error y en conflicto (resolución explícita).
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { ConflictDialog } from "../../components/ui/ConflictDialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { EmptyState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { QueueItem } from "../../lib/offline/db";
import { useSyncQueue } from "../../lib/offline/hooks";

const DESCRIBE: Record<string, string> = {
  POST: "Crear",
  PUT: "Actualizar",
  PATCH: "Actualizar",
  DELETE: "Eliminar",
};

export function SyncPage() {
  const q = useSyncQueue();
  const toast = useToast();
  const [open, setOpen] = useState<QueueItem | null>(null);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Sincronización"
        subtitle={q.online ? "Conectado" : "Sin conexión: los cambios se guardan en este dispositivo"}
        actions={
          <Button
            variant="secondary"
            loading={q.syncing}
            disabled={!q.online}
            onClick={async () => {
              const r = await q.flush();
              if (r)
                toast.show(`Enviados: ${r.sent}. Pendientes: ${r.remaining}.`, {
                  tone: r.conflicts || r.errors ? "error" : "ok",
                });
            }}
          >
            Sincronizar ahora
          </Button>
        }
      />
      {q.items.length === 0 ? (
        <EmptyState title="Todo está sincronizado">
          No hay cambios guardados solo en este dispositivo.
        </EmptyState>
      ) : (
        <Card title="Cambios en este dispositivo">
          <ul className="divide-y divide-line">
            {q.items.map((i) => (
              <li key={i.id} className="flex flex-wrap items-center gap-2 py-3">
                <div className="min-w-0 flex-1">
                  <p className="font-medium break-words">
                    {DESCRIBE[i.method]} <code className="text-sm">{i.path}</code>
                  </p>
                  <p className="text-sm text-muted">
                    {new Date(i.createdAt).toLocaleString("es-MX")} · intentos: {i.attempts}
                    {i.lastError ? ` · ${i.lastError}` : ""}
                  </p>
                </div>
                {i.status === "pending" && <Badge tone="warn">Pendiente</Badge>}
                {i.status === "error" && (
                  <>
                    <Badge tone="danger">Error</Badge>
                    <Button size="sm" variant="secondary" onClick={() => q.drop(i.id)}>
                      Descartar envío
                    </Button>
                  </>
                )}
                {i.status === "conflict" && (
                  <>
                    <Badge tone="danger">Conflicto</Badge>
                    <Button size="sm" onClick={() => setOpen(i)}>
                      Resolver
                    </Button>
                  </>
                )}
              </li>
            ))}
          </ul>
        </Card>
      )}
      {open && (
        <ConflictDialog
          open
          mine={(open.body ?? {}) as Record<string, unknown>}
          theirs={(open.current ?? {}) as Record<string, unknown>}
          onClose={() => setOpen(null)}
          onResolve={async (r) => {
            await q.resolve(open.id, r);
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}
