import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConflictDialog, diffFields } from "./ConflictDialog";
import { DataTable } from "./DataTable";
import { ProgressBar } from "./Display";
import { TextField } from "./Field";
import { SaveStatus } from "./SaveStatus";

afterEach(cleanup);

// jsdom no implementa <dialog>.showModal.
HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.removeAttribute("open");
};

describe("SaveStatus", () => {
  it("distingue borrador local, pendiente y guardado en servidor", () => {
    const { rerender } = render(<SaveStatus status="draft" />);
    expect(screen.getByRole("status").textContent).toContain("Borrador en este dispositivo");
    rerender(<SaveStatus status="pending" />);
    expect(screen.getByRole("status").textContent).toContain("Pendiente de sincronizar");
    rerender(<SaveStatus status="saved" />);
    expect(screen.getByRole("status").textContent).toContain("Guardado");
    rerender(<SaveStatus status="conflict" />);
    expect(screen.getByRole("status").textContent).toContain("Conflicto");
  });
});

describe("TextField", () => {
  it("asocia label y error accesibles", () => {
    render(<TextField label="Nombre" error="Es obligatorio" />);
    const input = screen.getByLabelText("Nombre");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    const describedBy = input.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(describedBy)?.textContent).toBe("Es obligatorio");
  });
});

describe("ProgressBar", () => {
  it("expone el avance como progressbar", () => {
    render(<ProgressBar label="Libros" value={3} max={4} valueText="3 de 4" />);
    const bar = screen.getByRole("progressbar", { name: "Libros" });
    expect(bar.getAttribute("aria-valuenow")).toBe("3");
    expect(bar.getAttribute("aria-valuetext")).toBe("3 de 4");
  });
});

describe("DataTable", () => {
  const rows = Array.from({ length: 25 }, (_, i) => ({ id: String(i), name: `Proyecto ${i}`, n: i }));
  rows.push({ id: "x", name: "Canción", n: 99 });
  const cols = [
    {
      key: "name",
      header: "Nombre",
      render: (r: (typeof rows)[number]) => r.name,
      sortValue: (r: (typeof rows)[number]) => r.name,
    },
    {
      key: "n",
      header: "N",
      render: (r: (typeof rows)[number]) => r.n,
      sortValue: (r: (typeof rows)[number]) => r.n,
    },
  ];

  it("filtra sin acentos, ordena y pagina", () => {
    render(
      <DataTable
        caption="Prueba"
        rows={rows}
        columns={cols}
        rowKey={(r) => r.id}
        searchText={(r) => r.name}
        pageSize={10}
      />,
    );
    expect(screen.getByText(/Página 1 de 3/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Filtrar"), { target: { value: "cancion" } });
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("row")).toHaveLength(2);
    expect(within(table).getByText("Canción")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Filtrar"), { target: { value: "" } });
    fireEvent.click(within(screen.getByRole("table")).getByRole("button", { name: "N" }));
    fireEvent.click(within(screen.getByRole("table")).getByRole("button", { name: "N" }));
    const first = within(screen.getByRole("table")).getAllByRole("row")[1];
    expect(first?.textContent).toContain("99");
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" }));
    expect(screen.getByText(/Página 2 de 3/)).toBeTruthy();
  });

  it("muestra estado vacío", () => {
    render(
      <DataTable caption="Vacía" rows={[]} columns={cols} rowKey={(r) => r.id} emptyTitle="Nada aquí" />,
    );
    expect(screen.getByText("Nada aquí")).toBeTruthy();
  });
});

describe("ConflictDialog", () => {
  it("detecta campos distintos ignorando metadatos", () => {
    expect(
      diffFields(
        { name: "A", version: 1, notes: "x" },
        { name: "B", version: 2, notes: "x", updated_at: "t" },
      ),
    ).toEqual(["name"]);
  });

  it("permite mezclar campo por campo", () => {
    const onResolve = vi.fn();
    render(
      <MemoryRouter>
        <ConflictDialog
          open
          mine={{ name: "Mío", notes: "nota mía" }}
          theirs={{ name: "Suyo", notes: "nota suya", version: 5 }}
          onResolve={onResolve}
          onClose={() => {}}
        />
      </MemoryRouter>,
    );
    const radios = screen.getAllByRole("radio", { hidden: true });
    // name: [mine, theirs], notes: [mine, theirs] → elegir "Suyo" para name.
    fireEvent.click(radios[1] as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: "Guardar mezcla", hidden: true }));
    expect(onResolve).toHaveBeenCalledWith({ choice: "merged", body: { name: "Suyo", notes: "nota mía" } });
  });
});
