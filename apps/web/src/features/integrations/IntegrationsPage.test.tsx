import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../../components/ui/Toast";

const srv = vi.hoisted(() => ({
  overview: {} as Record<string, unknown>,
  tasks: [] as unknown[],
}));

vi.mock("../../lib/api", () => {
  const ok = (data: unknown) => ({ data, error: undefined, response: new Response(null, { status: 200 }) });
  return {
    api: {
      GET: async (path: string) => {
        if (path === "/integrations") return ok(srv.overview);
        if (path === "/integrations/clickup/tasks") return ok(srv.tasks);
        if (path === "/integrations/google/calendars")
          return ok([
            { id: "primary", summary: "Personal", primary: true, excluded_by_rule: false, selected: true },
            {
              id: "es.mexican#holiday",
              summary: "Festivos",
              primary: false,
              excluded_by_rule: true,
              selected: false,
            },
          ]);
        if (path === "/integrations/suggestions") return ok({ interactions: [], people: [] });
        if (path === "/areas") return ok([]);
        return { data: undefined, error: {}, response: new Response(null, { status: 404 }) };
      },
    },
  };
});

const { IntegrationsPage } = await import("./IntegrationsPage");
afterEach(cleanup);

const conn = (provider: string, status: string, extra: Record<string, unknown> = {}) => ({
  provider,
  status,
  scopes: [],
  config: {},
  version: 1,
  ...extra,
});

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/ajustes/integraciones"]}>
          <IntegrationsPage />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("IntegrationsPage", () => {
  it("muestra estado desconectado, falta de credenciales y modo manual de IA", async () => {
    srv.overview = {
      google: conn("google", "disconnected"),
      clickup: conn("clickup", "disconnected"),
      google_available: false,
      google_redirect_uri: "https://dane.example/api/v1/integrations/google/callback",
      llm: { configured: false, mode: "manual" },
      recent_runs: [],
    };
    renderPage();
    expect(await screen.findByText(/no tiene credenciales de Google/)).toBeTruthy();
    expect(screen.getByText("https://dane.example/api/v1/integrations/google/callback")).toBeTruthy();
    expect(screen.getByText("Modo manual")).toBeTruthy();
    expect(screen.getByText(/Marcar una tarea aquí no la cambia en ClickUp/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Conectar ClickUp" })).toBeTruthy();
  });

  it("con Google en error muestra el último error y excluye festivos; ClickUp ordena tareas", async () => {
    srv.overview = {
      google: conn("google", "error", { last_error: "token revocado", config: { calendars: ["primary"] } }),
      clickup: conn("clickup", "connected", { config: { view_id: "v1" } }),
      google_available: true,
      google_redirect_uri: "x",
      llm: { configured: true, model: "claude-haiku-4-5-20251001", mode: "llm" },
      recent_runs: [],
    };
    srv.tasks = [
      { task_id: "1", external_id: "a", title: "Bloqueada", group: "need_info", status: "need info" },
      {
        task_id: "2",
        external_id: "b",
        title: "Cambiar texto",
        group: "easy",
        reason: "texto",
        status: "new",
      },
    ];
    renderPage();
    expect(await screen.findByText(/token revocado/)).toBeTruthy();
    const festivos = (await screen.findByLabelText("Festivos")) as HTMLInputElement;
    expect(festivos.disabled).toBe(true);
    expect(festivos.checked).toBe(false);
    expect(((await screen.findByLabelText("Personal (principal)")) as HTMLInputElement).checked).toBe(true);
    const easy = await screen.findByText("Fáciles para hoy");
    const blocked = screen.getByText("Need info (bloqueadas)");
    expect(easy.compareDocumentPosition(blocked) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("🟢 fácil: texto")).toBeTruthy();
  });
});
