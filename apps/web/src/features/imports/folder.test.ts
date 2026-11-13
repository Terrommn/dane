import { describe, expect, it } from "vitest";
import { isText, parseResult, parseSummary, readFolder, shouldSkip } from "./folder";

const file = (path: string, content = "x") => ({
  name: path.split("/").pop() ?? path,
  webkitRelativePath: path,
  size: content.length,
  text: async () => content,
});

describe("lectura de carpeta del vault", () => {
  it("omite configuración y plugins, envía texto y registra binarios como referencia", async () => {
    const sel = await readFolder([
      file("Vault/.obsidian/plugins/dataview/main.js", "código"),
      file("Vault/.DS_Store"),
      file("Vault/Daily/2026-11-12.md", "---\nfecha: 2026-11-12\n---"),
      file("Vault/Bases/Hábitos.base", "views: []"),
      file("Vault/Attachments/foto.png", "binario"),
      file("Vault/.git/HEAD"),
    ]);
    expect(sel.name).toBe("Vault");
    expect(sel.skipped).toBe(3);
    expect(sel.files).toEqual([
      { path: "Vault/Daily/2026-11-12.md", content: "---\nfecha: 2026-11-12\n---" },
      { path: "Vault/Bases/Hábitos.base", content: "views: []" },
      { path: "Vault/Attachments/foto.png", content: null },
    ]);
  });

  it("clasifica rutas", () => {
    expect(shouldSkip("V/.obsidian/app.json")).toBe(true);
    expect(shouldSkip("V/Notas/obsidian.md")).toBe(false);
    expect(isText("a/b.MD")).toBe(true);
    expect(isText("a/b.js")).toBe(false);
  });
});

describe("resumen del dry-run", () => {
  it("lee el resumen de simulación", () => {
    const s = parseSummary({
      total: 5,
      create: 3,
      link: 1,
      unchanged: 1,
      conflicts: 0,
      references: 2,
      ignored: 9,
      warnings: 1,
      by_entity: { checkin: { create: 2, unchanged: 1 }, person: { create: 1, link: 1 } },
    });
    expect(s.create).toBe(3);
    expect(s.byEntity).toEqual([
      { entity: "checkin", actions: { create: 2, unchanged: 1 } },
      { entity: "person", actions: { create: 1, link: 1 } },
    ]);
  });

  it("lee el resumen después de aplicar ({ plan, result })", () => {
    const raw = {
      plan: { total: 2, create: 2 },
      result: { created: 2, linked: 0, skipped: 0, conflicts: 0 },
    };
    expect(parseSummary(raw).create).toBe(2);
    expect(parseResult(raw)).toEqual({ created: 2, linked: 0, skipped: 0, conflicts: 0 });
    expect(parseResult({ total: 1 })).toBeNull();
  });

  it("un vault vacío (solo esqueleto) no tiene nada que crear", () => {
    const s = parseSummary({ total: 0, create: 0, link: 0, ignored: 60 });
    expect(s.create + s.link).toBe(0);
    expect(s.ignored).toBe(60);
  });
});
