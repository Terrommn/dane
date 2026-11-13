// Pruebas de la fase 2 de módulos: lógica pura (hooks de contenido, cumpleaños, reglas, propuestas del Inbox,
// escenas de dibujo, agenda, búsqueda) y componentes de seguridad (Markdown sin HTML crudo).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { eventLine, localTime } from "./agenda/AgendaPage";
import { compactHooks, daysToPublishLabel, padHooks } from "./content/contentApi";
import { asScene, sceneFromExcalidraw, sceneSize } from "./drawings/scene";
import { actionFromProposal } from "./inbox/inboxApi";
import { entityHref, ListField, parseList, SafeMarkdown, safeUrl } from "./notes/common";
import { noteInput } from "./notes/notesApi";
import { birthdayString, validBirthday } from "./people/peopleApi";
import { searchQuery } from "./search/SearchPage";
import { ruleLabel, taskBody } from "./tasks/tasksApi";

afterEach(cleanup);

function ListHarness() {
  const [v, setV] = useState<string[]>([]);
  return (
    <>
      <ListField label="Tags" value={v} onChange={setV} />
      <output>{v.join("|")}</output>
    </>
  );
}

describe("Markdown seguro", () => {
  it("no renderiza HTML crudo ni enlaces javascript:", () => {
    const { container } = render(
      <SafeMarkdown>
        {
          "**hola** <script>alert(1)</script><img src=x onerror=alert(1)> [x](javascript:alert(1)) [ok](https://a.b)"
        }
      </SafeMarkdown>,
    );
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("strong")?.textContent).toBe("hola");
    const links = [...container.querySelectorAll("a")];
    expect(links.find((a) => a.textContent === "x")?.getAttribute("href") ?? "").toBe("");
    const ok = links.find((a) => a.textContent === "ok");
    expect(ok?.getAttribute("href")).toBe("https://a.b");
    expect(ok?.getAttribute("rel")).toContain("noopener");
  });

  it("safeUrl solo deja http(s), mailto y rutas internas", () => {
    expect(safeUrl("https://x.y")).toBe("https://x.y");
    expect(safeUrl("mailto:a@b.c")).toBe("mailto:a@b.c");
    expect(safeUrl("/notas/1")).toBe("/notas/1");
    expect(safeUrl("//evil.com")).toBe("");
    expect(safeUrl("javascript:alert(1)")).toBe("");
    expect(safeUrl("data:text/html,x")).toBe("");
  });
});

describe("Listas y rutas por entidad", () => {
  it("parseList quita vacíos, # y duplicados sin distinguir mayúsculas", () => {
    expect(parseList(" #rust, tool/git ,, Rust , fe")).toEqual(["rust", "tool/git", "fe"]);
  });

  it("ListField emite la lista conforme se escribe", () => {
    render(<ListHarness />);
    fireEvent.change(screen.getByLabelText("Tags"), { target: { value: "a, b, a" } });
    expect(screen.getByRole("status").textContent).toBe("a|b");
  });

  it("entityHref abre cada tipo en su pantalla", () => {
    expect(entityHref("note", "1")).toBe("/notas/1");
    expect(entityHref("person", "2")).toBe("/personas/2");
    expect(entityHref("task", "3")).toBe("/tareas?task=3");
    expect(entityHref("checkin", "4", "2026-11-12")).toBe("/hoy?date=2026-11-12");
    expect(entityHref("weekly_review", "5", null, "2026-W46")).toBe("/semanal?week=2026-W46");
    expect(entityHref("adr", "6")).toBe("/dev/adr/6");
    expect(entityHref("class_note", "7")).toBe("/escuela/clase/7");
  });
});

describe("Contenido", () => {
  it("siempre ofrece 10 hooks y compacta conservando el elegido", () => {
    expect(padHooks(["a"])).toHaveLength(10);
    expect(padHooks(Array.from({ length: 12 }, (_, i) => `${i}`))).toHaveLength(10);
    const hooks = ["", "uno", "", "dos", "", "", "", "", "", ""];
    expect(compactHooks(hooks, 3)).toEqual({ hooks: ["uno", "dos"], chosen_hook: 1 });
    expect(compactHooks(hooks, 2)).toEqual({ hooks: ["uno", "dos"], chosen_hook: null });
  });

  it("días para publicar como en Pipeline.base", () => {
    expect(daysToPublishLabel(0)).toBe("hoy");
    expect(daysToPublishLabel(3)).toBe("en 3 días");
    expect(daysToPublishLabel(-2)).toBe("hace 2 días");
    expect(daysToPublishLabel(null)).toBe("—");
  });
});

describe("Personas", () => {
  it("cumpleaños con o sin año", () => {
    expect(birthdayString({ birthday_month: 4, birthday_day: 23, birthday_year: 1990 })).toBe("1990-04-23");
    expect(birthdayString({ birthday_month: 4, birthday_day: 3, birthday_year: null })).toBe("04-03");
    expect(birthdayString({ birthday_month: null, birthday_day: null, birthday_year: null })).toBe("");
    expect(validBirthday("02-29")).toBe(true);
    expect(validBirthday("2026-02-29")).toBe(false);
    expect(validBirthday("2028-02-29")).toBe(true);
    expect(validBirthday("13-01")).toBe(false);
    expect(validBirthday("1990/04/23")).toBe(false);
    expect(validBirthday("")).toBe(true);
  });
});

describe("Tareas", () => {
  it("describe reglas recurrentes incluyendo meses cortos", () => {
    expect(ruleLabel({ day_kind: "day", day: 14 })).toBe("Día 14 de cada mes");
    expect(ruleLabel({ day_kind: "day", day: 31 })).toContain("el último si el mes es más corto");
    expect(ruleLabel({ day_kind: "last" })).toBe("Último día del mes");
    expect(ruleLabel({ day_kind: "penultimate" })).toBe("Penúltimo día del mes");
  });

  it("taskBody conserva ID de versión y personas al editar (arrastre no crea tareas)", () => {
    const t = {
      id: "t1",
      title: "Pagar luz",
      notes_md: "",
      status: "open",
      version: 7,
      person_ids: ["p1"],
      planned_on: "2026-11-11",
      created_at: "",
      updated_at: "",
    };
    const b = taskBody(t, { planned_on: "2026-11-12" });
    expect(b.version).toBe(7);
    expect(b.person_ids).toEqual(["p1"]);
    expect(b.planned_on).toBe("2026-11-12");
  });
});

describe("Inbox", () => {
  it("convierte la propuesta en una acción editable y no asume destino sin propuesta", () => {
    expect(actionFromProposal("Llamar al dentista", null)).toMatchObject({
      type: "task",
      title: "Llamar al dentista",
    });
    expect(actionFromProposal("idea", { destination: "note", title: "Idea", area_id: "a1" })).toMatchObject({
      type: "note",
      title: "Idea",
      body_md: "idea",
      area_id: "a1",
    });
    expect(actionFromProposal("Juan Pérez", { destination: "person" })).toMatchObject({
      type: "person",
      name: "Juan Pérez",
    });
    expect(actionFromProposal("x", { destination: "discard" })).toEqual({ type: "discard" });
  });
});

describe("Notas", () => {
  it("noteInput conserva archivado y versión", () => {
    const n = {
      id: "n",
      kind: "snippet",
      title: "t",
      body_md: "<b>no html</b>",
      aliases: [],
      tags: ["lang/rust"],
      metadata: {},
      archived_at: "2026-11-12T00:00:00Z",
      version: 3,
      created_at: "",
      updated_at: "",
    };
    const i = noteInput(n, { title: "x" });
    expect(i).toMatchObject({ kind: "snippet", title: "x", archived: true, version: 3, tags: ["lang/rust"] });
  });
});

describe("Búsqueda", () => {
  it("construye la consulta omitiendo filtros vacíos", () => {
    const q = searchQuery(new URLSearchParams("q=canción&types=note,task&area=&archived=1"));
    expect(q).toMatchObject({ q: "canción", types: "note,task", area_id: undefined, archived: true });
    expect(searchQuery(new URLSearchParams("q=x")).archived).toBeUndefined();
  });
});

describe("Dibujos", () => {
  it("guarda solo elementos vivos y archivos usados", () => {
    const s = sceneFromExcalidraw(
      [{ id: "a" }, { id: "b", isDeleted: true }, { id: "c", fileId: "f1" }],
      { viewBackgroundColor: "#000", zoom: 2, selectedElementIds: { a: true } },
      { f1: { id: "f1" }, f2: { id: "f2" } },
    );
    expect(s.elements).toHaveLength(2);
    expect(Object.keys(s.files ?? {})).toEqual(["f1"]);
    expect(s.appState).toEqual({ viewBackgroundColor: "#000" });
    expect(sceneSize(s)).toBeGreaterThan(10);
    expect(asScene(null)).toEqual({ elements: [], appState: {}, files: {} });
  });
});

describe("Agenda", () => {
  it("muestra horas en la zona del usuario", () => {
    // 16:00 UTC = 10:00 en Monterrey (UTC−6).
    expect(localTime("2026-11-12T16:00:00Z", "America/Monterrey")).toBe("10:00");
    const e = {
      id: "1",
      source: "manual",
      calendar_id: "manual",
      on_date: "2026-11-12",
      starts_at: "2026-11-12T16:00:00Z",
      ends_at: "2026-11-12T17:30:00Z",
      all_day: false,
      title: "Clase",
      attendees: [],
      status: "confirmed",
      updated_at: "",
    };
    expect(eventLine(e, "America/Monterrey")).toBe("10:00–11:30 Clase");
    expect(eventLine({ ...e, all_day: true }, "America/Monterrey")).toBe("Todo el día · Clase");
  });
});
