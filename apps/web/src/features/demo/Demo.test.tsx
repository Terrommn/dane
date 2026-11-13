import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const auth = vi.hoisted(() => ({
  me: null as { user: { is_demo: boolean } } | null,
  logout: vi.fn(async () => {}),
  completeDevLogin: vi.fn(async () => {}),
  status: 404,
}));

vi.mock("../auth/AuthProvider", () => ({
  useAuth: () => ({ me: auth.me, logout: auth.logout, completeDevLogin: auth.completeDevLogin }),
}));
vi.mock("../../lib/api", () => ({
  api: {
    POST: async () =>
      auth.status === 404
        ? {
            data: undefined,
            error: { error: { code: "not_found", message: "No" } },
            response: new Response(null, { status: 404 }),
          }
        : {
            data: { user_id: "demo-1", is_demo: true },
            error: undefined,
            response: new Response(null, { status: 200 }),
          },
  },
}));

const { DemoBanner, DemoEntry } = await import("./Demo");

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.setAttribute("open", "");
};
afterEach(cleanup);

describe("Datos demo", () => {
  it("el banner aparece solo en sesión demo y permite salir con confirmación", async () => {
    auth.me = { user: { is_demo: false } };
    const { rerender } = render(<DemoBanner />);
    expect(screen.queryByText("Modo demo — datos ficticios")).toBeNull();
    auth.me = { user: { is_demo: true } };
    rerender(<DemoBanner />);
    expect(screen.getByText("Modo demo — datos ficticios")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Salir del demo" }));
    fireEvent.click(await screen.findByRole("button", { name: "Salir" }));
    await waitFor(() => expect(auth.logout).toHaveBeenCalledOnce());
  });

  it("la opción se oculta si el servidor no tiene demo habilitado (404)", async () => {
    auth.status = 404;
    render(<DemoEntry redirectTo="/hoy" />);
    fireEvent.click(screen.getByRole("button", { name: "Explorar con datos demo" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Explorar con datos demo" })).toBeNull());
    expect(auth.completeDevLogin).not.toHaveBeenCalled();
  });

  it("entra al espacio demo aislado cuando está habilitado", async () => {
    auth.status = 200;
    render(<DemoEntry redirectTo="/hoy" />);
    fireEvent.click(screen.getByRole("button", { name: "Explorar con datos demo" }));
    await waitFor(() => expect(auth.completeDevLogin).toHaveBeenCalledWith("demo-1"));
  });
});
