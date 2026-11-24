// Contrato en vivo contra un servidor real (opcional): DANE_LIVE_API=http://127.0.0.1:8097 pnpm -C apps/web exec vitest run core.live
// Usa las mismas funciones que la UI para armar los cuerpos (buildPatch, valuesToBody, fixedBody, cleanFrequency…)
// y verifica que el servidor los acepta y que los datos regresan iguales (ida y vuelta).
// Requiere DEV_LOGIN_ENABLED=true y el ID 111 en ALLOWED_TELEGRAM_IDS.
import { describe, expect, it } from "vitest";
import { todayIso } from "../lib/format";
import { valuesToBody } from "./goals/GoalForm";
import { asProgress, describeProgress } from "./goals/goalsApi";
import { isoWeekKey } from "./goals/periods";
import { defaultFixedRows, fixedBody } from "./habits/FixedHabitsSetup";
import { cleanFrequency } from "./habits/FrequencyFields";
import { buildPatch, formFromFull, serverAsPatch } from "./today/checkin";
import { formFromReview } from "./weekly/WeeklyPage";

const BASE = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env
  .DANE_LIVE_API;
const live = BASE ? describe : describe.skip;

live("contrato en vivo de la UI núcleo", () => {
  let token = "";
  const call = async (method: string, path: string, body?: unknown) => {
    const res = await fetch(`${BASE}/api/v1${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
        ...(method === "GET" ? {} : { "Idempotency-Key": crypto.randomUUID() }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    return { status: res.status, body: text ? JSON.parse(text) : null };
  };
  const today = todayIso("America/Monterrey");

  it("flujo completo: hábitos fijos → check-in → objetivo → lectura → dashboard → semanal", async () => {
    const login = await fetch(`${BASE}/api/v1/auth/dev-login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ telegram_id: 111, transport: "bearer" }),
    });
    token = (await login.json()).token;
    expect(token).toBeTruthy();

    // Onboarding: solo los marcados (ejercicio 4/semana y lectura diaria).
    const rows = defaultFixedRows();
    const fixed = fixedBody({
      ...rows,
      exercise: { ...rows.exercise, enabled: true },
      reading: { ...rows.reading, enabled: true },
    });
    const created = await call("POST", "/habits/fixed", fixed);
    expect(created.status).toBe(200);
    const exercise = created.body.find((h: { fixed_key: string }) => h.fixed_key === "exercise");
    expect(exercise).toBeTruthy();
    expect(created.body.some((h: { fixed_key: string }) => h.fixed_key === "reading")).toBe(false);

    // Hoy: guardado parcial e ida y vuelta.
    const view = await call("GET", `/checkins/${today}/view`);
    expect(view.status).toBe(200);
    expect(view.body.habits.fixed.length).toBe(2);
    const base = formFromFull(view.body);
    expect(base.energy).toBeNull();
    const form = {
      ...base,
      energy: 4,
      weight_kg: "72,4",
      went_well: ["Fui al gym"],
      planned_spend_amount: "250.50",
      habits: { ...base.habits, [exercise.id]: { status: "done" as const, reason: "", quantity: "" } },
    };
    const r = buildPatch(form, base, view.body.version);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const put = await call("PUT", `/checkins/${today}`, r.patch);
    expect(put.status).toBe(200);
    expect(put.body.version).toBeGreaterThan(view.body.version);
    const again = formFromFull((await call("GET", `/checkins/${today}/view`)).body);
    expect(again.energy).toBe(4);
    expect(again.weight_kg).toBe("72.4");
    expect(again.planned_spend_amount).toBe("250.50");
    expect(again.went_well).toEqual(["Fui al gym"]);
    expect(again.habits[exercise.id]?.status).toBe("done");

    // Versión vieja → 409 con el estado actual comparable.
    const stale = await call("PUT", `/checkins/${today}`, { version: 0, energy: 1 });
    expect(stale.status).toBe(409);
    expect(serverAsPatch(stale.body.error.current).energy).toBe(4);

    // Objetivo acumulativo + hábito del objetivo + planificación con avisos.
    const g = valuesToBody({
      kind: "month",
      period_key: today.slice(0, 7),
      title: "Leer 4 libros",
      area_id: "",
      motivation: "Aprender",
      success_criteria: "4 terminados",
      parent_goal_id: "",
      progress_type: "cumulative",
      unit: "libros",
      baseline: "",
      target: "4",
      direction: "",
    });
    expect(g.ok).toBe(true);
    if (!g.ok) return;
    const goal = await call("POST", "/goals", g.body);
    expect(goal.status).toBe(201);
    const goalId = goal.body.goal.id;
    const habit = await call("POST", "/habits", {
      ...cleanFrequency({ frequency: "daily", target_quantity: "20" }),
      name: "Leer 20 min",
      kind: "goal",
      goal_id: goalId,
      unit: "min",
      description: "",
      starts_on: null,
    });
    expect(habit.status).toBe(201);
    const plan = await call("GET", `/goals/planning?week=${isoWeekKey(today)}`);
    expect(plan.status).toBe(200);
    expect(Array.isArray(plan.body.warnings)).toBe(true);
    expect(plan.body.month_habit_count).toBeGreaterThanOrEqual(1);

    // Progreso manual + lectura terminada enlazada: aporta una vez.
    expect(
      (await call("POST", `/goals/${goalId}/progress`, { occurred_on: today, amount: "1", note: "" })).status,
    ).toBe(201);
    const reading = await call("POST", "/readings", {
      kind: "book",
      title: "Hábitos atómicos",
      author: "J. Clear",
      status: "finished",
      finished_on: today,
      goal_ids: [goalId],
    });
    expect(reading.status).toBe(201);
    const detail = (await call("GET", `/goals/${goalId}`)).body;
    const p = asProgress(detail.progress);
    expect(describeProgress(detail.goal, p)).toBe("2 de 4 libros");
    expect(detail.habits[0].unit).toBe("min"); // la unidad del hábito no se mezcla con la del objetivo

    // Vínculo explícito y dashboard.
    expect((await call("PUT", "/habit-links/weight_logged", { habit_id: exercise.id })).status).toBe(200);
    const dash = await call("GET", `/dashboard?date=${today}&compare=same_span`);
    expect(dash.status).toBe(200);
    expect(dash.body.weight.last.kg).toBeCloseTo(72.4);
    expect(dash.body.week_comparison.mode).toBe("same_span");

    // Semanal: guardar con paz y prioridades.
    const week = isoWeekKey(today);
    const rv = await call("GET", `/weekly-reviews/${week}`);
    const f = formFromReview(rv.body.review);
    const saved = await call("PUT", `/weekly-reviews/${week}`, {
      what_happened: "Semana enfocada",
      peace: 4,
      priorities: [{ text: "Terminar el libro", reason: "Meta del mes" }],
      completed: f.completed,
      version: rv.body.version,
    });
    expect(saved.status).toBe(200);
    expect((await call("GET", `/weekly-reviews/${week}/summary`)).status).toBe(200);

    const history = await call("GET", `/habits/history?from=${today}&to=${today}`);
    expect(history.status).toBe(200);
  });
});
