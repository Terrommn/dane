import { describe, expect, it } from "vitest";
import {
  applyPatch,
  buildPatch,
  type CheckinForm,
  draftToLocal,
  emptyForm,
  formFromFull,
  isDirty,
  serverAsPatch,
} from "./checkin";
import { normalizeDecimal, parsePesosToCents, parseQuantity, parseWeight } from "./numbers";

const full = {
  version: 3,
  checkin: {
    id: "c1",
    on_date: "2026-11-12",
    weight_kg: "72.40",
    energy: 4,
    planned_spend_text: "Comida",
    planned_spend_cents: 25050,
    planned_spend_none: false,
    notes_md: "",
    morning_done_at: "2026-11-12T14:00:00Z",
    evening_done_at: null,
    version: 3,
    updated_at: "2026-11-12T14:00:00Z",
  },
  log: [
    { id: "l2", on_date: "2026-11-12", text: "Segundo", position: 1, source: "manual" },
    { id: "l1", on_date: "2026-11-12", text: "Primero", position: 0, source: "manual" },
  ],
  went_well: [{ id: "g1", on_date: "2026-11-12", polarity: "good", text: "Fui al gym", position: 0 }],
  went_badly: [],
  habit_entries: [
    { habit_id: "h1", on_date: "2026-11-12", status: "done", source: "checkin", version: 1, updated_at: "" },
    {
      habit_id: "h2",
      on_date: "2026-11-12",
      status: "not_applicable",
      reason: "Viaje",
      source: "checkin",
      version: 1,
      updated_at: "",
    },
  ],
};

describe("formFromFull", () => {
  it("convierte el estado del servidor sin inventar datos", () => {
    const f = formFromFull(full);
    expect(f.weight_kg).toBe("72.4");
    expect(f.energy).toBe(4);
    expect(f.planned_spend_amount).toBe("250.50");
    expect(f.log).toEqual(["Primero", "Segundo"]);
    expect(f.morning_done).toBe(true);
    expect(f.evening_done).toBe(false);
    expect(f.habits.h1?.status).toBe("done");
    expect(f.habits.h2).toEqual({ status: "not_applicable", reason: "Viaje", quantity: "" });
  });

  it("un día sin registro queda 'sin dato' (no 0 ni 'no')", () => {
    const f = formFromFull({
      version: 0,
      checkin: null,
      log: [],
      went_well: [],
      went_badly: [],
      habit_entries: [],
    });
    expect(f).toEqual(emptyForm());
    expect(f.energy).toBeNull();
    expect(f.weight_kg).toBe("");
  });
});

describe("buildPatch (guardado parcial)", () => {
  const base = formFromFull(full);

  it("sin cambios → parche vacío con la versión", () => {
    const r = buildPatch(base, base, 3);
    expect(r).toEqual({ ok: true, patch: { version: 3 }, empty: true });
  });

  it("solo envía los campos que cambiaron", () => {
    const form: CheckinForm = { ...base, energy: 2, went_badly: ["Me desvelé"] };
    const r = buildPatch(form, base, 3);
    expect(r.ok && r.patch).toEqual({ version: 3, energy: 2, went_badly: ["Me desvelé"] });
  });

  it("borrar un valor envía null explícito (vuelve a 'sin dato')", () => {
    const r = buildPatch({ ...base, weight_kg: "", energy: null, planned_spend_amount: "" }, base, 3);
    expect(r.ok && r.patch).toEqual({ version: 3, weight_kg: null, energy: null, planned_spend_cents: null });
  });

  it("convierte el gasto planeado a centavos exactos y valida el peso", () => {
    const ok = buildPatch({ ...base, planned_spend_amount: "1,250.5", weight_kg: "71,95" }, base, 3);
    expect(ok.ok && ok.patch).toMatchObject({ planned_spend_cents: 125050, weight_kg: "71.95" });
    expect(buildPatch({ ...base, weight_kg: "0" }, base, 3).ok).toBe(false);
    expect(buildPatch({ ...base, weight_kg: "72.345" }, base, 3).ok).toBe(false);
    expect(buildPatch({ ...base, planned_spend_amount: "-5" }, base, 3).ok).toBe(false);
  });

  it("hábitos: solo los modificados; desmarcar envía status null; no aplica exige motivo", () => {
    const form = {
      ...base,
      habits: {
        ...base.habits,
        h1: { status: null, reason: "", quantity: "" },
        h3: { status: "done" as const, reason: "", quantity: "20" },
      },
    };
    const r = buildPatch(form, base, 3);
    expect(r.ok && r.patch.habits).toEqual([
      { habit_id: "h1", status: null, reason: null, quantity: null },
      { habit_id: "h3", status: "done", reason: null, quantity: "20" },
    ]);
    const bad = buildPatch(
      { ...base, habits: { ...base.habits, h1: { status: "not_applicable", reason: " ", quantity: "" } } },
      base,
      3,
    );
    expect(bad.ok).toBe(false);
  });

  it("las listas ignoran viñetas vacías y espacios", () => {
    const r = buildPatch({ ...base, log: ["Primero ", "", "Segundo"] }, base, 3);
    expect(r.ok && r.empty).toBe(true);
  });

  it("energía fuera de 1–5 se rechaza", () => {
    expect(buildPatch({ ...base, energy: 6 }, base, 3).ok).toBe(false);
  });
});

describe("borradores offline", () => {
  const server = formFromFull(full);

  it("applyPatch reconstruye los cambios pendientes sobre el servidor", () => {
    const f = applyPatch(server, {
      energy: null,
      weight_kg: "70.1",
      habits: [{ habit_id: "h1", status: "not_done" }],
      evening_done: true,
    });
    expect(f.energy).toBeNull();
    expect(f.weight_kg).toBe("70.1");
    expect(f.habits.h1?.status).toBe("not_done");
    expect(f.evening_done).toBe(true);
    expect(f.log).toEqual(server.log);
  });

  it("draftToLocal acepta {form, base} o un parche encolado", () => {
    const local = { form: { ...server, energy: 1 }, base: server };
    expect(draftToLocal(local, server)).toBe(local);
    const fromPatch = draftToLocal({ version: 3, energy: 5 }, server);
    expect(fromPatch.form.energy).toBe(5);
    expect(fromPatch.base).toBe(server);
    expect(isDirty(fromPatch)).toBe(true);
    expect(isDirty({ form: server, base: server })).toBe(false);
  });

  it("serverAsPatch permite comparar campo por campo en un conflicto", () => {
    const p = serverAsPatch(full);
    expect(p.energy).toBe(4);
    expect(p.planned_spend_cents).toBe(25050);
    expect(p.habits).toContainEqual({
      habit_id: "h2",
      status: "not_applicable",
      reason: "Viaje",
      quantity: null,
    });
  });
});

describe("números", () => {
  it("montos a centavos sin floats", () => {
    expect(parsePesosToCents("0.1")).toEqual({ ok: true, value: 10 });
    expect(parsePesosToCents("$1,250.50")).toEqual({ ok: true, value: 125050 });
    expect(parsePesosToCents("19.99")).toEqual({ ok: true, value: 1999 });
    expect(parsePesosToCents("")).toEqual({ ok: true, value: null });
    expect(parsePesosToCents("1.999").ok).toBe(false);
    expect(parsePesosToCents("abc").ok).toBe(false);
  });
  it("peso y cantidades", () => {
    expect(parseWeight("72,40")).toEqual({ ok: true, value: "72.4" });
    expect(parseWeight("")).toEqual({ ok: true, value: null });
    expect(parseWeight("600").ok).toBe(false);
    expect(parseQuantity("20")).toEqual({ ok: true, value: "20" });
    expect(parseQuantity("x").ok).toBe(false);
    expect(normalizeDecimal("072.400")).toBe("72.4");
    expect(normalizeDecimal("5.0")).toBe("5");
  });
});
