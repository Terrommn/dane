import { describe, expect, it } from "vitest";
import { defaultFixedRows, fixedBody } from "../habits/FixedHabitsSetup";
import { cleanFrequency, validateFrequency } from "../habits/FrequencyFields";
import { complianceText, streakText } from "../habits/HabitsPage";
import { describeFrequency } from "../habits/habitsApi";
import { byRating } from "../readings/ReadingsPage";
import { paceText } from "./GoalCard";
import { type GoalFormValues, valuesToBody } from "./GoalForm";
import { describeProgress, type Goal, type GoalProgress } from "./goalsApi";
import {
  formatWeek,
  isoWeekKey,
  isoWeekStart,
  isValidWeekKey,
  monthEnd,
  shiftMonth,
  shiftWeek,
  weekKeyStart,
} from "./periods";

describe("periodos ISO", () => {
  it("año ISO distinto del civil", () => {
    expect(isoWeekKey("2026-12-31")).toBe("2026-W53");
    expect(isoWeekKey("2027-01-03")).toBe("2026-W53");
    expect(isoWeekKey("2027-01-04")).toBe("2027-W01");
    expect(isoWeekKey("2024-12-30")).toBe("2025-W01");
    expect(weekKeyStart("2025-W01")).toBe("2024-12-30");
  });
  it("semanas que cruzan mes y navegación", () => {
    expect(isoWeekStart("2026-12-02")).toBe("2026-11-30");
    expect(shiftWeek("2026-W53", 1)).toBe("2027-W01");
    expect(shiftWeek("2027-W01", -1)).toBe("2026-W53");
    expect(isValidWeekKey("2026-W54")).toBe(false);
    expect(isValidWeekKey("2025-W53")).toBe(false);
    expect(formatWeek("2026-W46")).toContain("Semana 46");
  });
  it("meses cortos y cambio de año", () => {
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(monthEnd("2028-02")).toBe("2028-02-29");
    expect(monthEnd("2026-02")).toBe("2026-02-28");
  });
});

const base: GoalFormValues = {
  kind: "month",
  period_key: "2026-11",
  title: "Leer 4 libros",
  area_id: "",
  motivation: "",
  success_criteria: "",
  parent_goal_id: "",
  progress_type: "cumulative",
  unit: "libros",
  baseline: "",
  target: "4",
  direction: "",
};

describe("formulario de objetivo", () => {
  it("acumulativo exige unidad y meta, dirección implícita 'subir'", () => {
    const r = valuesToBody(base);
    expect(r.ok && r.body).toMatchObject({
      unit: "libros",
      target: "4",
      direction: "increase",
      parent_goal_id: null,
    });
    expect(valuesToBody({ ...base, unit: "" }).ok).toBe(false);
    expect(valuesToBody({ ...base, target: "cuatro" }).ok).toBe(false);
  });
  it("valor actual exige dirección; sin medición no manda meta", () => {
    expect(valuesToBody({ ...base, progress_type: "current_value", unit: "kg", target: "70" }).ok).toBe(
      false,
    );
    const r = valuesToBody({ ...base, progress_type: "none", unit: "", target: "" });
    expect(r.ok && r.body).toMatchObject({ target: null, direction: null });
  });
  it("semanal: periodo válido y relación con el mes solo en semanas", () => {
    expect(valuesToBody({ ...base, kind: "week", period_key: "2026-11" }).ok).toBe(false);
    const r = valuesToBody({ ...base, kind: "week", period_key: "2026-W46", parent_goal_id: "m1" });
    expect(r.ok && r.body.parent_goal_id).toBe("m1");
    const m = valuesToBody({ ...base, parent_goal_id: "m1" });
    expect(m.ok && m.body.parent_goal_id).toBeNull();
  });
});

const goal = (p: Partial<Goal>): Goal =>
  ({
    id: "g",
    kind: "month",
    period_key: "2026-11",
    progress_type: "cumulative",
    unit: "libros",
    ...p,
  }) as Goal;
const progress = (p: Partial<GoalProgress>): GoalProgress => ({
  has_data: true,
  current: 1,
  target: 4,
  baseline: null,
  ratio: 0.25,
  remaining: 3,
  achieved: false,
  overachieved: false,
  target_zero: false,
  expected_by_today: 1.6,
  pace_delta: -0.6,
  projection_estimate: 2.5,
  last_measured_on: null,
  elapsed_days: 12,
  total_days: 30,
  ...p,
});

describe("progreso de objetivos", () => {
  it("describe acumulativo, valor actual e hito", () => {
    expect(describeProgress(goal({}), progress({}))).toBe("1 de 4 libros");
    expect(
      describeProgress(
        goal({ progress_type: "current_value", unit: "kg" }),
        progress({ current: 72.4, target: 70 }),
      ),
    ).toBe("72.4 kg · meta 70 kg");
    expect(
      describeProgress(
        goal({ progress_type: "current_value", unit: "kg" }),
        progress({ has_data: false, current: null, target: 70 }),
      ),
    ).toContain("Sin mediciones");
    expect(describeProgress(goal({ progress_type: "milestone" }), progress({ achieved: true }))).toBe(
      "Hito completado",
    );
  });
  it("ritmo: adelantado/atrasado; sin datos de ritmo no inventa texto", () => {
    expect(paceText(progress({}), "libros")).toContain("Atrasado 0.6 libros");
    expect(paceText(progress({ pace_delta: 1 }), "libros")).toContain("Adelantado 1 libros");
    expect(paceText(progress({ pace_delta: null, expected_by_today: null }), "kg")).toBeNull();
  });
});

describe("hábitos", () => {
  it("frecuencia legible y validación (4/semana no es diario)", () => {
    expect(describeFrequency({ frequency: "daily" })).toBe("Diario");
    expect(describeFrequency({ frequency: "times_per_week", times_per_week: 4 })).toBe("4 veces por semana");
    expect(describeFrequency({ frequency: "weekdays", weekdays: 1 | 4 | 16 })).toBe("Lun, Mié, Vie");
    expect(validateFrequency({ frequency: "weekdays", weekdays: 0 })).not.toBeNull();
    expect(validateFrequency({ frequency: "times_per_week", times_per_week: 8 })).not.toBeNull();
    expect(
      cleanFrequency({ frequency: "daily", weekdays: 3, times_per_week: 2, target_quantity: " 20 " }),
    ).toEqual({
      frequency: "daily",
      weekdays: null,
      times_per_week: null,
      target_quantity: "20",
    });
  });
  it("cumplimiento distingue sin registro de 'no'; sin exigibles lo dice", () => {
    const c = {
      required: 22,
      done: 19,
      missed: 1,
      unrecorded: 2,
      excluded_not_applicable: 1,
      paused_days: 0,
      extra: 0,
      rate: 19 / 22,
    };
    expect(complianceText(c)).toBe("86% (19/22) · 2 sin registro · 1 no · 1 no aplica");
    expect(complianceText({ ...c, required: 0, rate: null })).toBe("Sin días exigibles");
    expect(streakText({ current: 3, longest: 5, unit: "weeks" })).toBe("Racha 3 sem · máx. 5");
  });
  it("hábitos fijos: ejemplos desmarcados; solo se envían los marcados", () => {
    const rows = defaultFixedRows();
    expect(Object.values(rows).every((r) => !r.enabled)).toBe(true);
    expect(rows.exercise.freq).toEqual({ frequency: "times_per_week", times_per_week: 4 });
    expect(fixedBody(rows)).toEqual({ starts_on: null });
    const body = fixedBody({ ...rows, exercise: { ...rows.exercise, enabled: true } });
    expect(body).toEqual({
      starts_on: null,
      exercise: {
        frequency: "times_per_week",
        weekdays: null,
        times_per_week: 4,
        target_quantity: null,
        enabled: true,
        name: "Ejercicio",
      },
    });
    expect(Object.keys(rows)).toEqual(["exercise", "reading"]);
  });
});

describe("lecturas", () => {
  it("tabla ordenada por rating (sin rating al final)", () => {
    const r = (id: string, rating: number | null, fin: string | null) =>
      ({ id, rating, finished_on: fin }) as Parameters<typeof byRating>[0];
    const sorted = [
      r("a", null, null),
      r("b", 5, "2026-01-01"),
      r("c", 3, null),
      r("d", 5, "2026-02-01"),
    ].sort(byRating);
    expect(sorted.map((x) => x.id)).toEqual(["d", "b", "c", "a"]);
  });
});
