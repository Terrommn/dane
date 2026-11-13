import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { formatMetric } from "../dashboard/DashboardPage";
import type { HabitMark } from "../today/checkin";
import { displayStatus } from "../today/TodayPage";
import { HabitStatusControl } from "./HabitStatusControl";

vi.mock("../auth/AuthProvider", () => ({ useAuth: () => ({ me: null, userId: null }) }));

afterEach(cleanup);

function Harness({ onChange }: { onChange: (m: HabitMark) => void }) {
  const [mark, setMark] = useState<HabitMark>({ status: null, reason: "", quantity: "" });
  return (
    <HabitStatusControl
      name="Leer"
      unit="min"
      mark={mark}
      onChange={(m) => {
        setMark(m);
        onChange(m);
      }}
    />
  );
}

describe("HabitStatusControl", () => {
  it("'sin registrar' es el estado inicial y es distinto de 'No'", () => {
    render(<Harness onChange={() => undefined} />);
    expect((screen.getByLabelText(/Sin registrar/) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^✕?\s*No$/) as HTMLInputElement).checked).toBe(false);
  });

  it("'No aplica' pide motivo y 'Cumplido' con unidad pide cantidad", () => {
    const changes: HabitMark[] = [];
    render(<Harness onChange={(m) => changes.push(m)} />);
    fireEvent.click(screen.getByLabelText(/No aplica/));
    const reason = screen.getByLabelText(/¿Por qué no aplica\?/);
    expect(reason.getAttribute("aria-invalid")).toBe("true");
    fireEvent.change(reason, { target: { value: "Viaje" } });
    expect(changes.at(-1)).toEqual({ status: "not_applicable", reason: "Viaje", quantity: "" });
    fireEvent.click(screen.getByLabelText(/Cumplido/));
    fireEvent.change(screen.getByLabelText("Cantidad (min)"), { target: { value: "20" } });
    expect(changes.at(-1)?.status).toBe("done");
    expect(changes.at(-1)?.quantity).toBe("20");
  });
});

describe("estado visible del guardado", () => {
  it("conflicto, error y pendiente mandan sobre borrador; guardado solo sin cambios locales", () => {
    expect(displayStatus("conflict", true)).toBe("conflict");
    expect(displayStatus("error", true)).toBe("error");
    expect(displayStatus("pending", true)).toBe("pending");
    expect(displayStatus("saved", true)).toBe("draft");
    expect(displayStatus("saved", false)).toBe("saved");
    expect(displayStatus(null, true)).toBe("draft");
    expect(displayStatus(null, false)).toBeNull();
  });
});

describe("formatMetric (comparaciones del dashboard)", () => {
  it("formatea por unidad y no inventa valores", () => {
    expect(formatMetric(null, "kg")).toBe("—");
    expect(formatMetric(72.44, "kg")).toBe("72.4 kg");
    expect(formatMetric(86, "pct")).toBe("86%");
    expect(formatMetric(125050, "cents")).toContain("1,250.50");
    expect(formatMetric(3, "count")).toBe("3");
  });
});
