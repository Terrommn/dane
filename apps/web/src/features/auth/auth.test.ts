import { afterEach, describe, expect, it } from "vitest";
import { closeAll } from "../../lib/offline/db";
import { enqueue, pendingCount, saveDraft } from "../../lib/offline/store";
import { detectPlatform } from "../install/InstallPage";
import { isSafeUrl } from "../projects/ProjectDetailPage";
import { clearLocalUserData } from "./AuthProvider";
import { safeRedirect } from "./LoginPage";

afterEach(closeAll);

describe("cierre de sesión", () => {
  it("borra borradores y cola del usuario y no toca los de otro", async () => {
    await saveDraft("u1", "checkin", "2026-11-12", { energy: 3 }, null);
    await enqueue("u1", { method: "POST", path: "/inbox", body: { text: "a" } });
    await enqueue("u2", { method: "POST", path: "/inbox", body: { text: "b" } });
    expect(await pendingCount("u1")).toBe(1);
    await clearLocalUserData("u1");
    expect(await pendingCount("u1")).toBe(0);
    expect(await pendingCount("u2")).toBe(1);
    await clearLocalUserData("u2");
  });
});

describe("redirecciones y URLs seguras", () => {
  it("solo rutas internas tras el login", () => {
    expect(safeRedirect("/hoy?date=2026-11-11")).toBe("/hoy?date=2026-11-11");
    expect(safeRedirect("//evil.com")).toBe("/");
    expect(safeRedirect("https://evil.com")).toBe("/");
    expect(safeRedirect("/\\evil.com")).toBe("/");
    expect(safeRedirect(null)).toBe("/");
  });

  it("enlaces de proyecto solo http(s)", () => {
    expect(isSafeUrl("https://github.com/x/y")).toBe(true);
    expect(isSafeUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeUrl("https://a b")).toBe(false);
  });

  it("detecta plataforma para instrucciones de instalación", () => {
    expect(detectPlatform("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)")).toBe("ios");
    expect(detectPlatform("Mozilla/5.0 (Linux; Android 15)")).toBe("android");
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe("desktop");
  });
});
