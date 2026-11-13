import { describe, expect, it } from "vitest";
import {
  connectionInfo,
  DEFAULT_STATUS_MAP,
  googleConfig,
  googleReturnMessage,
  groupClickupTasks,
  rowsToStatusMap,
  statusMapToRows,
} from "./status";

describe("mapa de estados de ClickUp", () => {
  it("usa el mapa del vault si no hay configuración", () => {
    const rows = statusMapToRows(undefined);
    expect(rows).toContainEqual({ status: "new", category: "open" });
    expect(rows).toContainEqual({ status: "need info", category: "need_info" });
    expect(Object.keys(DEFAULT_STATUS_MAP).length).toBe(rows.length);
  });

  it("normaliza a minúsculas, omite vacíos y detecta repetidos", () => {
    const { map, errors } = rowsToStatusMap([
      { status: " New ", category: "open" },
      { status: "", category: "closed" },
      { status: "new", category: "need_info" },
    ]);
    expect(map).toEqual({ new: "need_info" });
    expect(errors.some((e) => e.includes("repetido"))).toBe(true);
    expect(errors.some((e) => e.includes("Abierta"))).toBe(true);
  });

  it("ignora categorías desconocidas que vengan del servidor", () => {
    expect(statusMapToRows({ new: "open", raro: "otro" })).toEqual([{ status: "new", category: "open" }]);
  });
});

describe("tareas en orden de la daily", () => {
  it("agrupa fáciles → asignadas → need info y omite grupos vacíos", () => {
    const g = groupClickupTasks([
      { id: 1, group: "need_info" },
      { id: 2, group: "easy" },
      { id: 3, group: "assigned" },
      { id: 4, group: "easy" },
      { id: 5, group: "desconocido" },
    ]);
    expect(g.map((x) => x.key)).toEqual(["easy", "assigned", "need_info"]);
    expect(g[0]?.items.map((x) => x.id)).toEqual([2, 4]);
    expect(g[1]?.items.map((x) => x.id)).toEqual([3, 5]);
  });
});

describe("estado y configuración", () => {
  it("etiqueta estados de conexión", () => {
    expect(connectionInfo("error")).toEqual({ label: "Error", tone: "danger" });
    expect(connectionInfo(undefined).label).toBe("Desconectado");
  });

  it("lee la configuración de Google con valores por defecto activos", () => {
    expect(googleConfig({ calendars: ["a", 3], gmail_enabled: false })).toEqual({
      calendars: ["a"],
      calendarEnabled: true,
      gmailEnabled: false,
    });
  });

  it("explica el regreso del OAuth", () => {
    expect(googleReturnMessage("connected")?.tone).toBe("ok");
    expect(googleReturnMessage("error")?.tone).toBe("error");
    expect(googleReturnMessage(null)).toBeNull();
  });
});
