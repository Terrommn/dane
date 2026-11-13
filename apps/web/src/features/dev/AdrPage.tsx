import { useState } from "react";
import { useParams } from "react-router";
import { Button, ButtonLink } from "../../components/ui/Button";
import { Badge, Card, PageHeader } from "../../components/ui/Display";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { ApiError, errorMessage } from "../../lib/errors";
import { ConflictPrompt, LinksPanel, SafeMarkdown, useAreasAndProjects } from "../notes/common";
import { ADR_STATUS, type Adr, type AdrBody, adrCode, useAdr, useAdrMutations, useAdrs } from "./devApi";

function adrBody(a: Adr): AdrBody {
  return {
    project_id: a.project_id,
    title: a.title,
    decided_on: a.decided_on,
    context_md: a.context_md,
    decision_md: a.decision_md,
    alternatives_md: a.alternatives_md,
    consequences_md: a.consequences_md,
    status: a.status,
    superseded_by_id: a.superseded_by_id ?? null,
    version: a.version,
  };
}

const SECTIONS = [
  ["context_md", "Contexto"],
  ["decision_md", "Decisión"],
  ["alternatives_md", "Alternativas consideradas"],
  ["consequences_md", "Consecuencias"],
] as const;

function AdrEditor({ adr }: { adr: Adr }) {
  const { update } = useAdrMutations();
  const { projects } = useAreasAndProjects();
  const siblings = useAdrs(adr.project_id);
  const toast = useToast();
  const [f, setF] = useState(() => adrBody(adr));
  const [edit, setEdit] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<Adr | null>(null);
  const save = async (data: AdrBody, version: number) => {
    if (data.status === "superseded" && !data.superseded_by_id)
      return setError("Elige el ADR que lo reemplaza");
    setError(null);
    try {
      const saved = await update.mutateAsync({ id: adr.id, body: { ...data, version } });
      setF(adrBody(saved));
      setEdit(false);
      toast.show("ADR guardado", { tone: "ok" });
    } catch (e) {
      if (e instanceof ApiError && e.isConflict && e.current) setConflict(e.current as Adr);
      else setError(errorMessage(e));
    }
  };
  const replacement = siblings.data?.find((s) => s.id === adr.superseded_by_id);
  return (
    <div className="space-y-4">
      <PageHeader
        title={`${adrCode(adr.number)} ${adr.title}`}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            {projects.find((p) => p.id === adr.project_id)?.name}
            <Badge tone={adr.status === "accepted" ? "ok" : adr.status === "superseded" ? "warn" : "muted"}>
              {ADR_STATUS[adr.status]}
            </Badge>
            {replacement && <span>Reemplazado por {adrCode(replacement.number)}</span>}
          </span>
        }
        actions={
          <>
            <Button variant="secondary" onClick={() => setEdit((x) => !x)}>
              {edit ? "Ver" : "Editar"}
            </Button>
            <ButtonLink to="/dev?tab=adrs">Volver</ButtonLink>
          </>
        }
      />
      {edit ? (
        <Card>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              void save(f, adr.version);
            }}
          >
            <div className="grid gap-3 sm:grid-cols-3">
              <TextField
                label="Título"
                value={f.title}
                onChange={(e) => setF({ ...f, title: e.target.value })}
              />
              <TextField
                label="Fecha"
                type="date"
                value={f.decided_on ?? ""}
                onChange={(e) => setF({ ...f, decided_on: e.target.value })}
              />
              <SelectField
                label="Estado"
                value={f.status ?? "proposed"}
                onChange={(e) => setF({ ...f, status: e.target.value })}
              >
                {Object.entries(ADR_STATUS).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v}
                  </option>
                ))}
              </SelectField>
            </div>
            {f.status === "superseded" && (
              <SelectField
                label="Reemplazado por"
                value={f.superseded_by_id ?? ""}
                onChange={(e) => setF({ ...f, superseded_by_id: e.target.value || null })}
                error={error ?? undefined}
              >
                <option value="">Elige…</option>
                {(siblings.data ?? [])
                  .filter((s) => s.id !== adr.id)
                  .map((s) => (
                    <option key={s.id} value={s.id}>
                      {adrCode(s.number)} {s.title}
                    </option>
                  ))}
              </SelectField>
            )}
            {SECTIONS.map(([k, label]) => (
              <TextArea
                key={k}
                label={label}
                rows={5}
                value={f[k] ?? ""}
                onChange={(e) => setF({ ...f, [k]: e.target.value })}
              />
            ))}
            {error && f.status !== "superseded" && <p className="text-sm text-danger">{error}</p>}
            <div className="flex justify-end">
              <Button type="submit" loading={update.isPending}>
                Guardar
              </Button>
            </div>
          </form>
        </Card>
      ) : (
        SECTIONS.map(([k, label]) => (
          <Card key={k} title={label}>
            {adr[k] ? <SafeMarkdown>{adr[k]}</SafeMarkdown> : <p className="text-muted">—</p>}
          </Card>
        ))
      )}
      <Card>
        <LinksPanel type="adr" id={adr.id} />
      </Card>
      <ConflictPrompt
        conflict={conflict}
        mine={f as unknown as Record<string, unknown>}
        onClose={() => setConflict(null)}
        onTakeServer={(srv) => setF(adrBody(srv))}
        onSave={(data, v) => void save(data as unknown as AdrBody, v)}
      />
    </div>
  );
}

export function AdrPage() {
  const { id = "" } = useParams();
  const q = useAdr(id);
  return <QueryState query={q}>{(a) => <AdrEditor key={`${a.id}:${a.version}`} adr={a} />}</QueryState>;
}
