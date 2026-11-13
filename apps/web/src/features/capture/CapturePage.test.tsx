import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../../components/ui/Toast";

// API simulada a nivel de módulo (el Request de Node no acepta las URL relativas del cliente real).
const server = vi.hoisted(() => {
  const state = {
    capture: null as Record<string, unknown> | null,
    calls: [] as { method: string; path: string; body?: unknown }[],
  };
  const ok = (data: unknown, status = 200) => ({
    data,
    error: undefined,
    response: new Response(null, { status }),
  });
  const fail = (status: number, error: unknown) => ({
    data: undefined,
    error,
    response: new Response(null, { status }),
  });
  const handle = (method: string, path: string, opts?: { body?: unknown }) => {
    state.calls.push({ method, path, body: opts?.body });
    const c = state.capture as Record<string, unknown>;
    if (path === "/captures/{id}" && method === "GET") return ok(c);
    if (path === "/captures/{id}" && method === "PUT") {
      const b = opts?.body as { version: number; proposal: unknown };
      if (b.version !== c.version)
        return fail(409, { error: { code: "conflict", message: "Cambió", current: c } });
      state.capture = {
        ...c,
        proposal: b.proposal,
        version: (c.version as number) + 1,
        status: "pending",
        questions: [],
      };
      return ok(state.capture);
    }
    if (path === "/captures/{id}/apply") {
      const b = opts?.body as { version: number };
      if (b.version !== c.version)
        return fail(409, { error: { code: "conflict", message: "Cambió", current: c } });
      state.capture = { ...c, status: "applied", applied: { created: [{ type: "transaction", id: "t1" }] } };
      return ok(state.capture);
    }
    if (path === "/captures/{id}/reject") {
      state.capture = { ...c, status: "rejected" };
      return ok(state.capture);
    }
    if (path === "/finance/categories")
      return ok([
        { id: "c1", name: "Gasolina", kind: "expense", role: "consumption", position: 0 },
        { id: "c2", name: "Sueldo", kind: "income", role: "earned", position: 0 },
      ]);
    if (path === "/finance/accounts") return ok([{ account: { name: "Débito Banco" } }]);
    if (path === "/areas") return ok([{ id: "a1", name: "Personal" }]);
    if (path === "/people") return ok([{ name: "Ana López" }]);
    return fail(404, { error: { code: "not_found", message: "No encontrado" } });
  };
  return { state, handle };
});

vi.mock("../../lib/api", () => ({
  api: {
    GET: async (path: string, opts?: { body?: unknown }) => server.handle("GET", path, opts),
    PUT: async (path: string, opts?: { body?: unknown }) => server.handle("PUT", path, opts),
    POST: async (path: string, opts?: { body?: unknown }) => server.handle("POST", path, opts),
    DELETE: async (path: string, opts?: { body?: unknown }) => server.handle("DELETE", path, opts),
  },
  apiFetch: vi.fn(),
  newIdempotencyKey: () => "k",
}));

const { CapturePage } = await import("./CapturePage");

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.removeAttribute("open");
};

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={["/captura/cap-1"]}>
          <Routes>
            <Route path="/captura/:id" element={<CapturePage />} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

const base = {
  id: "cap-1",
  intent: "gasto",
  input_text: "250 de gasolina",
  source: "telegram",
  reference_date: "2026-11-12",
  status: "needs_input",
  mode: "manual",
  proposal: { expenses: [{ concept: "Gasolina", amount: "250", date: null, category: "Gasolina" }] },
  questions: [{ field: "expenses[0].date", message: "Falta date en «Gasolina»" }],
  errors: ["La IA no está configurada: completa la propuesta manualmente."],
  applied: null,
  llm_configured: false,
  llm_model: null,
  allowed_fields: ["expenses"],
  version: 3,
};

beforeEach(() => {
  server.state.capture = structuredClone(base);
  server.state.calls = [];
});
afterEach(cleanup);

describe("CapturePage", () => {
  it("muestra modo manual, el texto original y la pregunta de campo ambiguo", async () => {
    renderPage();
    expect(await screen.findByText("Modo manual")).toBeTruthy();
    expect(screen.getByText("250 de gasolina")).toBeTruthy();
    expect(screen.getAllByText("Falta date en «Gasolina»").length).toBeGreaterThan(0);
    // Solo la sección permitida para /gasto.
    expect(screen.getByRole("group", { name: "Gastos" })).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Pendientes" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Check-in del día" })).toBeNull();
    expect(screen.getByLabelText("Fecha del gasto").getAttribute("aria-invalid")).toBe("true");
  });

  it("guarda la edición y aplica solo tras confirmar, con la versión vigente", async () => {
    renderPage();
    const date = await screen.findByLabelText("Fecha del gasto");
    fireEvent.change(date, { target: { value: "2026-11-11" } });
    expect(screen.getByText("Cambios sin guardar")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Aplicar…" }));
    const dialog = await screen.findByRole("dialog", { name: "¿Aplicar esta captura?" });
    expect(within(dialog).getByText("1 gasto")).toBeTruthy();
    expect(server.state.calls.some((c) => c.path.endsWith("/apply"))).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Sí, aplicar" }));
    await waitFor(() => expect(server.state.capture?.status).toBe("applied"));
    const put = server.state.calls.find((c) => c.method === "PUT")?.body as
      | { version: number; proposal: { expenses: { date: string }[] } }
      | undefined;
    expect(put?.version).toBe(3);
    const sent = put?.proposal.expenses[0];
    expect(sent?.date).toBe("2026-11-11");
    const apply = server.state.calls.find((c) => c.path.endsWith("/apply"));
    expect(apply?.body).toEqual({ version: 4 });
    expect(await screen.findByRole("link", { name: "Movimiento" })).toBeTruthy();
  });

  it("bloquea aplicar si un monto es inválido", async () => {
    renderPage();
    const amount = await screen.findByLabelText("Monto");
    fireEvent.change(amount, { target: { value: "1.999" } });
    expect(screen.getByText("Monto inválido (ej. 180.50)")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Aplicar…" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("descartar pide confirmación y no aplica nada", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Descartar" }));
    const dialog = await screen.findByRole("dialog", { name: "¿Descartar la captura?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Descartar" }));
    await waitFor(() => expect(server.state.capture?.status).toBe("rejected"));
    expect(server.state.calls.some((c) => c.path.endsWith("/apply"))).toBe(false);
  });

  it("ante un 409 ofrece elegir versión sin sobrescribir en silencio", async () => {
    renderPage();
    const amount = await screen.findByLabelText("Monto");
    // Otra sesión guardó antes.
    server.state.capture = { ...(server.state.capture as object), version: 9 } as Record<string, unknown>;
    fireEvent.change(amount, { target: { value: "300" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cambios" }));
    expect(await screen.findByRole("dialog", { name: "La captura cambió en otro dispositivo" })).toBeTruthy();
  });
});
