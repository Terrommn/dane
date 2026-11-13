// Editor de áreas: renombrar (conserva el ID), agregar, archivar y recuperar.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Badge } from "../../components/ui/Display";
import { CheckboxField, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { type Area, useAreaMutations, useAreas } from "./projectsApi";

function AreaRow({ area }: { area: Area }) {
  const [name, setName] = useState(area.name);
  const { update } = useAreaMutations();
  const toast = useToast();
  const archived = area.archived_at != null;
  const run = async (patch: { name?: string; archived?: boolean }) => {
    try {
      await update.mutateAsync({ area, patch });
      toast.show(
        patch.archived === undefined
          ? "Área renombrada"
          : patch.archived
            ? "Área archivada"
            : "Área recuperada",
        { tone: "ok" },
      );
    } catch (e) {
      toast.show(
        e instanceof ApiError && e.isConflict
          ? "El área cambió en otro dispositivo; se recargó la versión actual."
          : errorMessage(e),
        { tone: "error" },
      );
      setName(area.name);
    }
  };
  return (
    <li className="flex flex-wrap items-end gap-2 py-2">
      <form
        className="flex min-w-0 flex-1 items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim() && name !== area.name) void run({ name: name.trim() });
        }}
      >
        <TextField
          className="min-w-0 flex-1"
          label={`Nombre del área ${area.slug}`}
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={archived}
        />
        <Button
          type="submit"
          variant="secondary"
          size="sm"
          disabled={archived || !name.trim() || name === area.name}
          loading={update.isPending}
        >
          Renombrar
        </Button>
      </form>
      {archived && <Badge>Archivada</Badge>}
      <Button variant="ghost" size="sm" onClick={() => void run({ archived: !archived })}>
        {archived ? "Recuperar" : "Archivar"}
      </Button>
    </li>
  );
}

export function AreasEditor() {
  const [showArchived, setShowArchived] = useState(false);
  const areas = useAreas(showArchived);
  const { create } = useAreaMutations();
  const toast = useToast();
  const [newName, setNewName] = useState("");
  return (
    <div className="space-y-3">
      <CheckboxField
        label="Mostrar archivadas"
        checked={showArchived}
        onChange={(e) => setShowArchived(e.target.checked)}
      />
      <QueryState query={areas}>
        {(list) => (
          <ul className="divide-y divide-line">
            {list.map((a) => (
              <AreaRow key={`${a.id}-${a.version}`} area={a} />
            ))}
          </ul>
        )}
      </QueryState>
      <form
        className="flex items-end gap-2"
        onSubmit={async (e) => {
          e.preventDefault();
          if (!newName.trim()) return;
          try {
            await create.mutateAsync({ name: newName.trim(), position: (areas.data?.length ?? 0) + 1 });
            setNewName("");
            toast.show("Área agregada", { tone: "ok" });
          } catch (err) {
            toast.show(errorMessage(err), { tone: "error" });
          }
        }}
      >
        <TextField
          className="min-w-0 flex-1"
          label="Nueva área"
          placeholder="Ej. Salud"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
        />
        <Button type="submit" loading={create.isPending} disabled={!newName.trim()}>
          Agregar
        </Button>
      </form>
    </div>
  );
}
