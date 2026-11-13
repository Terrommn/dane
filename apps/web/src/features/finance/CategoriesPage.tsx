// Catálogo editable de categorías de gasto e ingreso. Renombrar o archivar conserva el ID y el historial.
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { CheckboxField, SelectField, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import type { Schemas } from "../../lib/api";
import { ApiError, errorMessage } from "../../lib/errors";
import { FinanceNav, useVersionedSave } from "./components";
import { type Category, useCategories, useCategoryMutations } from "./financeApi";
import { EXPENSE_ROLES, INCOME_ROLES, ROLE_LABEL } from "./labels";

export function CategoriesPage() {
  const [archived, setArchived] = useState(false);
  const q = useCategories(archived);
  const [editing, setEditing] = useState<Category | "expense" | "income" | null>(null);
  return (
    <div>
      <PageHeader
        title="Categorías"
        subtitle="Catálogo de gastos e ingresos"
        actions={
          <>
            <Button variant="secondary" onClick={() => setEditing("income")}>
              + Ingreso
            </Button>
            <Button onClick={() => setEditing("expense")}>+ Gasto</Button>
          </>
        }
      />
      <FinanceNav />
      <CheckboxField
        className="mb-3"
        label="Mostrar archivadas"
        checked={archived}
        onChange={(e) => setArchived(e.target.checked)}
      />
      <QueryState query={q}>
        {(list) => (
          <div className="grid gap-4 md:grid-cols-2">
            {(["expense", "income"] as const).map((k) => (
              <Card key={k} title={k === "expense" ? "Gastos" : "Ingresos"}>
                <ul className="divide-y divide-line">
                  {list
                    .filter((c) => c.kind === k)
                    .map((c) => (
                      <li key={c.id} className="flex items-center justify-between gap-2 py-2">
                        <span className="min-w-0">
                          <span className="font-medium">{c.name}</span>{" "}
                          <Badge>{ROLE_LABEL[c.role] ?? c.role}</Badge>{" "}
                          {c.archived_at && <Badge tone="warn">Archivada</Badge>}
                        </span>
                        <Button size="sm" variant="secondary" onClick={() => setEditing(c)}>
                          Editar
                        </Button>
                      </li>
                    ))}
                </ul>
              </Card>
            ))}
          </div>
        )}
      </QueryState>
      {editing && (
        <CategoryDialog
          category={typeof editing === "string" ? null : editing}
          kind={typeof editing === "string" ? editing : (editing.kind as "expense" | "income")}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}

function CategoryDialog({
  category,
  kind,
  onClose,
}: {
  category: Category | null;
  kind: "expense" | "income";
  onClose: () => void;
}) {
  const { create, update } = useCategoryMutations();
  const toast = useToast();
  const roles = kind === "expense" ? EXPENSE_ROLES : INCOME_ROLES;
  const [name, setName] = useState(category?.name ?? "");
  const [role, setRole] = useState<string>(category?.role ?? roles[0]);
  const [archived, setArchived] = useState(category?.archived_at != null);
  const [error, setError] = useState<string | null>(null);
  const versioned = useVersionedSave<Schemas["CategoryBody"]>({
    save: (body) => update.mutateAsync({ id: category?.id ?? "", body }),
    labels: { name: "Nombre", role: "Tipo", archived: "Archivada" },
    toBody: (cur) => {
      const c = cur as Category;
      return {
        name: c.name,
        kind: c.kind,
        role: c.role,
        archived: c.archived_at != null,
        position: c.position,
        version: c.version,
      };
    },
    onSaved: () => {
      toast.show("Categoría guardada", { tone: "ok" });
      onClose();
    },
    onError: (e) => setError(errorMessage(e)),
  });
  const submit = async () => {
    if (!name.trim()) return setError("Ponle un nombre");
    const body: Schemas["CategoryBody"] = {
      name: name.trim(),
      kind,
      role,
      archived,
      position: category?.position ?? 0,
    };
    try {
      if (category) await versioned.run({ ...body, version: category.version });
      else {
        await create.mutateAsync(body);
        toast.show("Categoría creada", { tone: "ok" });
        onClose();
      }
    } catch (e) {
      setError(
        e instanceof ApiError && e.code === "duplicate"
          ? "Ya existe una categoría con ese nombre"
          : errorMessage(e),
      );
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={
        category
          ? `Editar ${category.name}`
          : `Nueva categoría de ${kind === "expense" ? "gasto" : "ingreso"}`
      }
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={submit} loading={create.isPending || update.isPending}>
            Guardar
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
        <SelectField
          label="Cómo cuenta"
          value={role}
          onChange={(e) => setRole(e.target.value)}
          hint="Define en qué total del dashboard aparece."
        >
          {roles.map((r) => (
            <option key={r} value={r}>
              {ROLE_LABEL[r]}
            </option>
          ))}
        </SelectField>
        {category && (
          <CheckboxField
            label="Archivada"
            hint="Deja de ofrecerse en formularios; los movimientos históricos la conservan."
            checked={archived}
            onChange={(e) => setArchived(e.target.checked)}
          />
        )}
      </div>
      {versioned.dialog}
    </Dialog>
  );
}
