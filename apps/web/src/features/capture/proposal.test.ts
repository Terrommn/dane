import { describe, expect, it } from "vitest";
import { appliedLinks } from "./applied";
import {
  cleanProposal,
  describeProposal,
  emptyProposal,
  isProposalEmpty,
  isValidAmount,
  localWarnings,
  parseProposal,
  parseQuestionField,
} from "./proposal";

describe("parseProposal", () => {
  it("completa campos ausentes sin inventar valores", () => {
    const p = parseProposal({ expenses: [{ concept: "Gasolina" }], checkin: { energy: 4 } });
    expect(p.expenses[0]).toEqual({
      date: null,
      concept: "Gasolina",
      amount: null,
      category: null,
      payment: null,
    });
    expect(p.checkin).toEqual({
      weight_kg: null,
      energy: 4,
      exercise: null,
      reading: null,
      planned_spend: null,
    });
    expect(p.tasks).toEqual([]);
    expect(p.devlog).toBeNull();
  });

  it("tolera basura (objeto vacío, tipos erróneos)", () => {
    expect(parseProposal(null)).toEqual(emptyProposal());
    const p = parseProposal({ tasks: "x", log: [1, "ok"], goals: [{ title: "Leer", kind: "otro" }] });
    expect(p.tasks).toEqual([]);
    expect(p.log).toEqual(["ok"]);
    expect(p.goals[0]?.kind).toBe("month");
  });
});

describe("cleanProposal", () => {
  it("descarta secciones que el intent no permite y filas vacías", () => {
    const p = emptyProposal();
    p.expenses = [
      { date: "2026-11-12", concept: " Gasolina ", amount: " 250 ", category: "Gasolina", payment: "" },
      { date: null, concept: "  ", amount: null, category: null, payment: null },
    ];
    p.tasks = [{ title: "Llamar al dentista", area: null, due: null, people: [], clickup_candidate: false }];
    const c = cleanProposal(p, ["expenses"]);
    expect(c.expenses).toEqual([
      { date: "2026-11-12", concept: "Gasolina", amount: "250", category: "Gasolina", payment: null },
    ]);
    expect(c.tasks).toEqual([]);
  });

  it("un check-in sin datos no se envía (sin dato ≠ cero ni «no»)", () => {
    const p = emptyProposal();
    p.checkin = { weight_kg: " ", energy: null, exercise: null, reading: null, planned_spend: null };
    expect(cleanProposal(p, ["checkin"]).checkin).toBeNull();
    p.checkin = { weight_kg: null, energy: null, exercise: false, reading: null, planned_spend: null };
    expect(cleanProposal(p, ["checkin"]).checkin?.exercise).toBe(false);
  });

  it("limpia listas de texto y personas de tareas", () => {
    const p = emptyProposal();
    p.log = [" Fui al gym ", ""];
    p.tasks = [{ title: "Reporte", area: "Trabajo", due: "", people: ["Ana", " "], clickup_candidate: true }];
    const c = cleanProposal(p, ["log", "tasks"]);
    expect(c.log).toEqual(["Fui al gym"]);
    expect(c.tasks[0]).toEqual({
      title: "Reporte",
      area: "Trabajo",
      due: null,
      people: ["Ana"],
      clickup_candidate: true,
    });
  });
});

describe("montos", () => {
  it("acepta formatos es-MX y rechaza ambiguos", () => {
    for (const ok of ["180", "180.5", "180.50", "$1,250", "$ 1,250.00", "15000"])
      expect(isValidAmount(ok)).toBe(true);
    for (const bad of ["", "0", "-5", "1.999", "abc", "1,25", null]) expect(isValidAmount(bad)).toBe(false);
  });

  it("avisa de montos inválidos y pesos fuera de rango", () => {
    const p = emptyProposal();
    p.expenses = [{ date: null, concept: "Cena", amount: "1.999", category: null, payment: null }];
    p.checkin = { weight_kg: "700", energy: null, exercise: null, reading: null, planned_spend: null };
    const w = localWarnings(p);
    expect(w).toHaveLength(2);
    expect(w[0]).toContain("Gasto 1");
  });
});

describe("resumen y preguntas", () => {
  it("describe lo que se creará", () => {
    const p = emptyProposal();
    expect(isProposalEmpty(p)).toBe(true);
    p.expenses = [
      { date: null, concept: "a", amount: "1", category: null, payment: null },
      { date: null, concept: "b", amount: "2", category: null, payment: null },
    ];
    p.reflections_good = ["Corrí"];
    expect(describeProposal(p)).toEqual(["2 gastos", "1 reflexión"]);
  });

  it("interpreta rutas de campo del servidor", () => {
    expect(parseQuestionField("expenses[1].amount")).toEqual({
      section: "expenses",
      index: 1,
      field: "amount",
    });
    expect(parseQuestionField("summary")).toEqual({ section: "summary", index: null, field: null });
  });

  it("convierte lo aplicado en enlaces agrupados", () => {
    const links = appliedLinks({
      created: [
        { type: "transaction", id: "1" },
        { type: "transaction", id: "2" },
        { type: "checkin", date: "2026-11-12" },
        { type: "weekly_review", iso_week: "2026-W46" },
      ],
    });
    expect(links).toEqual([
      { label: "2 × Movimiento", to: "/finanzas/movimientos" },
      { label: "Check-in", to: "/hoy?date=2026-11-12" },
      { label: "Revisión semanal", to: "/semanal?week=2026-W46" },
    ]);
    expect(appliedLinks(null)).toEqual([]);
  });
});
