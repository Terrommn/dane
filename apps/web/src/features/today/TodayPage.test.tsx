// Integración de la pantalla Hoy con la cola offline real (IndexedDB simulado) y un servidor simulado vía fetch:
// guardado parcial (solo campos cambiados + versión + Idempotency-Key), "sin dato" ≠ 0, sin red → pendiente, y 409 →
// diálogo de conflicto explícito.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../../components/ui/Toast";
import { deleteUserDb } from "../../lib/offline/db";
import { TodayPage } from "./TodayPage";

// openapi-fetch captura `fetch` al crear el cliente (al importar): se instala antes de los imports un fetch que delega
// en el simulador de cada prueba, y un Request que acepta URLs relativas ("/api/v1/...").
const net = vi.hoisted(() => {
  const state: { impl: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> } = {
    impl: async () => {
      throw new TypeError("fetch no configurado");
    },
  };
  const Native = globalThis.Request;
  class RelativeRequest extends Native {
    constructor(input: RequestInfo | URL, init?: RequestInit) {
      super(typeof input === "string" ? new URL(input, "http://localhost") : input, init);
    }
  }
  globalThis.Request = RelativeRequest as typeof Request;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) =>
    state.impl(input, init)) as typeof fetch;
  return state;
});

const USER = "u-today-test";
vi.mock("../auth/AuthProvider", () => ({
  useAuth: () => ({
    userId: USER,
    me: {
      settings: {
        timezone: "America/Monterrey",
        currency: "MXN",
        modules: { finance: false },
        onboarding_completed_at: "2026-01-01T00:00:00Z",
      },
    },
  }),
}));

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.setAttribute("open", "");
};

const DATE = "2026-01-05";
const habit = {
  id: "h-ex",
  name: "Ejercicio",
  kind: "fixed",
  fixed_key: "exercise",
  description: "",
  position: 0,
  created_on: "2026-01-01",
  version: 1,
  updated_at: "",
};
function dayView(energy: number | null, version: number) {
  return {
    date: DATE,
    today: "2026-01-05",
    timezone: "America/Monterrey",
    iso_week: "2026-W02",
    month: "2026-01",
    is_open: false,
    version,
    checkin:
      version === 0
        ? null
        : {
            id: "c1",
            on_date: DATE,
            energy,
            planned_spend_none: false,
            notes_md: "",
            version,
            updated_at: "",
          },
    log: [],
    went_well: [],
    went_badly: [],
    habit_entries: [],
    habits: {
      fixed: [
        {
          habit,
          entry: null,
          progress: {
            date: DATE,
            shown: true,
            required_today: false,
            status: null,
            week_done: 1,
            week_target: 4,
          },
        },
      ],
      month: [],
      week: [],
      other: [],
    },
    month_goals: [],
    week_goals: [],
    agenda: [],
    tasks_by_area: [],
    people: [],
    derived: {
      closed_tasks: [],
      past_events: [],
      devlog_sessions: [],
      expenses_total_cents: 0,
      expenses_count: 0,
    },
    reminder_done: false,
    currency: "MXN",
    generated_at: "",
  };
}

type Handler = (req: {
  method: string;
  path: string;
  body: unknown;
  headers: Headers;
}) => Response | Promise<Response>;
let handler: Handler;
const puts: { body: Record<string, unknown>; headers: Headers }[] = [];
const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

beforeEach(() => {
  puts.length = 0;
  net.impl = async (input: RequestInfo | URL, init?: RequestInit) => {
    const req = input instanceof Request ? input : null;
    const url = new URL(req ? req.url : String(input), "http://localhost");
    const method = (req?.method ?? init?.method ?? "GET").toUpperCase();
    const headers = new Headers(req?.headers ?? init?.headers);
    const raw = req ? await req.clone().text() : ((init?.body as string | undefined) ?? "");
    const body = raw ? JSON.parse(raw) : null;
    if (method === "PUT") puts.push({ body, headers });
    return handler({ method, path: url.pathname, body, headers });
  };
});

afterEach(async () => {
  cleanup();
  // Deja terminar el guardado diferido del borrador (400 ms) antes de borrar la base local.
  await new Promise((r) => setTimeout(r, 450));
  await deleteUserDb(USER);
});

function renderToday() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <MemoryRouter initialEntries={[`/hoy?date=${DATE}`]}>
          <TodayPage />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe("Hoy (check-in)", () => {
  it("envía solo lo que cambió, con versión e Idempotency-Key; sin dato no es 0", async () => {
    let saved = false;
    handler = ({ method, path }) => {
      if (method === "GET" && path === `/api/v1/checkins/${DATE}/view`)
        return json(200, saved ? dayView(4, 1) : dayView(null, 0));
      if (method === "PUT" && path === `/api/v1/checkins/${DATE}`) {
        saved = true;
        const v = dayView(4, 1);
        return json(200, {
          checkin: v.checkin,
          version: 1,
          log: [],
          went_well: [],
          went_badly: [],
          habit_entries: [],
        });
      }
      return json(404, { error: { code: "not_found", message: "No encontrado" } });
    };
    renderToday();
    expect(await screen.findByText("Sin registrar (no es 0)")).toBeTruthy();
    expect(screen.getByText(/1\/4 esta semana/)).toBeTruthy();
    fireEvent.click(screen.getByText("4", { selector: "span" }));
    fireEvent.change(screen.getByLabelText("Peso (kg, opcional)"), { target: { value: "72,5" } });
    fireEvent.click(screen.getAllByLabelText(/Cumplido/)[0] as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(puts.length).toBe(1));
    const put = puts[0];
    expect(put?.body).toEqual({
      version: 0,
      energy: 4,
      weight_kg: "72.5",
      habits: [{ habit_id: "h-ex", status: "done", reason: null, quantity: null }],
    });
    expect(put?.headers.get("Idempotency-Key")).toBeTruthy();
    expect(await screen.findByText("Guardado", { selector: "[role=status] *, [role=status]" })).toBeTruthy();
  });

  it("sin red: queda pendiente en el dispositivo y nunca dice 'Guardado'", async () => {
    handler = ({ method }) => {
      if (method === "GET") return json(200, dayView(null, 0));
      throw new TypeError("Failed to fetch");
    };
    renderToday();
    await screen.findByText("Sin registrar (no es 0)");
    fireEvent.click(screen.getByText("2", { selector: "span" }));
    fireEvent.click(screen.getByRole("button", { name: "Guardar" }));
    expect(await screen.findByText("Pendiente de sincronizar")).toBeTruthy();
    expect(screen.queryByText("Guardado", { selector: "[role=status]" })).toBeNull();
  });

  it("409: muestra el conflicto con mi versión y la del servidor", async () => {
    handler = ({ method }) => {
      if (method === "GET") return json(200, dayView(null, 0));
      const current = dayView(2, 2);
      return json(409, {
        error: {
          code: "conflict",
          message: "El registro cambió en otro dispositivo",
          current: {
            checkin: current.checkin,
            version: 2,
            log: [],
            went_well: [],
            went_badly: [],
            habit_entries: [],
          },
        },
      });
    };
    renderToday();
    await screen.findByText("Sin registrar (no es 0)");
    fireEvent.click(screen.getByText("5", { selector: "span" }));
    fireEvent.click(screen.getByRole("button", { name: "Guardar" }));
    expect(await screen.findByText("Conflicto: revisa los cambios")).toBeTruthy();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByText("Energía").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("Mi versión").length).toBeGreaterThan(0);
    // Mi valor (5) y el del servidor (2), uno junto al otro; nada se guardó todavía.
    expect(within(dialog).getAllByText("5").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("2").length).toBeGreaterThan(0);
  });
});
