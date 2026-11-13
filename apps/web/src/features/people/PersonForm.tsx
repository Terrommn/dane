import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { SelectField, TextArea, TextField } from "../../components/ui/Field";
import { ListField, useAreasAndProjects } from "../notes/common";
import { type PersonBody, RELATIONSHIPS, validBirthday } from "./peopleApi";

export type PersonFormValue = Omit<PersonBody, "version" | "archived">;

export function emptyPerson(name = ""): PersonFormValue {
  return {
    name,
    aliases: [],
    area_id: null,
    relationship: null,
    org: null,
    role: null,
    contact: null,
    email: null,
    context_md: "",
    birthday: null,
  };
}

export function PersonForm({
  initial,
  submitLabel,
  loading,
  serverError,
  onSubmit,
}: {
  initial: PersonFormValue;
  submitLabel: string;
  loading?: boolean;
  serverError?: string | null;
  onSubmit: (v: PersonFormValue) => void;
}) {
  const { areas } = useAreasAndProjects();
  const [v, setV] = useState(initial);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const set = <K extends keyof PersonFormValue>(k: K, val: PersonFormValue[K]) =>
    setV((x) => ({ ...x, [k]: val }));
  const opt = (s: string) => (s.trim() ? s.trim() : null);
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        const errs: Record<string, string> = {};
        if (!v.name.trim()) errs.name = "El nombre es obligatorio";
        if (v.birthday && !validBirthday(v.birthday)) errs.birthday = "Usa AAAA-MM-DD o MM-DD (sin año)";
        if (v.email && !/^\S+@\S+\.\S+$/.test(v.email)) errs.email = "Correo inválido";
        setErrors(errs);
        if (!Object.keys(errs).length) onSubmit({ ...v, name: v.name.trim() });
      }}
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <TextField
          label="Nombre"
          value={v.name}
          onChange={(e) => set("name", e.target.value)}
          error={errors.name}
        />
        <ListField
          label="Alias / apodos"
          value={v.aliases ?? []}
          onChange={(a) => set("aliases", a)}
          hint="Ej.: Juancho, Lalo"
        />
        <SelectField
          label="Área"
          value={v.area_id ?? ""}
          onChange={(e) => set("area_id", e.target.value || null)}
        >
          <option value="">Sin área</option>
          {areas.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </SelectField>
        <SelectField
          label="Relación"
          value={v.relationship ?? ""}
          onChange={(e) => set("relationship", e.target.value || null)}
        >
          <option value="">—</option>
          {RELATIONSHIPS.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </SelectField>
        <TextField
          label="Organización"
          value={v.org ?? ""}
          onChange={(e) => set("org", opt(e.target.value))}
        />
        <TextField label="Rol" value={v.role ?? ""} onChange={(e) => set("role", opt(e.target.value))} />
        <TextField
          label="Contacto"
          value={v.contact ?? ""}
          onChange={(e) => set("contact", opt(e.target.value))}
          hint="Teléfono, @usuario…"
        />
        <TextField
          label="Correo"
          type="email"
          value={v.email ?? ""}
          onChange={(e) => set("email", opt(e.target.value))}
          error={errors.email}
        />
        <TextField
          label="Cumpleaños"
          value={v.birthday ?? ""}
          onChange={(e) => set("birthday", opt(e.target.value))}
          placeholder="1990-04-23 o 04-23"
          hint="Con año (AAAA-MM-DD) o sin año (MM-DD)."
          error={errors.birthday}
        />
      </div>
      <TextArea
        label="Contexto (quién es para ti)"
        rows={3}
        value={v.context_md ?? ""}
        onChange={(e) => set("context_md", e.target.value)}
      />
      {serverError && (
        <p role="alert" className="text-sm text-danger">
          {serverError}
        </p>
      )}
      <div className="flex justify-end">
        <Button type="submit" loading={loading}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
