import { afterEach, describe, expect, it } from "vitest";
import { applySafeAreas, applyTelegramTheme, isTelegramLaunch, isValidIsoDate, parseStartParam, resolveTargetDate, startTargetPath, type TgWebApp, validWebApp } from "./telegram";

afterEach(() => {
  delete window.Telegram;
});

describe("start_param y fecha objetivo", () => {
  it("parsea el check-in de un mensaje anterior", () => {
    expect(parseStartParam("checkin_2026-11-11")).toEqual({ screen: "checkin", date: "2026-11-11", ref: null });
    expect(startTargetPath(parseStartParam("checkin_2026-11-11"))).toBe("/hoy?date=2026-11-11");
  });

  it("rechaza fechas imposibles y parámetros desconocidos", () => {
    expect(parseStartParam("checkin_2026-02-30")).toEqual({ screen: "checkin", date: null, ref: null });
    expect(parseStartParam("hack_x")).toBeNull();
    expect(parseStartParam("")).toBeNull();
    expect(isValidIsoDate("2028-02-29")).toBe(true);
    expect(isValidIsoDate("2026-02-29")).toBe(false);
  });

  it("otras pantallas", () => {
    expect(startTargetPath(parseStartParam("semanal_2026-W46"))).toBe("/semanal?week=2026-W46");
    expect(startTargetPath(parseStartParam("semanal_bad"))).toBe("/semanal");
    expect(startTargetPath(parseStartParam("captura_123e4567-e89b-12d3-a456-426614174000"))).toBe("/captura/123e4567-e89b-12d3-a456-426614174000");
    expect(startTargetPath(parseStartParam("dashboard"))).toBe("/dashboard");
  });

  it("?date de la URL tiene prioridad sobre el start_param", () => {
    expect(resolveTargetDate("?date=2026-11-10", "checkin_2026-11-11")).toBe("2026-11-10");
    expect(resolveTargetDate("?date=basura", "checkin_2026-11-11")).toBe("2026-11-11");
    expect(resolveTargetDate("", null)).toBeNull();
  });
});

describe("contexto de Telegram", () => {
  it("solo detecta lanzamiento con parámetros de Telegram", () => {
    expect(isTelegramLaunch({ hash: "#tgWebAppData=query_id%3D1", search: "" })).toBe(true);
    expect(isTelegramLaunch({ hash: "", search: "?tgWebAppStartParam=x" })).toBe(true);
    expect(isTelegramLaunch({ hash: "", search: "?date=2026-11-11" })).toBe(false);
  });

  it("sin initData firmado no se considera Mini App válida", () => {
    window.Telegram = { WebApp: { initData: "" } as TgWebApp };
    expect(validWebApp()).toBeNull();
    window.Telegram = { WebApp: { initData: "auth_date=1&hash=x" } as TgWebApp };
    expect(validWebApp()).not.toBeNull();
  });

  it("aplica tema y safe areas como variables CSS", () => {
    const root = document.createElement("div");
    const wa = {
      initData: "x",
      themeParams: { bg_color: "#111111", button_color: "#22aa22" },
      colorScheme: "dark",
      safeAreaInset: { top: 20, bottom: 10, left: 0, right: 0 },
      contentSafeAreaInset: { top: 44, bottom: 0, left: 0, right: 0 },
    } as unknown as TgWebApp;
    applyTelegramTheme(wa, root);
    applySafeAreas(wa, root);
    expect(root.style.getPropertyValue("--tg-bg")).toBe("#111111");
    expect(root.style.getPropertyValue("--tg-button")).toBe("#22aa22");
    expect(root.dataset.theme).toBe("dark");
    expect(root.style.getPropertyValue("--safe-top")).toBe("64px");
    expect(root.style.getPropertyValue("--safe-bottom")).toBe("10px");
  });
});
