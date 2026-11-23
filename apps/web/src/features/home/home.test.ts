import { describe, expect, it } from "vitest";
import { dayBrief, greeting, localHour, recordStreak } from "./HomePage";

describe("Inicio", () => {
  it("saluda según la hora local del usuario, no la del navegador", () => {
    // 13:30 UTC = 07:30 en Monterrey (−06:00) y 14:30 en Madrid (+01:00).
    const now = new Date("2026-11-23T13:30:00Z");
    expect(greeting(localHour("America/Monterrey", now))).toBe("Buenos días");
    expect(greeting(localHour("Europe/Madrid", now))).toBe("Buenas tardes");
    expect(greeting(21)).toBe("Buenas noches");
  });

  it("la racha cuenta días seguidos con registro sin incluir hoy", () => {
    const day = (date: string, done: number, energy: number | null = null) => ({
      date,
      done,
      required: 2,
      ratio: done / 2,
      unrecorded: 0,
      energy,
      route: "",
    });
    const days = [
      day("2026-11-19", 1),
      day("2026-11-20", 0),
      day("2026-11-21", 0, 3),
      day("2026-11-22", 2),
      day("2026-11-23", 0),
    ];
    expect(recordStreak(days, "2026-11-23")).toBe(2);
    expect(recordStreak([], "2026-11-23")).toBe(0);
  });

  it("resume el día en una frase legible", () => {
    expect(
      dayBrief({
        goalTitle: "Volver a mi prime",
        dayOfMonth: 23,
        habits: 3,
        tasks: 1,
        events: 2,
        nextEvent: { title: "Junta con cliente", time: "11:00" },
      }),
    ).toBe("Día 23 de «Volver a mi prime». Hoy tienes 3 hábitos, 1 tarea y Junta con cliente a las 11:00.");
    expect(dayBrief({ dayOfMonth: 5, habits: 0, tasks: 0, events: 0 })).toBe(
      "Hoy no tienes nada agendado: buen día para avanzar en lo importante.",
    );
    expect(dayBrief({ dayOfMonth: 5, habits: 1, tasks: 0, events: 0 })).toBe("Hoy tienes 1 hábito.");
  });
});
